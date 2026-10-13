// SPDX-License-Identifier: MIT OR Apache-2.0
//! The `levi` command-line interface.
//!
//! Phase 0 surface: `levi request <file>`. Load runs (`levi run`), the protocol
//! boundary, and the daemon land in later phases.
#![forbid(unsafe_code)]

use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lev_core::{prepare_request_with_seed, Collection, CoreError, MapSecretStore, Redactor};
use lev_format::{load_collection, load_request, load_vars, FormatError, Yaml};
use lev_http::{HttpClient, HttpError};

/// Use mimalloc as the global allocator: a load generator allocates constantly, and
/// a predictable allocator keeps generator-side jitter out of the histograms.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// Exit codes are a contract for CI and are documented permanently.
const EXIT_CONFIG: u8 = 2;
const EXIT_ENGINE: u8 = 3;

/// Environment variables named `LEVI_SECRET_<NAME>` seed the secret store until the
/// OS keychain lands in Phase 1.
const SECRET_ENV_PREFIX: &str = "LEVI_SECRET_";

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match run(&args).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(fail) => {
            eprintln!("error: {}", fail.message);
            ExitCode::from(fail.code)
        }
    }
}

fn run_usage() -> String {
    format!(
        "levi {version} — the request you already wrote is the load test

USAGE:
    levi request <file> [OPTIONS]

OPTIONS:
    --collection <dir>   Collection directory (default: the request file's folder)
    --env <name>         Load environments/<name>.yaml
    --seed <n>           Seed for dynamic functions (uuid, timestamp, randomInt)
    --show-body          Print the (redacted) response body
    --max-body <bytes>   Truncate the printed body [default: 4096]
    -h, --help           Print help

Secrets are read from environment variables named LEVI_SECRET_<NAME>.
",
        version = env!("CARGO_PKG_VERSION"),
    )
}

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

fn map_http(error: HttpError) -> Fail {
    let is_config = matches!(
        &error,
        HttpError::InvalidUrl(_)
            | HttpError::InvalidHeader(_)
            | HttpError::InvalidMethod(_)
            | HttpError::Tls(_)
    );
    let message = error.to_string();
    if is_config {
        Fail::config(message)
    } else {
        Fail::engine(message)
    }
}

async fn run(args: &[String]) -> Result<(), Fail> {
    let Some(command) = args.first() else {
        print!("{}", run_usage());
        return Err(Fail::config("no command given"));
    };
    match command.as_str() {
        "request" => run_request(&args[1..]).await,
        "help" | "--help" | "-h" => {
            print!("{}", run_usage());
            Ok(())
        }
        "version" | "--version" | "-V" => {
            println!("levi {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        other => {
            print!("{}", run_usage());
            Err(Fail::config(format!("unknown command `{other}`")))
        }
    }
}

async fn run_request(args: &[String]) -> Result<(), Fail> {
    let options = RequestOptions::parse(args)?;
    let format = Yaml;

    let request = load_request(&format, &options.file)?;

    let base_dir = options
        .collection
        .clone()
        .unwrap_or_else(|| discover_collection_dir(&options.file));
    let collection_path = base_dir.join("collection.yaml");
    let collection = if collection_path.exists() {
        load_collection(&format, &collection_path)?
    } else {
        Collection::empty()
    };

    let mut vars = collection.vars.clone();
    if let Some(name) = options.env.as_deref() {
        let env_path = base_dir.join("environments").join(format!("{name}.yaml"));
        if !env_path.exists() {
            return Err(Fail::config(format!(
                "environment file not found: {}",
                env_path.display()
            )));
        }
        vars = vars.merged_with(&load_vars(&format, &env_path)?);
    }

    let secrets = load_secrets_from_env();
    let redactor = Redactor::with_secrets(secrets.values());

    let prepared = prepare_request_with_seed(&collection, &request, vars, &secrets, options.seed)?;

    let client = HttpClient::new().map_err(map_http)?;
    let response = client.send(prepared.clone()).await.map_err(map_http)?;

    print_request(&prepared, &redactor);
    println!();
    println!("HTTP {} ({})", response.status, response.version);
    print!("{}", response.timings);
    print_response(&response, &redactor, options.show_body, options.max_body);

    Ok(())
}

fn load_secrets_from_env() -> MapSecretStore {
    let mut store = MapSecretStore::new();
    for (key, value) in env::vars_os() {
        let (Some(key), Some(value)) = (key.to_str(), value.to_str()) else {
            continue;
        };
        if let Some(name) = key.strip_prefix(SECRET_ENV_PREFIX) {
            store.insert(name.to_string(), value);
        }
    }
    store
}

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

fn print_request(prepared: &lev_core::PreparedRequest, redactor: &Redactor) {
    println!(
        "{} {}",
        prepared.method.as_str(),
        redactor.redact(&prepared.url)
    );
    for (name, value) in &prepared.headers {
        println!("  {name}: {}", redactor.redact_header_value(name, value));
    }
}

fn print_response(
    response: &lev_http::Response,
    redactor: &Redactor,
    show_body: bool,
    max_body: usize,
) {
    println!("response headers:");
    for (name, value) in &response.headers {
        println!("  {name}: {}", redactor.redact_header_value(name, value));
    }

    if !show_body {
        return;
    }

    let text = String::from_utf8_lossy(&response.body);
    let redacted = redactor.redact(&text);
    let truncated = redacted.len() > max_body;
    let shown: String = redacted.chars().take(max_body).collect();

    println!();
    if truncated {
        println!("response body (first {max_body} bytes):");
    } else {
        println!("response body:");
    }
    println!("{shown}");
}

struct RequestOptions {
    file: PathBuf,
    collection: Option<PathBuf>,
    env: Option<String>,
    seed: u64,
    show_body: bool,
    max_body: usize,
}

impl RequestOptions {
    fn parse(args: &[String]) -> Result<Self, Fail> {
        let mut file: Option<PathBuf> = None;
        let mut collection = None;
        let mut env = None;
        let mut seed = 0u64;
        let mut show_body = false;
        let mut max_body = 4096usize;

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
                    env = Some(next_value(args, index, "--env")?.to_string());
                }
                "--seed" => {
                    index += 1;
                    let value = next_value(args, index, "--seed")?;
                    seed = value.parse().map_err(|_| {
                        Fail::config(format!("--seed must be an integer, got `{value}`"))
                    })?;
                }
                "--max-body" => {
                    index += 1;
                    let value = next_value(args, index, "--max-body")?;
                    max_body = value.parse().map_err(|_| {
                        Fail::config(format!("--max-body must be an integer, got `{value}`"))
                    })?;
                }
                "--show-body" => show_body = true,
                "-h" | "--help" => {
                    print!("{}", run_usage());
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
            env,
            seed,
            show_body,
            max_body,
        })
    }
}

fn next_value<'a>(args: &'a [String], index: usize, flag: &str) -> Result<&'a str, Fail> {
    args.get(index)
        .map(String::as_str)
        .ok_or_else(|| Fail::config(format!("{flag} needs a value")))
}
