//! Transport abstraction: how raw frames move between nodes.
//!
//! Three implementations per ADR 0009:
//! - [`memory::InMemoryTransport`] — first-class, deterministic, supports
//!   scripted partitions (the chaos-test backbone).
//! - [`tcp`] — length-delimited frames over Tokio TCP (the fallback).
//! - `quic` (feature `quic`) — quinn + rustls (the production path).
//!
//! A transport moves **bytes**; framing, handshake, and semantics live
//! above it. Connections are bidirectional byte-frame pipes.

use bytes::Bytes;

pub mod memory;
pub mod tcp;

#[cfg(feature = "quic")]
pub mod quic;

/// Transport-level failure.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
  /// The peer is unreachable or refused the connection.
  #[error("connect failed: {0}")]
  Connect(String),
  /// The connection was lost (peer closed, network dropped, partition).
  #[error("connection closed")]
  Closed,
  /// Listener/bind failure.
  #[error("bind failed: {0}")]
  Bind(String),
  /// I/O error underneath.
  #[error("io: {0}")]
  Io(String),
}

impl From<std::io::Error> for TransportError {
  fn from(err: std::io::Error) -> Self {
    TransportError::Io(err.to_string())
  }
}

/// A bidirectional, ordered, frame-oriented connection to one peer.
///
/// Guarantees (normative): frames are delivered in order per direction or
/// the connection reports [`TransportError::Closed`]; no partial frames.
#[async_trait::async_trait]
pub trait Connection: Send + Sync {
  /// Send one frame. Errors mean the connection is dead.
  async fn send(&self, frame: Bytes) -> Result<(), TransportError>;
  /// Receive the next frame. Errors mean the connection is dead.
  async fn recv(&self) -> Result<Bytes, TransportError>;
  /// Close the connection (idempotent).
  async fn close(&self);
  /// Address of the remote end, for diagnostics.
  fn peer_addr(&self) -> String;
}

/// Accepts inbound connections on a bound address.
#[async_trait::async_trait]
pub trait Listener: Send + Sync {
  /// Wait for the next inbound connection.
  async fn accept(&self) -> Result<Box<dyn Connection>, TransportError>;
  /// The address this listener is bound to (resolves ephemeral ports).
  fn local_addr(&self) -> String;
}

/// A way of dialing and listening — the only thing the distribution layer
/// needs from the network.
#[async_trait::async_trait]
pub trait Transport: Send + Sync {
  /// Dial a peer address.
  async fn connect(&self, addr: &str) -> Result<Box<dyn Connection>, TransportError>;
  /// Bind a listener.
  async fn listen(&self, addr: &str) -> Result<Box<dyn Listener>, TransportError>;
}
