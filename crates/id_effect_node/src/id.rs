//! [`NodeId`] — the wire-visible identity of a node.
//!
//! A `NodeId` is the logical [`NodeName`] (`name@host`) plus a 128-bit
//! random **incarnation** minted at boot. Two boots of the same logical
//! node never share an incarnation, so stale connections and stale pids
//! are always distinguishable from the live node (BEAM's creation tag,
//! widened so collisions are out of the picture).

use std::fmt;

use id_effect_process::NodeName;
use serde::{Deserialize, Serialize};

/// Wire-visible node identity: logical name + boot incarnation.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId {
  name: String,
  incarnation: u128,
}

impl NodeId {
  /// Mint a fresh identity for this boot of `name`.
  ///
  /// Incarnations are **time-ordered**: the high 64 bits are boot time in
  /// nanoseconds, the low 64 bits are random. Later boots of the same
  /// logical node therefore compare strictly greater (given sane wall
  /// clocks), which is what lets a restarted node win the identity
  /// conflict against its own stale record in membership.
  pub fn fresh(name: &NodeName) -> Self {
    let nanos = std::time::SystemTime::now()
      .duration_since(std::time::UNIX_EPOCH)
      .unwrap_or_default()
      .as_nanos() as u64;
    let incarnation = ((nanos as u128) << 64) | (rand::random::<u64>() as u128);
    Self {
      name: name.as_str().to_string(),
      incarnation,
    }
  }

  /// Rebuild an identity from its parts (wire decode, tests).
  pub fn from_parts(name: impl Into<String>, incarnation: u128) -> Self {
    Self {
      name: name.into(),
      incarnation,
    }
  }

  /// The logical node name (`name@host`).
  pub fn name(&self) -> &str {
    &self.name
  }

  /// The boot incarnation.
  pub fn incarnation(&self) -> u128 {
    self.incarnation
  }

  /// Same logical node, regardless of boot.
  pub fn same_node(&self, other: &NodeId) -> bool {
    self.name == other.name
  }
}

impl fmt::Display for NodeId {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}#{:x}", self.name, self.incarnation)
  }
}
