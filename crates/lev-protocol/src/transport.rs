// SPDX-License-Identifier: MIT OR Apache-2.0
//! Transports.
//!
//! A transport carries [`Message`]s in both directions. Everything above it — the
//! engine loop, the client, the CLI — is transport-agnostic, so the same code path
//! serves an in-process request and a request against a spawned daemon.
//!
//! Two are implemented in Phase 1:
//!
//! - [`InProcessTransport`] — a typed `tokio` channel pair. No encoding, no
//!   serialization, no process. This is the interactive path.
//! - [`NdjsonTransport`] — one JSON object per line over any async reader/writer.
//!   This is the `levi serve --stdio` path, and the shape remote agents reuse.

use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;

use crate::error::TransportError;
use crate::message::Message;

/// A bidirectional channel carrying protocol messages.
///
/// `recv` returns `Ok(None)` exactly once, when the peer has hung up cleanly; after
/// that the transport is not usable. `send` fails once the peer is gone.
#[async_trait]
pub trait Transport: Send {
    /// Receive the next message, or `None` on a clean end of stream.
    async fn recv(&mut self) -> Result<Option<Message>, TransportError>;

    /// Send a message, flushing it if the transport buffers.
    async fn send(&mut self, message: Message) -> Result<(), TransportError>;
}

/// Create a connected pair of in-process endpoints.
///
/// `capacity` bounds the number of in-flight messages in each direction, so a slow
/// reader exerts back-pressure instead of growing a queue without limit.
pub fn in_process_channel(capacity: usize) -> (InProcessTransport, InProcessTransport) {
    assert!(capacity > 0, "in-process channel capacity must be non-zero");
    let (a_out, b_in) = mpsc::channel(capacity);
    let (b_out, a_in) = mpsc::channel(capacity);
    (
        InProcessTransport {
            outgoing: a_out,
            incoming: a_in,
        },
        InProcessTransport {
            outgoing: b_out,
            incoming: b_in,
        },
    )
}

/// One endpoint of an in-process, typed-channel transport.
///
/// Messages are moved, not encoded: this is the same protocol boundary with zero
/// serialization cost.
pub struct InProcessTransport {
    outgoing: mpsc::Sender<Message>,
    incoming: mpsc::Receiver<Message>,
}

#[async_trait]
impl Transport for InProcessTransport {
    async fn recv(&mut self) -> Result<Option<Message>, TransportError> {
        Ok(self.incoming.recv().await)
    }

    async fn send(&mut self, message: Message) -> Result<(), TransportError> {
        self.outgoing
            .send(message)
            .await
            .map_err(|_| TransportError::Closed)
    }
}

/// Newline-delimited JSON over an async reader and writer.
///
/// Each message is one JSON object on one line. A blank line is ignored, which
/// makes trailing newlines harmless. Exactly one message per line means a reader
/// that falls behind still sees whole messages — there is no framing ambiguity.
pub struct NdjsonTransport<R, W> {
    reader: BufReader<R>,
    writer: W,
}

impl<R, W> NdjsonTransport<R, W>
where
    R: tokio::io::AsyncRead + Unpin,
{
    pub fn new(reader: R, writer: W) -> Self {
        Self {
            reader: BufReader::new(reader),
            writer,
        }
    }
}

impl NdjsonTransport<tokio::io::Stdin, tokio::io::Stdout> {
    /// The stdio transport used by `levi serve --stdio`.
    ///
    /// Standard output carries protocol messages and nothing else; all human-facing
    /// output must go to standard error.
    pub fn stdio() -> Self {
        Self::new(tokio::io::stdin(), tokio::io::stdout())
    }
}

#[async_trait]
impl<R, W> Transport for NdjsonTransport<R, W>
where
    R: tokio::io::AsyncRead + Unpin + Send,
    W: tokio::io::AsyncWrite + Unpin + Send,
{
    async fn recv(&mut self) -> Result<Option<Message>, TransportError> {
        loop {
            let mut line = String::new();
            let read = self.reader.read_line(&mut line).await?;
            if read == 0 {
                return Ok(None);
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            return Ok(Some(serde_json::from_str(trimmed)?));
        }
    }

    async fn send(&mut self, message: Message) -> Result<(), TransportError> {
        let mut encoded = serde_json::to_string(&message)?;
        encoded.push('\n');
        self.writer.write_all(encoded.as_bytes()).await?;
        self.writer.flush().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::version::Version;

    #[tokio::test]
    async fn in_process_channel_moves_messages_without_encoding() {
        let (mut client, mut engine) = in_process_channel(4);
        client
            .send(Message::Hello {
                protocol_version: Version::new(0, 1),
                capabilities: vec![],
            })
            .await
            .unwrap();

        let received = engine.recv().await.unwrap().unwrap();
        assert_eq!(received.type_name(), "hello");

        engine
            .send(Message::Ready {
                version: Version::new(0, 1),
                engine: "test".to_string(),
            })
            .await
            .unwrap();
        assert_eq!(client.recv().await.unwrap().unwrap().type_name(), "ready");
    }

    #[tokio::test]
    async fn in_process_channel_reports_a_clean_close() {
        let (client, mut engine) = in_process_channel(4);
        drop(client);
        assert_eq!(engine.recv().await.unwrap(), None);
    }

    #[tokio::test]
    async fn in_process_send_fails_once_the_peer_is_gone() {
        let (mut client, engine) = in_process_channel(4);
        drop(engine);
        let error = client
            .send(Message::Stop { graceful: true })
            .await
            .unwrap_err();
        assert!(matches!(error, TransportError::Closed));
    }

    #[tokio::test]
    async fn ndjson_transport_round_trips_over_a_pipe() {
        let (near, far) = tokio::io::duplex(4096);
        let (near_read, near_write) = tokio::io::split(near);
        let (far_read, far_write) = tokio::io::split(far);
        let mut client = NdjsonTransport::new(near_read, near_write);
        let mut engine = NdjsonTransport::new(far_read, far_write);

        client.send(Message::Stop { graceful: true }).await.unwrap();
        assert_eq!(
            engine.recv().await.unwrap().unwrap(),
            Message::Stop { graceful: true }
        );

        engine
            .send(Message::error(crate::message::ErrorCode::Config, "nope"))
            .await
            .unwrap();
        assert_eq!(client.recv().await.unwrap().unwrap().type_name(), "error");

        // Closing the client's write half ends the stream for the engine.
        drop(client);
        assert_eq!(engine.recv().await.unwrap(), None);
    }

    #[tokio::test]
    async fn ndjson_transport_ignores_blank_lines() {
        use tokio::io::AsyncWriteExt as _;

        // `duplex` gives two connected ends: writes to one are read from the other.
        let (mut writer, reader) = tokio::io::duplex(4096);
        let mut engine = NdjsonTransport::new(reader, tokio::io::sink());

        writer.write_all(b"\n   \n").await.unwrap();
        writer
            .write_all(b"{\"type\":\"stop\",\"graceful\":false}\n")
            .await
            .unwrap();
        assert_eq!(
            engine.recv().await.unwrap().unwrap(),
            Message::Stop { graceful: false }
        );
    }

    #[tokio::test]
    async fn ndjson_transport_reports_malformed_lines() {
        use tokio::io::AsyncWriteExt as _;

        let (mut writer, reader) = tokio::io::duplex(4096);
        let mut engine = NdjsonTransport::new(reader, tokio::io::sink());

        writer.write_all(b"not json\n").await.unwrap();
        let error = engine.recv().await.unwrap_err();
        assert!(matches!(error, TransportError::Decode(_)));
    }
}
