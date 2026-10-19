// SPDX-License-Identifier: MIT OR Apache-2.0
//! The pooled script sandbox.
//!
//! A [`Sandbox`] owns a dedicated OS thread that owns a QuickJS [`Runtime`] and one
//! reused [`Context`]. `run` is `&self` and `Send + Sync`: it posts a job to that
//! thread and blocks for the reply. Pooling the runtime and context is what makes a
//! script cheap enough to run once per iteration — creating a QuickJS runtime per
//! iteration at load-run rates is the overhead the plan calls out.
//!
//! # Why secrets may be visible to a script
//!
//! `lev.env` exposes the resolved variable scope, secrets included. That does **not**
//! violate the "no secret crosses the boundary" invariant: the sandbox runs *inside*
//! the engine, and every check and log it produces is redacted before it is serialized.
//! A script therefore cannot leak a secret to a client even by logging it.
//!
//! # Isolation
//!
//! No filesystem, no network, no `eval` escape hatch beyond what QuickJS itself cannot
//! do: only the `lev.*` primitives are registered. A runaway script is stopped by the
//! runtime's interrupt handler (CPU budget) or its memory limit, and the sandbox stays
//! usable for the next script — a failure is contained, not fatal.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use lev_core::Check;
use rquickjs::{Context, Ctx, Function, Object, Runtime};
use serde_json::{json, Map, Value};

use crate::error::ScriptError;
use crate::limits::Limits;
use crate::model::{Phase, ScriptInput, ScriptOutcome, ScriptRequest, ScriptResponse};

/// The `lev.*` API, built over the native primitives registered below.
const PRELUDE: &str = include_str!("prelude.js");

/// Reads the (possibly mutated) request back out of the global scope.
const SETTLE_REQUEST: &str =
    "JSON.stringify((typeof lev !== 'undefined' && lev.request) ? lev.request : null)";

/// One unit of work for the worker thread.
struct Job {
    source: String,
    input: ScriptInput,
    reply: Sender<Result<ScriptOutcome, ScriptError>>,
}

/// A reusable JavaScript sandbox.
///
/// Cheap to call repeatedly: the runtime and context are created once and reused.
pub struct Sandbox {
    jobs: Sender<Job>,
    _worker: JoinHandle<()>,
}

impl Sandbox {
    /// Start a sandbox with the given limits.
    ///
    /// Blocks only until the worker has created its runtime, so a failure to
    /// initialise QuickJS is reported here rather than on the first `run`.
    pub fn new(limits: Limits) -> Result<Self, ScriptError> {
        let (jobs, inbox) = mpsc::channel::<Job>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), ScriptError>>();
        let worker = thread::Builder::new()
            .name("lev-script".to_string())
            .spawn(move || worker_loop(limits, inbox, ready_tx))
            .map_err(|error| {
                ScriptError::Internal(format!("could not start the script thread: {error}"))
            })?;

        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                jobs,
                _worker: worker,
            }),
            Ok(Err(error)) => Err(error),
            Err(_) => Err(ScriptError::Internal(
                "the script thread exited during start-up".to_string(),
            )),
        }
    }

    /// Run `source` against `input`, returning what it produced.
    ///
    /// A failure is contained: the sandbox remains usable afterwards.
    pub fn run(&self, source: &str, input: ScriptInput) -> Result<ScriptOutcome, ScriptError> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.jobs
            .send(Job {
                source: source.to_string(),
                input,
                reply: reply_tx,
            })
            .map_err(|_| ScriptError::Internal("the script thread is gone".to_string()))?;
        reply_rx
            .recv()
            .map_err(|_| ScriptError::Internal("the script thread dropped the job".to_string()))?
    }
}

impl std::fmt::Debug for Sandbox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sandbox").finish_non_exhaustive()
    }
}

/// The worker thread: owns the runtime and context for the sandbox's whole life.
fn worker_loop(limits: Limits, inbox: Receiver<Job>, ready: Sender<Result<(), ScriptError>>) {
    let runtime = match Runtime::new() {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = ready.send(Err(ScriptError::Internal(format!(
                "could not create the QuickJS runtime: {error}"
            ))));
            return;
        }
    };
    runtime.set_memory_limit(limits.memory_limit);

    let context = match Context::full(&runtime) {
        Ok(context) => context,
        Err(error) => {
            let _ = ready.send(Err(ScriptError::Internal(format!(
                "could not create the QuickJS context: {error}"
            ))));
            return;
        }
    };
    let _ = ready.send(Ok(()));

    while let Ok(job) = inbox.recv() {
        let Job {
            source,
            input,
            reply,
        } = job;
        let result = run_job(&runtime, &context, &source, input, limits);
        let _ = reply.send(result);
    }
}

/// Run one job under a CPU budget, then clear the interrupt handler.
fn run_job(
    runtime: &Runtime,
    context: &Context,
    source: &str,
    input: ScriptInput,
    limits: Limits,
) -> Result<ScriptOutcome, ScriptError> {
    let interrupted = Arc::new(AtomicBool::new(false));
    {
        let flag = Arc::clone(&interrupted);
        runtime.set_interrupt_handler(Some(Box::new(move || flag.load(Ordering::SeqCst))));
    }

    // A watchdog flips the flag if the script outlives its budget. QuickJS polls the
    // interrupt handler on every backward branch, so a `while (true)` is caught.
    let (done_tx, done_rx) = mpsc::channel::<()>();
    let watchdog = {
        let flag = Arc::clone(&interrupted);
        let budget = limits.cpu_budget;
        thread::Builder::new()
            .name("lev-script-budget".to_string())
            .spawn(move || {
                if done_rx.recv_timeout(budget).is_err() {
                    flag.store(true, Ordering::SeqCst);
                }
            })
            .map_err(|error| {
                ScriptError::Internal(format!("could not start the watchdog: {error}"))
            })?
    };

    let result = execute(context, source, input);

    let _ = done_tx.send(());
    let _ = watchdog.join();
    runtime.set_interrupt_handler(None);

    // A success always wins: the watchdog can fire just as a fast script finishes.
    if result.is_ok() {
        return result;
    }
    if interrupted.load(Ordering::SeqCst) {
        return Err(ScriptError::timeout(limits.cpu_budget));
    }
    result
}

/// Install the natives, build `lev.*`, run the script, and read the results back.
fn execute(
    context: &Context,
    source: &str,
    input: ScriptInput,
) -> Result<ScriptOutcome, ScriptError> {
    let checks: Rc<RefCell<Vec<Check>>> = Rc::new(RefCell::new(Vec::new()));
    let logs: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let rng = Rc::new(RefCell::new(Rng::new(input.seed)));
    let vars = Rc::new(RefCell::new(input.vars.clone()));

    let request_json = serde_json::to_string(&request_value(&input.request)).map_err(internal)?;
    let response_json = input
        .response
        .as_ref()
        .map(response_value)
        .map(|value| serde_json::to_string(&value))
        .transpose()
        .map_err(internal)?;

    let settled = context.with(|ctx| -> Result<String, ScriptError> {
        let globals = ctx.globals();
        let object = Object::new(ctx.clone()).map_err(js_internal)?;

        macro_rules! bind {
            ($name:literal, $func:expr) => {
                object
                    .set(
                        $name,
                        Function::new(ctx.clone(), $func).map_err(js_internal)?,
                    )
                    .map_err(js_internal)?;
            };
        }

        let check_sink = Rc::clone(&checks);
        bind!(
            "emitCheck",
            move |name: String, passed: bool, message: Option<String>| {
                check_sink
                    .borrow_mut()
                    .push(Check::new(name, passed, message));
            }
        );

        let log_sink = Rc::clone(&logs);
        bind!("log", move |message: String| {
            log_sink.borrow_mut().push(message);
        });

        let int_rng = Rc::clone(&rng);
        bind!("randInt", move |min: i64, max: i64| -> i64 {
            int_rng.borrow_mut().int(min, max)
        });
        let float_rng = Rc::clone(&rng);
        bind!("randFloat", move || -> f64 {
            float_rng.borrow_mut().float()
        });
        let uuid_rng = Rc::clone(&rng);
        bind!("uuid", move || -> String { uuid_rng.borrow_mut().uuid() });
        let string_rng = Rc::clone(&rng);
        bind!("randString", move |len: i64| -> String {
            string_rng.borrow_mut().string(len)
        });

        let get_vars = Rc::clone(&vars);
        bind!("envGet", move |name: String| -> Option<String> {
            get_vars.borrow().get(&name).cloned()
        });
        let set_vars = Rc::clone(&vars);
        bind!("envSet", move |name: String, value: String| {
            set_vars.borrow_mut().insert(name, value);
        });
        let dump_vars = Rc::clone(&vars);
        bind!("varsJson", move || -> String {
            serde_json::to_string(&*dump_vars.borrow()).unwrap_or_else(|_| "{}".to_string())
        });

        let request = request_json.clone();
        bind!("requestJson", move || -> String { request.clone() });
        let response = response_json.clone();
        bind!("responseJson", move || -> Option<String> {
            response.clone()
        });

        globals.set("__lev", object).map_err(js_internal)?;

        // The prelude is ours; a failure here is a harness bug, not the user's.
        ctx.eval::<(), _>(PRELUDE)
            .map_err(|error| ScriptError::Internal(js_message(&ctx, error)))?;

        if let Err(error) = ctx.eval::<(), _>(source) {
            return Err(classify(&ctx, error));
        }

        if input.phase == Phase::PreRequest {
            ctx.eval::<String, _>(SETTLE_REQUEST)
                .map_err(|error| ScriptError::Internal(js_message(&ctx, error)))
        } else {
            Ok("null".to_string())
        }
    })?;

    let request = if input.phase == Phase::PreRequest {
        let value: Value = serde_json::from_str(&settled).map_err(internal)?;
        merge_request(&value, &input.request)
    } else {
        input.request
    };

    // Hoist the borrows: a `RefMut` temporary cannot outlive the `Rc` locals.
    let vars = vars.borrow().clone();
    let checks = checks.borrow_mut().drain(..).collect();
    let logs = logs.borrow_mut().drain(..).collect();

    Ok(ScriptOutcome {
        request,
        vars,
        checks,
        logs,
    })
}

// ---- JSON shapes ----

fn request_value(request: &ScriptRequest) -> Value {
    json!({
        "method": request.method,
        "url": request.url,
        "headers": headers_value(&request.headers),
        "body": request.body,
    })
}

fn response_value(response: &ScriptResponse) -> Value {
    json!({
        "status": response.status,
        "headers": headers_value(&response.headers),
        "body": response.body,
        "time": response.time_ms,
    })
}

fn headers_value(headers: &[(String, String)]) -> Value {
    let mut map = Map::new();
    for (name, value) in headers {
        map.insert(name.clone(), Value::String(value.clone()));
    }
    Value::Object(map)
}

/// Fold the script's `lev.request` back into the original request.
///
/// A non-object (e.g. the script set it to `null`) leaves the request untouched.
/// Headers are rebuilt from the object, so the script may remove one by deleting the
/// key. The deadline is not scriptable and is always carried over from `original`.
fn merge_request(value: &Value, original: &ScriptRequest) -> ScriptRequest {
    let Some(object) = value.as_object() else {
        return original.clone();
    };

    let text = |key: &str, fallback: &str| {
        object
            .get(key)
            .and_then(scalar_text)
            .unwrap_or_else(|| fallback.to_string())
    };

    let headers = match object.get("headers").and_then(Value::as_object) {
        Some(map) => map
            .iter()
            .filter_map(|(name, value)| scalar_text(value).map(|text| (name.clone(), text)))
            .collect(),
        None => original.headers.clone(),
    };

    ScriptRequest {
        method: text("method", &original.method),
        url: text("url", &original.url),
        headers,
        body: object
            .get("body")
            .and_then(scalar_text)
            .unwrap_or_else(|| original.body.clone()),
        timeout: original.timeout,
    }
}

/// Coerce a JSON scalar to text, so `lev.request.headers['X'] = 1` works.
fn scalar_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

// ---- errors ----

fn internal(error: serde_json::Error) -> ScriptError {
    ScriptError::Internal(error.to_string())
}

fn js_internal(error: rquickjs::Error) -> ScriptError {
    ScriptError::Internal(error.to_string())
}

/// Turn a QuickJS error into a [`ScriptError`], recovering the thrown message.
fn classify(ctx: &Ctx<'_>, error: rquickjs::Error) -> ScriptError {
    let message = js_message(ctx, error);
    if message.to_ascii_lowercase().contains("out of memory") {
        ScriptError::Memory(message)
    } else if looks_like_syntax_error(&message) {
        ScriptError::Compile(message)
    } else {
        ScriptError::Runtime(message)
    }
}

/// `Error::Exception` alone is useless ("Exception generated by QuickJS"); the real
/// message lives in the value that was thrown, which `ctx.catch()` recovers.
fn js_message(ctx: &Ctx<'_>, error: rquickjs::Error) -> String {
    if matches!(error, rquickjs::Error::Exception) {
        let caught = ctx.catch();
        if let Some(exception) = caught.as_exception() {
            if let Some(message) = exception.message() {
                return message;
            }
        }
        return format!("{caught:?}");
    }
    error.to_string()
}

/// QuickJS reports a syntax error as an ordinary exception, so a marker is the best
/// available signal. It only affects the message prefix, never control flow.
fn looks_like_syntax_error(message: &str) -> bool {
    const MARKERS: &[&str] = &[
        "unexpected token",
        "unexpected end of input",
        "syntax error",
        "unterminated string",
        "invalid number",
        "invalid identifier",
        "duplicate parameter",
        "invalid use of",
    ];
    let lower = message.to_ascii_lowercase();
    MARKERS.iter().any(|marker| lower.contains(marker))
}

// ---- seeded RNG ----

/// A small xorshift PRNG, so `lev.random` is reproducible under `--seed`.
struct Rng {
    state: u64,
}

impl Rng {
    fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 {
                0x9E37_79B9_7F4A_7C15
            } else {
                seed
            },
        }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    fn int(&mut self, min: i64, max: i64) -> i64 {
        let (low, high) = if min <= max { (min, max) } else { (max, min) };
        let span = (high - low) as u64 + 1;
        low + (self.next_u64() % span) as i64
    }

    fn float(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn string(&mut self, len: i64) -> String {
        const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
        let len = len.clamp(0, 1 << 16) as usize;
        (0..len)
            .map(|_| ALPHABET[(self.next_u64() % ALPHABET.len() as u64) as usize] as char)
            .collect()
    }

    fn uuid(&mut self) -> String {
        let hi = self.next_u64();
        let lo = self.next_u64();
        let mut bytes = [0u8; 16];
        bytes[..8].copy_from_slice(&hi.to_be_bytes());
        bytes[8..].copy_from_slice(&lo.to_be_bytes());
        bytes[6] = (bytes[6] & 0x0f) | 0x40;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        format!(
            "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
        )
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn rng_is_reproducible_and_bounded() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..10 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
        let mut rng = Rng::new(7);
        for _ in 0..100 {
            let value = rng.int(5, 10);
            assert!((5..=10).contains(&value), "{value}");
        }
        assert!(rng.float() < 1.0);
    }

    #[test]
    fn uuid_has_the_right_shape() {
        let uuid = Rng::new(9).uuid();
        assert_eq!(uuid.len(), 36);
        assert_eq!(uuid.as_bytes()[14], b'4');
    }

    #[test]
    fn merge_request_preserves_untouched_fields() {
        let original = ScriptRequest {
            method: "GET".to_string(),
            url: "https://api.test/".to_string(),
            headers: vec![("Accept".to_string(), "application/json".to_string())],
            body: String::new(),
            timeout: Some(Duration::from_secs(5)),
        };
        let value = json!({"method": "POST", "headers": {"X-Trace": "1"}});
        let merged = merge_request(&value, &original);
        assert_eq!(merged.method, "POST");
        assert_eq!(merged.url, "https://api.test/");
        assert_eq!(
            merged.headers,
            vec![("X-Trace".to_string(), "1".to_string())]
        );
        // The deadline is not scriptable: a rewrite cannot drop it.
        assert_eq!(merged.timeout, original.timeout);
    }

    #[test]
    fn merge_request_ignores_a_non_object() {
        let original = ScriptRequest {
            method: "GET".to_string(),
            url: "u".to_string(),
            headers: Vec::new(),
            body: String::new(),
            timeout: None,
        };
        assert_eq!(merge_request(&Value::Null, &original), original);
    }
}
