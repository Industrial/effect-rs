//! Process identity: [`NodeName`], [`Pid`], and [`MonitorRef`].
//!
//! A [`Pid`] is location-transparent for `send`: it names a process by
//! `(node, local id, incarnation)`. Local ids are allocated from a
//! monotonically increasing counter and never reused within a node
//! incarnation, so a `Pid` can never silently alias a different process
//! (ADR 0009 § Process model).

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Logical node name, BEAM-style `name@host`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeName(Arc<str>);

impl NodeName {
  /// Node name from an arbitrary string (conventionally `name@host`).
  pub fn new(name: impl Into<Arc<str>>) -> Self {
    Self(name.into())
  }

  /// The non-distributed default, mirroring BEAM's `nonode@nohost`.
  pub fn local() -> Self {
    Self(Arc::from("nonode@nohost"))
  }

  /// Borrow the raw name.
  pub fn as_str(&self) -> &str {
    &self.0
  }
}

impl fmt::Display for NodeName {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(&self.0)
  }
}

/// Process identifier: node + local id + incarnation.
///
/// `incarnation` is the node incarnation the process was spawned under; it
/// exists so that remote peers can detect pids from a previous life of a
/// restarted node (wire protocol v1). Within one node run it is constant.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Pid {
  node: NodeName,
  id: u64,
  incarnation: u32,
}

impl Pid {
  pub(crate) fn new(node: NodeName, id: u64, incarnation: u32) -> Self {
    Self {
      node,
      id,
      incarnation,
    }
  }

  /// Rebuild a pid from its parts (wire decode). A pid built this way is
  /// only meaningful on the node that originally allocated it.
  pub fn from_parts(node: NodeName, id: u64, incarnation: u32) -> Self {
    Self::new(node, id, incarnation)
  }

  /// The node this process lives on.
  pub fn node(&self) -> &NodeName {
    &self.node
  }

  /// Node-local process id (never reused within a node incarnation).
  pub fn local_id(&self) -> u64 {
    self.id
  }

  /// Node incarnation this pid belongs to.
  pub fn incarnation(&self) -> u32 {
    self.incarnation
  }
}

impl fmt::Display for Pid {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "<{}.{}.{}>", self.node, self.id, self.incarnation)
  }
}

/// One-shot monitor reference — globally unique within the OS process.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MonitorRef(u64);

static NEXT_MONITOR: AtomicU64 = AtomicU64::new(1);

impl MonitorRef {
  /// Allocate a fresh, unique reference.
  pub fn fresh() -> Self {
    Self(NEXT_MONITOR.fetch_add(1, Ordering::Relaxed))
  }

  /// Raw value (stable within this OS process; used by the wire layer).
  pub fn into_inner(self) -> u64 {
    self.0
  }

  /// Rebuild a monitor ref from its wire value (distribution layer).
  pub fn from_inner(v: u64) -> Self {
    Self(v)
  }
}

impl fmt::Display for MonitorRef {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "#Ref<{}>", self.0)
  }
}
