// SPDX-License-Identifier: MIT OR Apache-2.0
//! The client side of the protocol.
//!
//! A [`Client`] is transport-generic: the same code drives an in-process engine and
//! a spawned `levi serve --stdio` daemon. That is the property the UI depends on —
//! it cannot tell local from remote execution because there is no difference to see.

use lev_protocol::{
    in_process_channel, Capability, EchoDto, EnvDto, InProcessTransport, Message, ProtocolError,
    RequestDto, ResponseDto, Transport, TransportError, Version, PROTOCOL_VERSION,
};
use tokio::task::JoinHandle;

use crate::engine::Engine;
use crate::error::EngineError;

/// A connected protocol client.
pub struct Client<T: Transport> {
    transport: T,
    version: Version,
    engine: String,
}

impl<T: Transport> Client<T> {
    /// Perform the `Hello` / `Ready` handshake.
    pub async fn connect(
        mut transport: T,
        capabilities: Vec<Capability>,
    ) -> Result<Self, EngineError> {
        transport
            .send(Message::Hello {
                protocol_version: PROTOCOL_VERSION,
                capabilities,
            })
            .await?;

        match transport.recv().await? {
            Some(Message::Ready { version, engine }) => Ok(Self {
                transport,
                version,
                engine,
            }),
            Some(Message::Error { code, msg }) => Err(EngineError::Remote { code, msg }),
            Some(other) => Err(EngineError::Protocol(ProtocolError::Unexpected(
                other.type_name().to_string(),
            ))),
            None => Err(EngineError::Transport(TransportError::Closed)),
        }
    }

    /// The negotiated protocol version.
    pub fn version(&self) -> Version {
        self.version
    }

    /// The engine's own version string.
    pub fn engine_version(&self) -> &str {
        &self.engine
    }

    /// Send one request and await its result.
    pub async fn send_request(
        &mut self,
        request: RequestDto,
        env: EnvDto,
    ) -> Result<(EchoDto, ResponseDto), EngineError> {
        self.transport
            .send(Message::SendRequest { request, env })
            .await?;
        match self.transport.recv().await? {
            Some(Message::RequestResult { request, response }) => Ok((request, response)),
            Some(Message::Error { code, msg }) => Err(EngineError::Remote { code, msg }),
            Some(other) => Err(EngineError::Protocol(ProtocolError::Unexpected(
                other.type_name().to_string(),
            ))),
            None => Err(EngineError::Transport(TransportError::Closed)),
        }
    }

    /// Ask the engine to wind down.
    pub async fn stop(&mut self, graceful: bool) -> Result<(), EngineError> {
        self.transport.send(Message::Stop { graceful }).await?;
        Ok(())
    }

    /// Receive the next unsolicited message (logs, and later snapshots).
    pub async fn next(&mut self) -> Result<Option<Message>, EngineError> {
        Ok(self.transport.recv().await?)
    }
}

/// Wire an engine to an in-process transport and return a connected client.
///
/// This is the interactive path: the same protocol, with no encoding and no process.
pub async fn in_process_client(
    engine: Engine,
    capacity: usize,
) -> Result<
    (
        Client<InProcessTransport>,
        JoinHandle<Result<(), EngineError>>,
    ),
    EngineError,
> {
    let (client_end, engine_end) = in_process_channel(capacity);
    let handle = tokio::spawn(async move { engine.serve(engine_end).await });
    let client = Client::connect(client_end, vec![Capability::Interactive]).await?;
    Ok((client, handle))
}
