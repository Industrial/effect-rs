//! Cookie-authenticated mutual handshake (wire protocol v1).
//!
//! BEAM's shared-secret cookie scheme with a modern MAC: both sides prove
//! knowledge of the cookie by HMAC-SHA256 over the peer's nonce and their
//! own node name. The cookie never travels on the wire. This authenticates
//! but does **not** encrypt — pair with the QUIC transport (mTLS) or a
//! trusted network for confidentiality (ADR 0009 § security).

use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::id::NodeId;
use crate::transport::{Connection, TransportError};
use crate::wire::{Frame, HandshakeMsg, PROTOCOL_VERSION, decode, encode};

type HmacSha256 = Hmac<Sha256>;

/// Shared cluster secret. Compared only through MACs, never sent raw.
#[derive(Clone)]
pub struct Cookie(Vec<u8>);

impl Cookie {
  /// Cookie from any secret bytes/string.
  pub fn new(secret: impl Into<Vec<u8>>) -> Self {
    Self(secret.into())
  }

  fn digest(&self, nonce: u64, peer_name: &str) -> [u8; 32] {
    let mut mac = HmacSha256::new_from_slice(&self.0).expect("HMAC accepts any key length");
    mac.update(&nonce.to_be_bytes());
    mac.update(peer_name.as_bytes());
    mac.finalize().into_bytes().into()
  }

  fn verify(&self, nonce: u64, peer_name: &str, digest: &[u8; 32]) -> bool {
    let mut mac = HmacSha256::new_from_slice(&self.0).expect("HMAC accepts any key length");
    mac.update(&nonce.to_be_bytes());
    mac.update(peer_name.as_bytes());
    mac.verify_slice(digest).is_ok()
  }
}

/// Why a handshake failed.
#[derive(Debug, thiserror::Error)]
pub enum HandshakeError {
  /// Transport died mid-handshake.
  #[error("transport: {0}")]
  Transport(#[from] TransportError),
  /// Peer sent something that is not a valid handshake frame.
  #[error("protocol violation: {0}")]
  Protocol(String),
  /// Peer speaks a different protocol version.
  #[error("version mismatch: ours {ours}, theirs {theirs}")]
  VersionMismatch {
    /// Our protocol version.
    ours: u16,
    /// Peer's protocol version.
    theirs: u16,
  },
  /// Cookie proof failed — not part of this cluster.
  #[error("bad cookie")]
  BadCookie,
  /// Peer explicitly refused.
  #[error("rejected by peer: {0}")]
  Rejected(String),
}

async fn send_hs(conn: &dyn Connection, msg: HandshakeMsg) -> Result<(), HandshakeError> {
  let bytes =
    encode(&Frame::Handshake(msg)).map_err(|e| HandshakeError::Protocol(format!("encode: {e}")))?;
  conn.send(bytes).await?;
  Ok(())
}

async fn recv_hs(conn: &dyn Connection) -> Result<HandshakeMsg, HandshakeError> {
  let bytes = conn.recv().await?;
  match decode(&bytes).map_err(|e| HandshakeError::Protocol(format!("decode: {e}")))? {
    Frame::Handshake(msg) => Ok(msg),
    other => Err(HandshakeError::Protocol(format!(
      "expected handshake frame, got {other:?}"
    ))),
  }
}

/// Run the initiator side. On success the connection is authenticated and
/// the peer's [`NodeId`] is returned.
pub async fn initiate(
  conn: &dyn Connection,
  me: &NodeId,
  cookie: &Cookie,
) -> Result<NodeId, HandshakeError> {
  send_hs(
    conn,
    HandshakeMsg::Hello {
      version: PROTOCOL_VERSION,
      node: me.clone(),
    },
  )
  .await?;

  let (peer, their_nonce) = match recv_hs(conn).await? {
    HandshakeMsg::Challenge {
      version,
      node,
      nonce,
    } => {
      if version != PROTOCOL_VERSION {
        return Err(HandshakeError::VersionMismatch {
          ours: PROTOCOL_VERSION,
          theirs: version,
        });
      }
      (node, nonce)
    }
    HandshakeMsg::Rejected { reason } => return Err(HandshakeError::Rejected(reason)),
    other => {
      return Err(HandshakeError::Protocol(format!(
        "expected Challenge, got {other:?}"
      )));
    }
  };

  let my_nonce: u64 = rand::random();
  send_hs(
    conn,
    HandshakeMsg::ChallengeReply {
      digest: cookie.digest(their_nonce, me.name()),
      nonce: my_nonce,
    },
  )
  .await?;

  match recv_hs(conn).await? {
    HandshakeMsg::Welcome { digest } => {
      if cookie.verify(my_nonce, peer.name(), &digest) {
        Ok(peer)
      } else {
        Err(HandshakeError::BadCookie)
      }
    }
    HandshakeMsg::Rejected { reason } => Err(HandshakeError::Rejected(reason)),
    other => Err(HandshakeError::Protocol(format!(
      "expected Welcome, got {other:?}"
    ))),
  }
}

/// Run the responder side. On success the connection is authenticated and
/// the peer's [`NodeId`] is returned.
pub async fn respond(
  conn: &dyn Connection,
  me: &NodeId,
  cookie: &Cookie,
) -> Result<NodeId, HandshakeError> {
  let peer = match recv_hs(conn).await? {
    HandshakeMsg::Hello { version, node } => {
      if version != PROTOCOL_VERSION {
        let _ = send_hs(
          conn,
          HandshakeMsg::Rejected {
            reason: format!("version mismatch: have {PROTOCOL_VERSION}, got {version}"),
          },
        )
        .await;
        return Err(HandshakeError::VersionMismatch {
          ours: PROTOCOL_VERSION,
          theirs: version,
        });
      }
      node
    }
    other => {
      return Err(HandshakeError::Protocol(format!(
        "expected Hello, got {other:?}"
      )));
    }
  };

  let my_nonce: u64 = rand::random();
  send_hs(
    conn,
    HandshakeMsg::Challenge {
      version: PROTOCOL_VERSION,
      node: me.clone(),
      nonce: my_nonce,
    },
  )
  .await?;

  match recv_hs(conn).await? {
    HandshakeMsg::ChallengeReply { digest, nonce } => {
      if !cookie.verify(my_nonce, peer.name(), &digest) {
        let _ = send_hs(
          conn,
          HandshakeMsg::Rejected {
            reason: "bad cookie".to_string(),
          },
        )
        .await;
        return Err(HandshakeError::BadCookie);
      }
      send_hs(
        conn,
        HandshakeMsg::Welcome {
          digest: cookie.digest(nonce, me.name()),
        },
      )
      .await?;
      Ok(peer)
    }
    HandshakeMsg::Rejected { reason } => Err(HandshakeError::Rejected(reason)),
    other => Err(HandshakeError::Protocol(format!(
      "expected ChallengeReply, got {other:?}"
    ))),
  }
}
