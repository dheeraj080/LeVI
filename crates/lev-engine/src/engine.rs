// SPDX-License-Identifier: MIT OR Apache-2.0
//! The engine session loop.
//!
//! The whole engine is this: a handshake, then a dispatch loop over a [`Transport`].
//! Because the loop is transport-generic, `levi serve --stdio` and an in-process
//! request run *identical* code — there is no second code path to keep in sync.

use std::collections::BTreeMap;
use std::sync::Arc;

use lev_core::{
    prepare_request_with_seed, Check, MapSecretStore, Redactor, SecretStore, VarSet, VarValue,
};
use lev_http::HttpClient;
use lev_protocol::{
    negotiate, EchoDto, EnvDto, ErrorCode, Message, RequestDto, ResponseDto, Transport,
    PROTOCOL_VERSION,
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
    redactor: Redactor,
    http: HttpClient,
    scripts: Arc<Sandbox>,
}

impl Engine {
    /// Build an engine with an explicit secret store and redactor.
    ///
    /// The redactor must cover the same values the store can resolve, or a secret
    /// could escape in an echo.
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
            redactor,
            http: HttpClient::new()?,
            scripts: Arc::new(Sandbox::new(limits)?),
        })
    }

    /// Build an engine from an in-memory store, deriving the redactor from it.
    pub fn from_store(store: MapSecretStore) -> Result<Self, EngineError> {
        let redactor = crate::secrets::redactor_for(&store);
        Self::new(Arc::new(store), redactor)
    }

    /// Serve one session to completion over `transport`.
    ///
    /// Returns `Ok(())` for a clean end of stream, a `Stop`, or a rejected
    /// handshake. A rejected handshake sends exactly one `Error` first, so the peer
    /// learns *why* instead of just seeing a closed pipe.
    pub async fn serve<T: Transport>(&self, mut transport: T) -> Result<(), EngineError> {
        let Some(first) = transport.recv().await? else {
            return Ok(());
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

        loop {
            let Some(message) = transport.recv().await? else {
                return Ok(());
            };
            match message {
                Message::SendRequest { request, env } => {
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
                Message::Stop { .. } => return Ok(()),
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

        // The echo reflects what actually goes out, after any pre-request rewrite.
        let echo = convert::echo_to_dto(&prepared, &self.redactor);
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

        Ok(Dispatch {
            echo,
            response: convert::response_to_dto(&response, &self.redactor),
            checks: redact_checks(checks, &self.redactor),
            logs: logs
                .into_iter()
                .map(|log| self.redactor.redact(&log))
                .collect(),
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
