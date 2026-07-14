//! Wire protocol v1: versioned, postcard-encoded frames.
//!
//! Every frame on a connection is a [`Frame`]. The first bytes exchanged
//! after transport connect are handshake frames; only after a successful
//! handshake do data frames flow. The envelope payload (`Frame::Data`) is
//! opaque to this layer — the process layer validates it through Schema
//! at the trust boundary (ADR 0009 § wire protocol).

use bytes::Bytes;
use serde::{Deserialize, Serialize};

use crate::id::NodeId;

/// Protocol version spoken by this build. Nodes refuse to connect across
/// versions in v1 (no cross-version clusters — an explicit non-goal).
pub const PROTOCOL_VERSION: u16 = 1;

/// Everything that can travel on a connection.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Frame {
  /// Handshake sub-protocol (see [`crate::handshake`]).
  Handshake(HandshakeMsg),
  /// Opaque application payload (process-layer envelope).
  Data(Vec<u8>),
  /// Liveness probe.
  Ping(u64),
  /// Liveness reply.
  Pong(u64),
  /// Orderly close with a human-readable reason.
  Close(String),
}

/// Cookie-authenticated mutual handshake (HMAC-SHA256 challenge/response;
/// BEAM's cookie scheme with a modern MAC).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum HandshakeMsg {
  /// Initiator → responder: who I am, what I speak.
  Hello {
    /// Protocol version of the initiator.
    version: u16,
    /// Initiator identity.
    node: NodeId,
  },
  /// Responder → initiator: who I am + a nonce to prove cookie knowledge.
  Challenge {
    /// Protocol version of the responder.
    version: u16,
    /// Responder identity.
    node: NodeId,
    /// Nonce the initiator must MAC.
    nonce: u64,
  },
  /// Initiator → responder: MAC over the responder's nonce + my own nonce.
  ChallengeReply {
    /// `HMAC(cookie, responder_nonce ‖ initiator_name)`.
    digest: [u8; 32],
    /// Nonce the responder must MAC back (mutual auth).
    nonce: u64,
  },
  /// Responder → initiator: MAC over the initiator's nonce. Connection is
  /// established once the initiator verifies this.
  Welcome {
    /// `HMAC(cookie, initiator_nonce ‖ responder_name)`.
    digest: [u8; 32],
  },
  /// Either side: handshake refused (bad cookie, version skew, …).
  Rejected {
    /// Human-readable reason (never contains secrets).
    reason: String,
  },
}

/// Wire encode/decode failure.
#[derive(Debug, thiserror::Error)]
pub enum WireError {
  /// Postcard (de)serialization failed.
  #[error("wire codec error: {0}")]
  Codec(#[from] postcard::Error),
}

/// Encode a frame to bytes (postcard).
pub fn encode(frame: &Frame) -> Result<Bytes, WireError> {
  Ok(Bytes::from(postcard::to_allocvec(frame)?))
}

/// Decode a frame from bytes (postcard). Unknown/garbage input fails
/// cleanly — the caller must treat it as a protocol violation and close.
pub fn decode(bytes: &[u8]) -> Result<Frame, WireError> {
  Ok(postcard::from_bytes(bytes)?)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn frames_round_trip() {
    let frames = vec![
      Frame::Handshake(HandshakeMsg::Hello {
        version: PROTOCOL_VERSION,
        node: NodeId::from_parts("a@host", 7),
      }),
      Frame::Data(vec![1, 2, 3]),
      Frame::Ping(42),
      Frame::Pong(42),
      Frame::Close("bye".to_string()),
    ];
    for frame in frames {
      let bytes = encode(&frame).expect("encode");
      assert_eq!(decode(&bytes).expect("decode"), frame);
    }
  }

  #[test]
  fn garbage_fails_cleanly() {
    assert!(decode(&[0xff, 0xff, 0xff, 0xff, 0xff]).is_err());
  }
}
