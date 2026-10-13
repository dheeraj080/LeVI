// SPDX-License-Identifier: MIT OR Apache-2.0
//! The engine session loop.
//!
//! The whole engine is this: a handshake, then a dispatch loop over a [`Transport`].
//! Because the loop is transport-generic, `levi serve --stdio` and an in-process
//! request run *identical* code — there is no second code path to keep in sync.

use std::sync::Arc;

use lev_core::{prepare_request_with_seed, MapSecretStore, Redactor, SecretStore};
use lev_http::HttpClient;
use lev_protocol::{
    negotiate, EchoDto, EnvDto, ErrorCode, Message, RequestDto, ResponseDto, Transport,
    PROTOCOL_VERSION,
};

use crate::convert;
use crate::error::EngineError;
use crate::ENGINE_VERSION;

/// The engine, holding the transport-independent state a session needs.
pub struct Engine {
    secrets: Arc<dyn SecretStore>,
    redactor: Redactor,
    http: HttpClient,
}

impl Engine {
    /// Build an engine with an explicit secret store and redactor.
    ///
    /// The redactor must cover the same values the store can resolve, or a secret
    /// could escape in an echo.
    pub fn new(secrets: Arc<dyn SecretStore>, redactor: Redactor) -> Result<Self, EngineError> {
        Ok(Self {
            secrets,
            redactor,
            http: HttpClient::new()?,
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
                        Ok((echo, response)) => Message::RequestResult {
                            request: echo,
                            response,
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

    /// Resolve and send one request, returning both halves redacted.
    pub async fn dispatch(
        &self,
        request: RequestDto,
        env: EnvDto,
    ) -> Result<(EchoDto, ResponseDto), EngineError> {
        let (collection, vars) = convert::env_from_dto(&env)?;
        let request = convert::request_from_dto(request)?;
        let prepared = prepare_request_with_seed(
            &collection,
            &request,
            vars,
            self.secrets.as_ref(),
            env.seed,
        )?;

        let echo = convert::echo_to_dto(&prepared, &self.redactor);
        let response = self.http.send(prepared).await?;
        Ok((echo, convert::response_to_dto(&response, &self.redactor)))
    }
}
