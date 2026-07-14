//! Process-layer envelopes: what travels inside [`Frame::Data`].
//!
//! The transport layer sees opaque bytes; this module defines their shape.
//! Payloads (`args`, `msg`, `reply`) are themselves postcard-encoded typed
//! values, decoded at the trust boundary by the [`crate::registry`] entry
//! for the target behavior — “parse, don't validate” at every node
//! boundary (ADR 0009 § wire protocol).
//!
//! [`Frame::Data`]: crate::wire::Frame::Data

use id_effect_process::{ExitReason, NodeName, Pid};
use serde::{Deserialize, Serialize};

/// Wire form of [`Pid`] (the process crate stays serde-free).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PidWire {
  /// Logical node name (`name@host`).
  pub node: String,
  /// Node-local process id.
  pub id: u64,
  /// Node incarnation the pid was minted under.
  pub incarnation: u32,
}

impl From<&Pid> for PidWire {
  fn from(pid: &Pid) -> Self {
    Self {
      node: pid.node().as_str().to_string(),
      id: pid.local_id(),
      incarnation: pid.incarnation(),
    }
  }
}

impl PidWire {
  /// Rebuild a [`Pid`] from its wire form.
  pub fn to_pid(&self) -> Pid {
    Pid::from_parts(NodeName::new(self.node.as_str()), self.id, self.incarnation)
  }
}

/// Wire form of [`ExitReason`] — exit reasons are plain data.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExitReasonWire {
  /// Graceful completion.
  Normal,
  /// Graceful supervisor stop.
  Shutdown,
  /// Abnormal termination, rendered to a string.
  Error(String),
  /// Untrappable kill.
  Killed,
  /// Fiber interrupted.
  Interrupted,
  /// Target did not exist.
  NoProc,
  /// Hosting node unreachable.
  NoConnection,
}

impl From<&ExitReason> for ExitReasonWire {
  fn from(reason: &ExitReason) -> Self {
    match reason {
      ExitReason::Normal => Self::Normal,
      ExitReason::Shutdown => Self::Shutdown,
      ExitReason::Error(e) => Self::Error(e.clone()),
      ExitReason::Killed => Self::Killed,
      ExitReason::Interrupted => Self::Interrupted,
      ExitReason::NoProc => Self::NoProc,
      ExitReason::NoConnection => Self::NoConnection,
    }
  }
}

impl From<&ExitReasonWire> for ExitReason {
  fn from(reason: &ExitReasonWire) -> Self {
    match reason {
      ExitReasonWire::Normal => Self::Normal,
      ExitReasonWire::Shutdown => Self::Shutdown,
      ExitReasonWire::Error(e) => Self::Error(e.clone()),
      ExitReasonWire::Killed => Self::Killed,
      ExitReasonWire::Interrupted => Self::Interrupted,
      ExitReasonWire::NoProc => Self::NoProc,
      ExitReasonWire::NoConnection => Self::NoConnection,
    }
  }
}

/// W3C-style trace context propagated with cross-node messages (consumed
/// by `id_effect_opentelemetry::propagation`; empty when tracing is off).
///
/// Inject with `id_effect_opentelemetry::propagation::inject_current_trace_context`
/// into a mutable `TraceContext`; extract with `extract_trace_context_from_headers`.
pub type TraceContext = Vec<(String, String)>;

/// Empty trace bag.
pub fn empty_trace() -> TraceContext {
  Vec::new()
}

/// Single header pair (tests / manual propagation).
pub fn trace_pair(key: impl Into<String>, value: impl Into<String>) -> TraceContext {
  vec![(key.into(), value.into())]
}

/// Everything the process layer sends between nodes.
///
/// Delivery contract (normative, ADR 0009): at-most-once, ordered per
/// connection epoch, dropped on disconnect. `Call`/`Spawn` correlate
/// request and reply by id; ids are unique per (node, boot).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ProcessEnvelope {
  /// Fire-and-forget cast to a remote process.
  Cast {
    /// Target process.
    to: PidWire,
    /// Postcard-encoded `P::Cast`.
    msg: Vec<u8>,
    /// Distributed tracing context.
    trace: TraceContext,
  },
  /// Bare message delivered to `handle_info`.
  Info {
    /// Target process.
    to: PidWire,
    /// Postcard-encoded `P::Info`.
    msg: Vec<u8>,
    /// Distributed tracing context.
    trace: TraceContext,
  },
  /// Synchronous request expecting a [`ProcessEnvelope::Reply`].
  Call {
    /// Target process.
    to: PidWire,
    /// Correlation id, unique per sender boot.
    call_id: u64,
    /// Postcard-encoded `P::Call`.
    msg: Vec<u8>,
    /// Distributed tracing context.
    trace: TraceContext,
  },
  /// Response to a [`ProcessEnvelope::Call`].
  Reply {
    /// Correlation id of the originating call.
    call_id: u64,
    /// `Ok(postcard P::Reply)` or a rendered failure.
    result: Result<Vec<u8>, RemoteFault>,
  },
  /// Spawn a registered behavior on the receiving node.
  Spawn {
    /// Correlation id, unique per sender boot.
    spawn_id: u64,
    /// Behavior name in the receiver's [`crate::registry::BehaviorRegistry`].
    behavior: String,
    /// Postcard-encoded `P::Args`.
    args: Vec<u8>,
    /// Pid on the *requesting* node to link the child to (remote link),
    /// if any.
    link_to: Option<PidWire>,
  },
  /// Response to a [`ProcessEnvelope::Spawn`].
  SpawnReply {
    /// Correlation id of the originating spawn.
    spawn_id: u64,
    /// The new pid, or why the spawn failed.
    result: Result<PidWire, RemoteFault>,
  },
  /// Set up a monitor: watcher on the sending node, target on the
  /// receiving node. The receiver answers with `Down` exactly once.
  Monitor {
    /// Wire-unique monitor reference (sender-allocated).
    monitor: u64,
    /// Target process on the receiving node.
    target: PidWire,
  },
  /// Tear down a monitor before it fires.
  Demonitor {
    /// Reference from the original `Monitor`.
    monitor: u64,
    /// Target process it was watching.
    target: PidWire,
  },
  /// Monitor fired: the watched process exited.
  Down {
    /// Reference from the original `Monitor`.
    monitor: u64,
    /// The process that exited.
    target: PidWire,
    /// Why it exited.
    reason: ExitReasonWire,
  },
  /// Link two processes across nodes (bidirectional fate-sharing).
  Link {
    /// Process on the sending node.
    from: PidWire,
    /// Process on the receiving node.
    to: PidWire,
  },
  /// Remove a cross-node link.
  Unlink {
    /// Process on the sending node.
    from: PidWire,
    /// Process on the receiving node.
    to: PidWire,
  },
  /// Exit signal crossing the wire (link propagation, remote `exit/2`).
  ExitSignal {
    /// Process the signal is about.
    from: PidWire,
    /// Process to deliver the signal to.
    to: PidWire,
    /// Why.
    reason: ExitReasonWire,
  },
}

/// Why a remote operation failed, as data.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RemoteFault {
  /// Target process does not exist (or already exited).
  NoProc,
  /// No behavior registered under the requested name.
  UnknownBehavior(String),
  /// Payload failed typed decode at the trust boundary.
  Decode(String),
  /// The callee exited before replying.
  Down(ExitReasonWire),
  /// The callee's node-side call timed out.
  Timeout,
  /// Behavior init failed.
  SpawnFailed(String),
}

impl std::fmt::Display for RemoteFault {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      RemoteFault::NoProc => f.write_str("noproc"),
      RemoteFault::UnknownBehavior(name) => write!(f, "unknown behavior: {name}"),
      RemoteFault::Decode(e) => write!(f, "decode: {e}"),
      RemoteFault::Down(r) => write!(f, "down: {r:?}"),
      RemoteFault::Timeout => f.write_str("timeout"),
      RemoteFault::SpawnFailed(e) => write!(f, "spawn failed: {e}"),
    }
  }
}

/// Encode an envelope for a [`Frame::Data`] payload.
///
/// [`Frame::Data`]: crate::wire::Frame::Data
pub fn encode_envelope(envelope: &ProcessEnvelope) -> Result<Vec<u8>, postcard::Error> {
  postcard::to_allocvec(envelope)
}

/// Decode a [`Frame::Data`] payload. Garbage fails cleanly.
///
/// [`Frame::Data`]: crate::wire::Frame::Data
pub fn decode_envelope(bytes: &[u8]) -> Result<ProcessEnvelope, postcard::Error> {
  postcard::from_bytes(bytes)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn envelopes_round_trip() {
    let pid = PidWire {
      node: "a@host".to_string(),
      id: 7,
      incarnation: 1,
    };
    let cases = vec![
      ProcessEnvelope::Cast {
        to: pid.clone(),
        msg: vec![1, 2],
        trace: vec![("traceparent".to_string(), "00-abc".to_string())],
      },
      ProcessEnvelope::Call {
        to: pid.clone(),
        call_id: 9,
        msg: vec![3],
        trace: Vec::new(),
      },
      ProcessEnvelope::Reply {
        call_id: 9,
        result: Err(RemoteFault::Timeout),
      },
      ProcessEnvelope::Down {
        monitor: 4,
        target: pid,
        reason: ExitReasonWire::NoConnection,
      },
    ];
    for envelope in cases {
      let bytes = encode_envelope(&envelope).expect("encode");
      assert_eq!(decode_envelope(&bytes).expect("decode"), envelope);
    }
  }
}
