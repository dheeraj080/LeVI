// SPDX-License-Identifier: MIT OR Apache-2.0
//! The engine session loop.
//!
//! The whole engine is this: a handshake, then a dispatch loop over a [`Transport`].
//! Because the loop is transport-generic, `levi serve --stdio` and an in-process
//! request run *identical* code — there is no second code path to keep in sync.

use std::collections::BTreeMap;
use std::future::Future;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use lev_core::{
    prepare_request_with_seed, Check, MapSecretStore, Redactor, SecretStore, VarSet, VarValue,
};
use lev_http::{ClientConfig, HttpClient};
use lev_protocol::{
    negotiate, EchoDto, EnvDto, ErrorCode, LogLevel, Message, RequestDto, ResponseDto, Transport,
    TransportError, PROTOCOL_VERSION,
};
use lev_script::{
    Limits, Phase, Sandbox, ScriptInput, ScriptOutcome, ScriptRequest, ScriptResponse,
};

use crate::checks;
use crate::convert;
use crate::error::EngineError;
use crate::ENGINE_VERSION;

/// Everything one `SendRequest` produces.
///
/// `checks` and `logs` have already passed through the redactor, so they are safe to
/// serialize and show.
pub struct Dispatch {
    pub echo: EchoDto,
    pub response: ResponseDto,
    pub checks: Vec<Check>,
    pub logs: Vec<String>,
}

/// The engine, holding the transport-independent state a session needs.
pub struct Engine {
    secrets: Arc<dyn SecretStore>,
    /// Behind a lock because a keychain secret's value cannot be known when the
    /// engine is built: the keychain has no "list", so the store reports what it
    /// resolves and [`Engine::redactor`] folds each value in before it can be echoed.
    redactor: Mutex<Redactor>,
    http: HttpClient,
    scripts: Arc<Sandbox>,
}

impl Engine {
    /// Build an engine with an explicit secret store and redactor.
    ///
    /// The redactor must cover the values the store can resolve *without being
    /// asked per name*; a store that cannot report them ahead of time (the OS
    /// keychain) reports them through [`SecretStore::newly_resolved`] instead, and
    /// this engine folds those in as they arrive.
    pub fn new(secrets: Arc<dyn SecretStore>, redactor: Redactor) -> Result<Self, EngineError> {
        Self::with_script_limits(secrets, redactor, Limits::default())
    }

    /// Build an engine with explicit script limits (used by tests to tighten the budget).
    pub fn with_script_limits(
        secrets: Arc<dyn SecretStore>,
        redactor: Redactor,
        limits: Limits,
    ) -> Result<Self, EngineError> {
        Ok(Self {
            secrets,
            redactor: Mutex::new(redactor),
            http: HttpClient::new()?,
            scripts: Arc::new(Sandbox::new(limits)?),
        })
    }

    /// Build an engine from an in-memory store, deriving the redactor from it.
    ///
    /// In-memory only: no OS calls, ever. This is what tests use.
    pub fn from_store(store: MapSecretStore) -> Result<Self, EngineError> {
        let redactor = crate::secrets::redactor_for(store.values());
        Self::new(
            Arc::new(crate::secrets::ResolvingStore::memory(store)),
            redactor,
        )
    }

    /// Build an engine whose secrets come from the environment and the OS keychain.
    ///
    /// This is what the CLI uses. The redactor starts with the environment's values
    /// — all that is knowable without asking the keychain — and every keychain value
    /// is folded in the moment it is resolved (see [`SecretStore::newly_resolved`]).
    pub fn with_os_secrets() -> Result<Self, EngineError> {
        let store = crate::secrets::store_from_env();
        let redactor = crate::secrets::redactor_for(store.known_values());
        Self::new(Arc::new(store), redactor)
    }

    /// The redactor, with everything the store resolved since the last look folded in.
    ///
    /// A keychain value cannot be known up front — the keychain has no "list" — so
    /// the store reports what it resolved and this is where that becomes redaction,
    /// before the request is echoed or any script output is kept. The lock is
    /// dropped before the next await: it guards bookkeeping, never I/O.
    fn redactor(&self) -> MutexGuard<'_, Redactor> {
        let mut redactor = self.redactor.lock().unwrap_or_else(PoisonError::into_inner);
        for value in self.secrets.newly_resolved() {
            redactor.add_secret(value);
        }
        redactor
    }

    /// Replace the transport policy this engine sends with.
    ///
    /// Timeouts, retries and pool sizing belong to the client, not to the collection:
    /// a request file says *what* to send, and how hard to try again is a decision
    /// about the client's own time and about the target's load.
    pub fn with_client_config(mut self, config: ClientConfig) -> Result<Self, EngineError> {
        self.http = HttpClient::with_config(config)?;
        Ok(self)
    }

    /// Retry a transient failure up to `retries` more times on an idempotent request.
    ///
    /// `retry_non_idempotent` lifts that restriction for a target known to be safe
    /// with it. The policy is set here, by whoever runs the engine — a collection
    /// cannot express it, because a load run must never inherit a retry from a file.
    pub fn with_retries(
        mut self,
        retries: u32,
        retry_non_idempotent: bool,
    ) -> Result<Self, EngineError> {
        let mut config = ClientConfig::default().with_retries(retries);
        if retry_non_idempotent {
            config = config.retrying_non_idempotent();
        }
        self.http = HttpClient::with_config(config)?;
        Ok(self)
    }

    /// Serve one session to completion over `transport`.
    ///
    /// Returns `Ok(())` for a clean end of stream, a `Stop`, or a rejected
    /// handshake. A rejected handshake sends exactly one `Error` first, so the peer
    /// learns *why* instead of just seeing a closed pipe.
    pub async fn serve<T: Transport>(&self, transport: T) -> Result<(), EngineError> {
        self.serve_with_shutdown(transport, std::future::pending())
            .await
    }

    /// Serve one session until the stream ends, a `stop` arrives, or `shutdown`
    /// resolves.
    ///
    /// `shutdown` is the **graceful** stop: it takes effect between messages, so a
    /// request already being dispatched runs to completion and its result is
    /// delivered — a `levi serve --stdio` stopped by Ctrl-C never truncates an
    /// in-flight response. Nothing new is accepted after it resolves.
    ///
    /// A `stop` message means the same thing, because the loop is sequential: by the
    /// time one is read there is no work in flight to abandon, so `graceful: false`
    /// has nothing extra to cut short. The flag is echoed in the shutdown log so the
    /// distinction stays visible on the wire.
    pub async fn serve_with_shutdown<T, S>(
        &self,
        mut transport: T,
        shutdown: S,
    ) -> Result<(), EngineError>
    where
        T: Transport,
        S: Future<Output = ()>,
    {
        tokio::pin!(shutdown);

        let first = tokio::select! {
            biased;
            () = &mut shutdown => return Ok(()),
            next = transport.recv() => next,
        };
        let first = match first {
            Ok(Some(message)) => message,
            Ok(None) => return Ok(()),
            // A line that is not a protocol message is still a *client* mistake worth an
            // answer: `badMessage`, then close — there is no session yet to keep alive. The
            // framing is one message per line, so nothing after it is lost either way.
            Err(TransportError::Decode(error)) => {
                transport
                    .send(Message::error(
                        ErrorCode::BadMessage,
                        format!("not a protocol message: {error}"),
                    ))
                    .await?;
                return Ok(());
            }
            Err(other) => return Err(other.into()),
        };

        let Message::Hello {
            protocol_version, ..
        } = first
        else {
            transport
                .send(Message::error(
                    ErrorCode::BadMessage,
                    format!(
                        "expected `hello` as the first message, got `{}`",
                        first.type_name()
                    ),
                ))
                .await?;
            return Ok(());
        };

        let version = match negotiate(protocol_version, PROTOCOL_VERSION) {
            Ok(version) => version,
            Err(error) => {
                transport
                    .send(Message::error(
                        ErrorCode::ProtocolVersionMismatch,
                        error.to_string(),
                    ))
                    .await?;
                return Ok(());
            }
        };

        transport
            .send(Message::Ready {
                version,
                engine: ENGINE_VERSION.to_string(),
            })
            .await?;
        tracing::info!(version = %version, engine = ENGINE_VERSION, "session ready");

        loop {
            let incoming = tokio::select! {
                biased;
                () = &mut shutdown => {
                    self.note_shutdown(&mut transport, "signal").await;
                    return Ok(());
                }
                next = transport.recv() => next,
            };
            let message = match incoming {
                Ok(Some(message)) => message,
                Ok(None) => {
                    tracing::info!(reason = "stream closed", "engine session ending");
                    return Ok(());
                }
                // Framing is one message per line, so a garbled line costs the line and not
                // the session: answer `badMessage` and keep serving.
                Err(TransportError::Decode(error)) => {
                    transport
                        .send(Message::error(
                            ErrorCode::BadMessage,
                            format!("not a protocol message: {error}"),
                        ))
                        .await?;
                    continue;
                }
                Err(other) => return Err(other.into()),
            };
            match message {
                Message::SendRequest { request, env } => {
                    tracing::debug!(name = %request.name, "dispatching request");
                    let reply = match self.dispatch(request, env).await {
                        Ok(dispatch) => Message::RequestResult {
                            request: dispatch.echo,
                            response: dispatch.response,
                            checks: dispatch.checks.iter().map(convert::check_to_dto).collect(),
                            logs: dispatch.logs,
                        },
                        Err(error) => Message::error(error.code(), error.to_string()),
                    };
                    transport.send(reply).await?;
                }
                Message::Stop { graceful } => {
                    let cause = if graceful {
                        "stop"
                    } else {
                        "stop (ungraceful)"
                    };
                    self.note_shutdown(&mut transport, cause).await;
                    return Ok(());
                }
                Message::Hello { .. } => {
                    transport
                        .send(Message::error(
                            ErrorCode::BadMessage,
                            "`hello` was already received",
                        ))
                        .await?;
                }
                other => {
                    transport
                        .send(Message::error(
                            ErrorCode::Unsupported,
                            format!(
                                "`{}` is not supported by protocol {PROTOCOL_VERSION}",
                                other.type_name()
                            ),
                        ))
                        .await?;
                }
            }
        }
    }

    /// Tell the peer why the session is ending, best-effort.
    ///
    /// The pipe may already be gone (the parent exited, the client closed it); a
    /// failed send must not turn a clean stop into an error.
    async fn note_shutdown<T: Transport>(&self, transport: &mut T, cause: &str) {
        // Both records of the same fact: the peer learns it on the wire, and whoever
        // is watching the process learns it on stderr — they are different audiences
        // and neither should have to guess.
        tracing::info!(reason = cause, "engine session ending");
        let _ = transport
            .send(Message::Log {
                level: LogLevel::Info,
                msg: format!("engine stopping: {cause}"),
            })
            .await;
    }

    /// Resolve, run the hooks, send, then evaluate assertions — everything redacted.
    pub async fn dispatch(
        &self,
        request: RequestDto,
        env: EnvDto,
    ) -> Result<Dispatch, EngineError> {
        let (collection, vars) = convert::env_from_dto(&env)?;
        let request = convert::request_from_dto(request)?;
        let prepared = prepare_request_with_seed(
            &collection,
            &request,
            vars.clone(),
            self.secrets.as_ref(),
            env.seed,
        )?;

        let mut scope = script_scope(&vars, self.secrets.as_ref())?;
        let mut checks: Vec<Check> = Vec::new();
        let mut logs: Vec<String> = Vec::new();

        // Pre-request: the script may rewrite the resolved request before it is sent.
        let mut prepared = prepared;
        if let Some(script) = request.scripts.pre_request.as_deref() {
            let input = ScriptInput {
                phase: Phase::PreRequest,
                request: ScriptRequest::from(&prepared),
                response: None,
                vars: scope.clone(),
                seed: env.seed,
            };
            let outcome = run_script(Arc::clone(&self.scripts), script.to_string(), input).await?;
            prepared = outcome.request.into_prepared();
            scope = outcome.vars;
            checks.extend(outcome.checks);
            logs.extend(outcome.logs);
        }

        // The echo reflects what actually goes out, after any pre-request rewrite —
        // and after every secret it may contain is known to the redactor, including
        // one that only the OS keychain could answer for.
        let echo = {
            let redactor = self.redactor();
            convert::echo_to_dto(&prepared, &redactor)
        };
        let sent = prepared.clone();
        let response = self.http.send(prepared).await?;

        // Post-response: the script inspects the response and records checks.
        if let Some(script) = request.scripts.post_response.as_deref() {
            let input = ScriptInput {
                phase: Phase::PostResponse,
                request: ScriptRequest::from(&sent),
                response: Some(ScriptResponse {
                    status: response.status,
                    headers: response.headers.clone(),
                    body: String::from_utf8_lossy(&response.body).into_owned(),
                    time_ms: response.timings.total.as_secs_f64() * 1_000.0,
                }),
                vars: scope,
                seed: env.seed,
            };
            let outcome = run_script(Arc::clone(&self.scripts), script.to_string(), input).await?;
            checks.extend(outcome.checks);
            logs.extend(outcome.logs);
        }

        // Declarative assertions run against the raw response, after the script.
        checks.extend(checks::evaluate(&request.assertions, &response));

        let redactor = self.redactor();
        Ok(Dispatch {
            echo,
            response: convert::response_to_dto(&response, &redactor),
            checks: redact_checks(checks, &redactor),
            logs: logs.into_iter().map(|log| redactor.redact(&log)).collect(),
        })
    }
}

/// Run one hook on the pooled runtime without blocking the async executor.
async fn run_script(
    sandbox: Arc<Sandbox>,
    source: String,
    input: ScriptInput,
) -> Result<ScriptOutcome, EngineError> {
    tokio::task::spawn_blocking(move || sandbox.run(&source, input))
        .await
        .map_err(|join| EngineError::Internal(format!("script task failed: {join}")))?
        .map_err(EngineError::from)
}

/// The variable scope a script sees, with secrets resolved.
///
/// Scripts run *inside* the engine and have no I/O, and every check and log is redacted
/// on the way out, so exposing a value here does not put it on the wire.
fn script_scope(
    vars: &VarSet,
    secrets: &dyn SecretStore,
) -> Result<BTreeMap<String, String>, EngineError> {
    let mut scope = BTreeMap::new();
    for (name, value) in &vars.entries {
        let text = match value {
            VarValue::Plain(text) => text.clone(),
            VarValue::Secret(secret_name) => secrets.resolve(secret_name)?.expose().to_string(),
        };
        scope.insert(name.clone(), text);
    }
    Ok(scope)
}

/// Scrub a check's name and message: a script can name a check after a secret.
fn redact_checks(checks: Vec<Check>, redactor: &Redactor) -> Vec<Check> {
    checks
        .into_iter()
        .map(|check| Check {
            name: redactor.redact(&check.name),
            passed: check.passed,
            message: check.message.map(|message| redactor.redact(&message)),
        })
        .collect()
}
