// SPDX-License-Identifier: MIT OR Apache-2.0
//! The `levi` command-line interface.
//!
//! Surface: `request`, `run`, `serve --stdio` and `validate`. `request` runs the
//! **real protocol path** — it sends a `SendRequest` to an engine over an in-process
//! transport, rather than calling the transport directly. There is no shortcut here
//! on purpose: if the protocol is wrong, the CLI breaks first.
//!
//! `run` is the load path: it compiles a scenario against a collection and drives it
//! through `lev-load`, then prints a summary and (optionally) writes a JSON report.
//! `import` and `selftest` land in later phases.
#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use lev_core::{Collection, CoreError, Id, RequestDef};
use lev_engine::{convert, in_process_client, store_from_env, Engine, EngineError, RequestOutcome};
use lev_format::{
    load_collection, load_request, load_scenario, load_vars, resolve_body_files, FormatError, Yaml,
};
use lev_load::{CompiledScenario, JsonReporter, LoadError, Reporter, RunConfig};
use lev_metrics::{RunId, RunReport, StopReason, ThresholdResult, ThresholdUnit};
use lev_protocol::{EchoDto, ErrorCode, NdjsonTransport, ResponseDto, TimingsDto};
use tokio_util::sync::CancellationToken;

/// Use mimalloc as the global allocator: a load generator allocates constantly, and
/// a predictable allocator keeps generator-side jitter out of the histograms.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

// Exit codes are a contract for CI and are documented permanently (see README).
/// Success.
const EXIT_SUCCESS: u8 = 0;
/// A check or threshold failed. Precedence: a failed threshold outranks an abort,
/// because a failed threshold is the actionable signal CI gates on.
const EXIT_THRESHOLD: u8 = 1;
/// A configuration or usage error.
const EXIT_CONFIG: u8 = 2;
/// The engine failed to do its job.
const EXIT_ENGINE: u8 = 3;
/// The run was aborted (Ctrl-C, or `abortOn`) without a failed threshold to report.
const EXIT_ABORTED: u8 = 4;

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match run(&args).await {
        Ok(()) => ExitCode::from(EXIT_SUCCESS),
        Err(fail) => {
            eprintln!("error: {}", fail.message);
            ExitCode::from(fail.code)
        }
    }
}

fn usage() -> String {
    format!(
        "levi {version} 

USAGE:
    levi <COMMAND> [OPTIONS]

COMMANDS:
    request <file>       Send one request and print the response
    run <scenario>       Run a load scenario and print a report
    serve --stdio        Serve the protocol over stdin/stdout (NDJSON)
    validate <dir>       Parse a collection directory and report errors
    help                 Print this help
    version              Print the version

REQUEST OPTIONS:
    --collection <dir>   Collection directory (default: the request file's folder)
    --env <name>         Load environments/<name>.yaml
    --seed <n>           Seed for dynamic functions (uuid, timestamp, randomInt)
    --timeout <dur>      Deadline per request, e.g. 30s or 500ms. Wins over the
                         request's own `timeout:` [default: 30s]
    --retries <n>        Retry a transient failure n more times: 429/502/503/504,
                         a refused connection, a reset, a deadline. The wait
                         doubles each attempt, and `Retry-After` is honoured.
                         Idempotent methods only [default: 0]
    --retry-non-idempotent
                         Also retry POST/PATCH. A retried POST can send twice,
                         so it is never what `--retries` alone implies.
    --show-body          Print the (redacted) response body
    --max-body <bytes>   Truncate the printed body [default: 4096]

RUN OPTIONS:
    --collection <dir>   Collection directory (default: the scenario file's folder)
    --env <name>         Load environments/<name>.yaml
    --workers <n>        Worker threads [default: logical CPUs]
    --seed <n>           Base seed; each virtual user derives its own
    --timeout <dur>      Deadline for every request in the run. Wins over request
                         and scenario `timeout:` [default: 30s]
    --out <file>         Write a JSON report to <file>
    --quiet              Suppress the printed summary

EXIT CODES:
    0  success
    1  a check or threshold failed
    2  configuration or usage error
    3  engine error
    4  aborted (Ctrl-C or `abortOn`) with no failed threshold

Secrets are read from environment variables named LEVI_SECRET_<NAME>. The engine
redacts secret values before anything is printed or crosses the protocol. Pre- and
post-request scripts run in a sandbox with a CPU budget and a memory limit; checks
from `lev.test(...)` and from `assertions:` are reported and set the exit code.

`levi run` drives the closed (constant-VU) executor. A scenario's `thresholds` are
evaluated on the run's numbers and printed in the report; a threshold that fails exits
1. A metric listed in `abortOn` stops the run as soon as its threshold fails, and
Ctrl-C stops it gracefully with a partial report. Scripts and assertions are not yet
evaluated under load.

Requests are bounded: a send is cut off at its `timeout:` (or the run/flag default), so
a target that stops answering costs one iteration instead of hanging the tool. Retries go
the other way — spending a second request on a target that refused the first — so they are
off unless asked for, they never cover a method that is not safe to send twice unless
`--retry-non-idempotent` says so out loud, and `levi run` has none at all: a load report
counts every request the target saw, and a retry loop would fold three of them into one
sample. `levi serve --stdio` stops gracefully on Ctrl-C or SIGTERM — the request in flight is
answered, then the engine reports why it stopped and exits 0.
",
        version = env!("CARGO_PKG_VERSION"),
    )
}

/// A failure with the exit code it maps to.
struct Fail {
    message: String,
    code: u8,
}

impl Fail {
    fn config(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: EXIT_CONFIG,
        }
    }

    fn engine(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: EXIT_ENGINE,
        }
    }
}

impl From<FormatError> for Fail {
    fn from(error: FormatError) -> Self {
        Fail::config(error.to_string())
    }
}

impl From<CoreError> for Fail {
    fn from(error: CoreError) -> Self {
        Fail::config(error.to_string())
    }
}

impl From<EngineError> for Fail {
    fn from(error: EngineError) -> Self {
        Self {
            message: error.to_string(),
            code: exit_code_for(error.code()),
        }
    }
}

impl From<LoadError> for Fail {
    fn from(error: LoadError) -> Self {
        // A bad scenario or an unreachable variable is the author's mistake (2);
        // anything else is the engine failing to do its job (3).
        let code = match &error {
            LoadError::Config(_) | LoadError::Core(_) | LoadError::Http(_) => EXIT_CONFIG,
            _ => EXIT_ENGINE,
        };
        Self {
            message: error.to_string(),
            code,
        }
    }
}

/// Map a stable wire error code onto the CLI's exit-code contract.
fn exit_code_for(code: ErrorCode) -> u8 {
    match code {
        ErrorCode::Config | ErrorCode::BadMessage | ErrorCode::ProtocolVersionMismatch => {
            EXIT_CONFIG
        }
        ErrorCode::Unsupported => EXIT_CONFIG,
        ErrorCode::Engine | ErrorCode::Internal => EXIT_ENGINE,
    }
}

async fn run(args: &[String]) -> Result<(), Fail> {
    let Some(command) = args.first() else {
        print!("{}", usage());
        return Err(Fail::config("no command given"));
    };

    match command.as_str() {
        "request" => run_request(&args[1..]).await,
        "run" => run_load(&args[1..]).await,
        "serve" => run_serve(&args[1..]).await,
        "validate" => run_validate(&args[1..]),
        "help" | "--help" | "-h" => {
            print!("{}", usage());
            Ok(())
        }
        "version" | "--version" | "-V" => {
            println!("levi {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        other => {
            print!("{}", usage());
            Err(Fail::config(format!("unknown command `{other}`")))
        }
    }
}

// ---- request ----

async fn run_request(args: &[String]) -> Result<(), Fail> {
    let options = RequestOptions::parse(args)?;
    let format = Yaml;

    let base_dir = options
        .collection
        .clone()
        .unwrap_or_else(|| discover_collection_dir(&options.file));

    // A `file:` body is read *now*, relative to the collection, so the engine is
    // handed bytes and never opens a path of its own.
    let mut request = load_and_resolve(&format, &options.file, &base_dir)?;

    // Precedence: `--timeout` (the flag wins) > the request's own `timeout:` > the
    // run/scenario default > the transport default. The override is applied here,
    // where files are loaded, so every layer below sees one resolved deadline.
    if let Some(timeout) = options.timeout {
        request.timeout = Some(timeout);
    }
    let collection_path = base_dir.join("collection.yaml");
    let collection = if collection_path.is_file() {
        load_collection(&format, &collection_path)?
    } else {
        Collection::empty()
    };

    // `global < collection < environment`; the engine still owns resolution.
    let mut vars = collection.vars.clone();
    if let Some(name) = options.env.as_deref() {
        let env_path = base_dir.join("environments").join(format!("{name}.yaml"));
        if !env_path.is_file() {
            return Err(Fail::config(format!(
                "environment file not found: {}",
                env_path.display()
            )));
        }
        vars = vars.merged_with(&load_vars(&format, &env_path)?);
    }

    let request_dto = convert::request_to_dto(&request);
    let env_dto = convert::env_to_dto(&collection, &vars, options.seed);

    // The interactive path: a real `SendRequest` over the typed in-process channel.
    //
    // Retries are a transport policy, owned by whoever is running the command, so
    // they are set here and not in the collection: a load run must never pick one up
    // from a file that was written to paper over a flaky CI network.
    if options.retry_non_idempotent && options.retries == 0 {
        return Err(Fail::config(
            "--retry-non-idempotent does nothing without --retries",
        ));
    }
    let engine = Engine::from_store(store_from_env())?
        .with_retries(options.retries, options.retry_non_idempotent)?;
    let (mut client, handle) = in_process_client(engine, 8).await?;

    let outcome = client.send_request(request_dto, env_dto).await?;
    client.stop(true).await?;
    drop(client);

    match handle.await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => return Err(error.into()),
        Err(join) => return Err(Fail::engine(format!("engine task failed: {join}"))),
    }

    print_echo(&outcome.echo);
    println!();
    print_response(&outcome.response, options.show_body, options.max_body);
    print_checks(&outcome);

    let failed = outcome.checks_failed();
    if failed > 0 {
        return Err(Fail {
            message: format!("{failed} of {} checks failed", outcome.checks.len()),
            code: EXIT_THRESHOLD,
        });
    }
    Ok(())
}

// ---- run (load) ----

async fn run_load(args: &[String]) -> Result<(), Fail> {
    let options = RunOptions::parse(args)?;
    let format = Yaml;

    let base_dir = options
        .collection
        .clone()
        .unwrap_or_else(|| discover_collection_dir(&options.file));

    // The collection supplies `baseUrl` and default auth, exactly as a single send.
    let collection_path = base_dir.join("collection.yaml");
    let collection = if collection_path.is_file() {
        load_collection(&format, &collection_path)?
    } else {
        Collection::empty()
    };

    // Every request in the collection is a candidate step target.
    let mut requests = load_requests(&format, &base_dir)?;
    if requests.is_empty() {
        return Err(Fail::config(format!(
            "no requests found under `{}`",
            base_dir.join("requests").display()
        )));
    }
    // Same precedence as `request`: the flag wins over per-request and per-scenario
    // deadlines, so CI can bound a whole run with one flag regardless of what the
    // files ask for.
    if let Some(timeout) = options.timeout {
        for request in requests.values_mut() {
            request.timeout = Some(timeout);
        }
    }

    // `global < collection < environment`; the engine still owns resolution.
    let mut vars = collection.vars.clone();
    if let Some(name) = options.env.as_deref() {
        let env_path = base_dir.join("environments").join(format!("{name}.yaml"));
        if !env_path.is_file() {
            return Err(Fail::config(format!(
                "environment file not found: {}",
                env_path.display()
            )));
        }
        vars = vars.merged_with(&load_vars(&format, &env_path)?);
    }

    let scenario = load_scenario(&format, &options.file)?;
    let compiled = CompiledScenario::compile(&scenario, &collection, &requests)?;

    // Ctrl-C stops the run gracefully: the signal cancels the token the VUs watch, the
    // engine drains what is in flight, and the report says `aborted`.
    let cancel = CancellationToken::new();
    let cancel_for_signal = cancel.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            eprintln!("stopping... (Ctrl-C again to quit)");
            cancel_for_signal.cancel();
        }
    });

    let mut config = RunConfig::new(
        new_run_id(),
        Arc::new(compiled),
        vars,
        Arc::new(store_from_env()),
        options.seed,
    )
    .with_cancel(cancel);
    if let Some(workers) = options.workers {
        config = config.with_workers(workers);
    }

    let mut reporters: Vec<Box<dyn Reporter>> = Vec::new();
    if let Some(path) = options.out.clone() {
        reporters.push(Box::new(JsonReporter::new(path)));
    }

    let report = lev_load::run(config, &mut reporters).await?;

    if !options.quiet {
        print_report(&report, options.out.as_deref());
    }

    // The exit-code contract: a failed threshold is the verdict CI acts on and outranks
    // an abort; a plain abort (nothing failed) is its own code.
    let failed = report.thresholds.iter().filter(|t| !t.passed).count();
    if failed > 0 {
        return Err(Fail {
            message: format!(
                "{failed} of {} threshold(s) failed",
                report.thresholds.len()
            ),
            code: EXIT_THRESHOLD,
        });
    }
    match report.stopped_by {
        Some(reason) => Err(Fail {
            message: match reason {
                StopReason::Interrupt => "run aborted".to_string(),
                StopReason::Threshold { metric } => {
                    format!("run aborted: threshold on `{metric}` failed")
                }
            },
            code: EXIT_ABORTED,
        }),
        None => Ok(()),
    }
}

/// Load one request file and resolve its `file:` bodies against the collection root.
///
/// A `file:` body is part of the collection: a missing, escaping or oversized fixture
/// is an error wherever a request is loaded, never a surprise mid-run.
fn load_and_resolve(format: &Yaml, path: &Path, root: &Path) -> Result<RequestDef, FormatError> {
    let mut request = load_request(format, path)?;
    resolve_body_files(root, &mut request)?;
    Ok(request)
}

/// Load every `requests/*.yaml` under the collection rooted at `root` into an
/// id-keyed map; duplicate ids are an error.
///
/// `file:` bodies are read here, relative to `root`, so the load path and the engine
/// both receive bytes.
fn load_requests(format: &Yaml, root: &Path) -> Result<BTreeMap<Id, RequestDef>, Fail> {
    let mut requests: BTreeMap<Id, RequestDef> = BTreeMap::new();
    for path in yaml_files(&root.join("requests"))? {
        let request = load_and_resolve(format, &path, root)?;
        if requests.insert(request.id.clone(), request).is_some() {
            return Err(Fail::config(format!(
                "duplicate request id in `{}`",
                path.display()
            )));
        }
    }
    Ok(requests)
}

fn new_run_id() -> RunId {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    RunId::new(format!("run-{millis}"))
}

fn print_report(report: &RunReport, out: Option<&Path>) {
    println!(
        "run {} — {}",
        report.run_id.as_str(),
        report.outcome.as_str()
    );
    println!("  duration         {}", format_secs(report.duration));
    println!("  vus (max)        {}", report.max_vus);
    println!("  http_reqs        {}", report.http_reqs);
    println!("  http_req_failed  {}", report.http_req_failed);
    println!("  iterations       {}", report.iterations);
    println!("  rps              {:.1}", report.requests_per_second());
    println!(
        "  latency p50/p90/p95/p99  {} / {} / {} / {}",
        format_latency(report.latency.p50),
        format_latency(report.latency.p90),
        format_latency(report.latency.p95),
        format_latency(report.latency.p99),
    );
    println!(
        "  scheduler_lag    {}",
        format_ms(report.scheduler_lag.as_secs_f64() * 1000.0)
    );
    if let Some(reason) = &report.stopped_by {
        match reason {
            StopReason::Interrupt => println!("  stopped by       interrupt"),
            StopReason::Threshold { metric } => {
                println!("  stopped by       threshold ({metric})");
            }
        }
    }
    if !report.thresholds.is_empty() {
        println!("  thresholds");
        for result in &report.thresholds {
            let mark = if result.passed { "pass" } else { "FAIL" };
            println!(
                "    {mark}  {} {}  {}",
                result.metric,
                result.expression,
                format_observed(result)
            );
        }
    }
    if let Some(path) = out {
        println!("  report           {}", path.display());
    }
}

/// The observed number behind a threshold, in its own unit — `no samples` when the run
/// produced nothing to compare, which is a failure and should read like one.
fn format_observed(result: &ThresholdResult) -> String {
    match (result.observed, result.unit) {
        (None, _) => "no samples".to_string(),
        (Some(value), ThresholdUnit::Milliseconds) => format_ms(value),
        (Some(value), ThresholdUnit::Rate) => format!("{value:.4}"),
        (Some(value), ThresholdUnit::Count) => format!("{value:.0}"),
    }
}

fn format_secs(duration: Duration) -> String {
    format!("{:.3} s", duration.as_secs_f64())
}

fn format_latency(value: Option<Duration>) -> String {
    match value {
        Some(duration) => format_ms(duration.as_secs_f64() * 1000.0),
        None => "n/a".to_string(),
    }
}

// ---- serve ----

async fn run_serve(args: &[String]) -> Result<(), Fail> {
    let mut stdio = false;
    for arg in args {
        match arg.as_str() {
            "--stdio" => stdio = true,
            "-h" | "--help" => {
                print!("{}", usage());
                return Ok(());
            }
            other => {
                return Err(Fail::config(format!(
                    "unknown option `{other}` (only `--stdio` is available in this phase)"
                )));
            }
        }
    }
    if !stdio {
        return Err(Fail::config(
            "levi serve requires `--stdio`; a socket listener lands in Phase 6",
        ));
    }

    let engine = Engine::from_store(store_from_env())?;
    // Ctrl-C (and SIGTERM on unix) stops the session *gracefully*: the request in
    // flight finishes and its result is delivered, then the engine says why it
    // stopped and exits 0. Being killed mid-response would look like a crash.
    engine
        .serve_with_shutdown(NdjsonTransport::stdio(), shutdown_signal())
        .await?;
    Ok(())
}

/// Resolve when the process is asked to stop.
///
/// Ctrl-C everywhere, plus SIGTERM on unix — what a supervisor (systemd, Docker)
/// sends before it resorts to SIGKILL.
#[cfg(unix)]
async fn shutdown_signal() {
    use tokio::signal::unix::{signal, SignalKind};

    let Ok(mut term) = signal(SignalKind::terminate()) else {
        let _ = tokio::signal::ctrl_c().await;
        return;
    };
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = term.recv() => {}
    }
}

#[cfg(not(unix))]
async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

// ---- validate ----

fn run_validate(args: &[String]) -> Result<(), Fail> {
    let dir = match args {
        [] => return Err(Fail::config("validate needs a <dir>")),
        [dir] if !dir.starts_with('-') => PathBuf::from(dir),
        [flag] => {
            if flag == "-h" || flag == "--help" {
                print!("{}", usage());
                return Ok(());
            }
            return Err(Fail::config(format!("unknown option `{flag}`")));
        }
        _ => return Err(Fail::config("validate takes exactly one <dir>")),
    };

    let format = Yaml;
    let mut checked = 0usize;
    let mut failed = 0usize;

    let mut check = |label: &Path, result: Result<(), FormatError>| {
        checked += 1;
        if let Err(error) = result {
            failed += 1;
            eprintln!("{}: {error}", label.display());
        }
    };

    let collection_path = dir.join("collection.yaml");
    if collection_path.is_file() {
        check(
            &collection_path,
            load_collection(&format, &collection_path).map(|_| ()),
        );
    }

    for path in yaml_files(&dir.join("environments"))? {
        check(&path, load_vars(&format, &path).map(|_| ()));
    }
    for path in yaml_files(&dir.join("requests"))? {
        check(&path, load_and_resolve(&format, &path, &dir).map(|_| ()));
    }

    if checked == 0 {
        return Err(Fail::config(format!(
            "no collection documents found under `{}`",
            dir.display()
        )));
    }
    if failed > 0 {
        return Err(Fail::config(format!(
            "{failed} of {checked} documents failed to parse"
        )));
    }

    println!("ok: {checked} documents parsed");
    Ok(())
}

/// `*.yaml` files in `dir`, sorted, or an empty list if `dir` is absent.
fn yaml_files(dir: &Path) -> Result<Vec<PathBuf>, Fail> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let entries = std::fs::read_dir(dir)
        .map_err(|error| Fail::config(format!("failed to read `{}`: {error}", dir.display())))?;
    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path.extension().is_some_and(|ext| {
                    ext.eq_ignore_ascii_case("yaml") || ext.eq_ignore_ascii_case("yml")
                })
        })
        .collect();
    paths.sort();
    Ok(paths)
}

// ---- output ----

fn print_echo(echo: &EchoDto) {
    println!("{} {}", echo.method, echo.url);
    for header in &echo.headers {
        println!("  {}: {}", header.name, header.value);
    }
}

fn print_response(response: &ResponseDto, show_body: bool, max_body: usize) {
    // Saying how many attempts it took is the difference between "the target was
    // slow for 3s" and "the target refused twice, then answered".
    let attempts = if response.attempts > 1 {
        format!(", {} attempts", response.attempts)
    } else {
        String::new()
    };
    println!("HTTP {} ({}{attempts})", response.status, response.version);
    print_timings(&response.timings);

    println!("response headers:");
    for header in &response.headers {
        println!("  {}: {}", header.name, header.value);
    }

    if !show_body {
        return;
    }

    let truncated = response.body.len() > max_body;
    let shown: String = response.body.chars().take(max_body).collect();
    println!();
    if truncated {
        println!("response body (first {max_body} bytes):");
    } else {
        println!("response body:");
    }
    println!("{shown}");
}

fn print_checks(outcome: &RequestOutcome) {
    for log in &outcome.logs {
        println!("log: {log}");
    }
    if outcome.checks.is_empty() {
        return;
    }
    println!();
    println!("checks ({}):", outcome.checks.len());
    for check in &outcome.checks {
        let mark = if check.passed { "PASS" } else { "FAIL" };
        match &check.message {
            Some(message) => println!("  [{mark}] {} — {message}", check.name),
            None => println!("  [{mark}] {}", check.name),
        }
    }
}

fn print_timings(timings: &TimingsDto) {
    println!("  total      {:>10}", format_ms(timings.total_ms));
    println!("  ttfb       {:>10}", format_ms(timings.ttfb_ms));
    println!("  receive    {:>10}", format_ms(timings.receive_ms));
    for (label, value) in [
        ("dns", timings.dns_ms),
        ("connect", timings.connect_ms),
        ("tls", timings.tls_ms),
    ] {
        let shown = value.map(format_ms).unwrap_or_else(|| "n/a".to_string());
        println!("  {label:<10} {shown:>10}");
    }
}

fn format_ms(ms: f64) -> String {
    if ms < 1.0 {
        format!("{:.0} us", ms * 1_000.0)
    } else {
        format!("{ms:.3} ms")
    }
}

// ---- options ----

fn parent_dir(path: &Path) -> PathBuf {
    path.parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Walk up from the request file looking for `collection.yaml`, so files laid out as
/// `<collection>/requests/*.yaml` find their collection. Falls back to the file's
/// own directory.
fn discover_collection_dir(request_path: &Path) -> PathBuf {
    let start = parent_dir(request_path);
    let mut current: Option<&Path> = Some(start.as_path());
    while let Some(dir) = current {
        if dir.join("collection.yaml").is_file() {
            return dir.to_path_buf();
        }
        current = dir.parent();
    }
    start
}

struct RequestOptions {
    file: PathBuf,
    collection: Option<PathBuf>,
    env: Option<String>,
    seed: u64,
    show_body: bool,
    max_body: usize,
    timeout: Option<Duration>,
    retries: u32,
    retry_non_idempotent: bool,
}

impl RequestOptions {
    fn parse(args: &[String]) -> Result<Self, Fail> {
        let mut file: Option<PathBuf> = None;
        let mut collection = None;
        let mut env_name = None;
        let mut seed = 0u64;
        let mut show_body = false;
        let mut max_body = 4096usize;
        let mut timeout = None;
        let mut retries = 0u32;
        let mut retry_non_idempotent = false;

        let mut index = 0;
        while index < args.len() {
            let arg = &args[index];
            match arg.as_str() {
                "--collection" => {
                    index += 1;
                    collection = Some(PathBuf::from(next_value(args, index, "--collection")?));
                }
                "--env" => {
                    index += 1;
                    env_name = Some(next_value(args, index, "--env")?.to_string());
                }
                "--seed" => {
                    index += 1;
                    let value = next_value(args, index, "--seed")?;
                    seed = value.parse().map_err(|_| {
                        Fail::config(format!("--seed must be an integer, got `{value}`"))
                    })?;
                }
                "--timeout" => {
                    index += 1;
                    timeout = Some(parse_timeout_flag(next_value(args, index, "--timeout")?)?);
                }
                "--retries" => {
                    index += 1;
                    let value = next_value(args, index, "--retries")?;
                    retries = value.parse().map_err(|_| {
                        Fail::config(format!("--retries must be a whole number, got `{value}`"))
                    })?;
                }
                "--retry-non-idempotent" => retry_non_idempotent = true,
                "--max-body" => {
                    index += 1;
                    let value = next_value(args, index, "--max-body")?;
                    max_body = value.parse().map_err(|_| {
                        Fail::config(format!("--max-body must be an integer, got `{value}`"))
                    })?;
                }
                "--show-body" => show_body = true,
                "-h" | "--help" => {
                    print!("{}", usage());
                    std::process::exit(0);
                }
                other if other.starts_with('-') => {
                    return Err(Fail::config(format!("unknown option `{other}`")));
                }
                _ => {
                    if file.is_some() {
                        return Err(Fail::config("more than one request file given"));
                    }
                    file = Some(PathBuf::from(arg));
                }
            }
            index += 1;
        }

        let file = file.ok_or_else(|| Fail::config("request needs a <file>"))?;
        Ok(Self {
            file,
            collection,
            env: env_name,
            seed,
            show_body,
            max_body,
            timeout,
            retries,
            retry_non_idempotent,
        })
    }
}

/// Parse `--timeout`, rejecting a zero deadline: a request that must finish in
/// zero time is a bug, not a configuration.
fn parse_timeout_flag(value: &str) -> Result<Duration, Fail> {
    let duration = lev_format::parse_duration(value).map_err(|_| {
        Fail::config(format!(
            "--timeout must be a duration like `5s` or `500ms`, got `{value}`"
        ))
    })?;
    if duration.is_zero() {
        return Err(Fail::config("--timeout must be greater than 0"));
    }
    Ok(duration)
}

fn next_value<'a>(args: &'a [String], index: usize, flag: &str) -> Result<&'a str, Fail> {
    args.get(index)
        .map(String::as_str)
        .ok_or_else(|| Fail::config(format!("{flag} needs a value")))
}

struct RunOptions {
    file: PathBuf,
    collection: Option<PathBuf>,
    env: Option<String>,
    workers: Option<usize>,
    seed: u64,
    out: Option<PathBuf>,
    quiet: bool,
    timeout: Option<Duration>,
}

impl RunOptions {
    fn parse(args: &[String]) -> Result<Self, Fail> {
        let mut file: Option<PathBuf> = None;
        let mut collection = None;
        let mut env_name = None;
        let mut workers = None;
        let mut seed = 0u64;
        let mut out = None;
        let mut quiet = false;
        let mut timeout = None;

        let mut index = 0;
        while index < args.len() {
            let arg = &args[index];
            match arg.as_str() {
                "--collection" => {
                    index += 1;
                    collection = Some(PathBuf::from(next_value(args, index, "--collection")?));
                }
                "--env" => {
                    index += 1;
                    env_name = Some(next_value(args, index, "--env")?.to_string());
                }
                "--workers" => {
                    index += 1;
                    let value = next_value(args, index, "--workers")?;
                    let parsed: usize = value.parse().map_err(|_| {
                        Fail::config(format!("--workers must be an integer, got `{value}`"))
                    })?;
                    if parsed == 0 {
                        return Err(Fail::config("--workers must be at least 1"));
                    }
                    workers = Some(parsed);
                }
                "--seed" => {
                    index += 1;
                    let value = next_value(args, index, "--seed")?;
                    seed = value.parse().map_err(|_| {
                        Fail::config(format!("--seed must be an integer, got `{value}`"))
                    })?;
                }
                "--out" => {
                    index += 1;
                    out = Some(PathBuf::from(next_value(args, index, "--out")?));
                }
                "--timeout" => {
                    index += 1;
                    timeout = Some(parse_timeout_flag(next_value(args, index, "--timeout")?)?);
                }
                "--quiet" => quiet = true,
                "-h" | "--help" => {
                    print!("{}", usage());
                    std::process::exit(0);
                }
                other if other.starts_with('-') => {
                    return Err(Fail::config(format!("unknown option `{other}`")));
                }
                _ => {
                    if file.is_some() {
                        return Err(Fail::config("more than one scenario file given"));
                    }
                    file = Some(PathBuf::from(arg));
                }
            }
            index += 1;
        }

        let file = file.ok_or_else(|| Fail::config("run needs a <scenario>"))?;
        Ok(Self {
            file,
            collection,
            env: env_name,
            workers,
            seed,
            out,
            quiet,
            timeout,
        })
    }
}
