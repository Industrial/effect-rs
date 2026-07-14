//! [`Session`] — an authenticated, framed connection to one peer node.
//!
//! Combines transport connect/accept with the cookie handshake and frame
//! codec. This is the unit the membership and remote-process layers build
//! on: one session per (local, remote) pair per connection epoch.

use crate::handshake::{Cookie, HandshakeError, initiate, respond};
use crate::id::NodeId;
use crate::transport::{Connection, TransportError};
use crate::wire::{Frame, WireError, decode, encode};

/// Session-level failure.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
  /// Transport connect/IO failure.
  #[error("transport: {0}")]
  Transport(#[from] TransportError),
  /// Handshake failed (bad cookie, version skew, protocol violation).
  #[error("handshake: {0}")]
  Handshake(#[from] HandshakeError),
  /// Frame codec failure — treat as protocol violation and close.
  #[error("wire: {0}")]
  Wire(#[from] WireError),
}

/// An authenticated connection to one peer.
pub struct Session {
  peer: NodeId,
  conn: Box<dyn Connection>,
}

impl Session {
  /// Dial-side: run the initiator handshake over an established
  /// connection.
  pub async fn initiate(
    conn: Box<dyn Connection>,
    me: &NodeId,
    cookie: &Cookie,
  ) -> Result<Self, SessionError> {
    let peer = initiate(conn.as_ref(), me, cookie).await?;
    Ok(Self { peer, conn })
  }

  /// Accept-side: run the responder handshake over an accepted
  /// connection.
  pub async fn respond(
    conn: Box<dyn Connection>,
    me: &NodeId,
    cookie: &Cookie,
  ) -> Result<Self, SessionError> {
    let peer = respond(conn.as_ref(), me, cookie).await?;
    Ok(Self { peer, conn })
  }

  /// The authenticated peer identity.
  pub fn peer(&self) -> &NodeId {
    &self.peer
  }

  /// Send one frame.
  pub async fn send(&self, frame: &Frame) -> Result<(), SessionError> {
    let bytes = encode(frame)?;
    self.conn.send(bytes).await?;
    Ok(())
  }

  /// Receive the next frame.
  pub async fn recv(&self) -> Result<Frame, SessionError> {
    let bytes = self.conn.recv().await?;
    Ok(decode(&bytes)?)
  }

  /// Close the underlying connection.
  pub async fn close(&self) {
    self.conn.close().await;
  }
}
