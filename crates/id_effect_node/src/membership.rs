//! SWIM membership via foca (sans-io), plus node monitors.
//!
//! [`Membership`] wraps a `foca` instance in accumulate-then-drain style:
//! every input (`announce`, `handle_packet`, `handle_timer`, `gossip`)
//! returns an [`Output`] with packets to send, timers to schedule, and
//! membership events. **The caller owns time and the network**, which is
//! what makes partition/flap tests fully deterministic (ADR 0009
//! § verification): tests shuttle packets and fire timers by hand; the
//! production driver pumps them over real transports and `tokio::time`.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use foca::{Config, Foca, Notification, PostcardCodec, Timer};
use rand::SeedableRng;
use rand::rngs::StdRng;
use serde::{Deserialize, Serialize};

use crate::id::NodeId;

/// A cluster member: node identity + the address peers dial it on.
///
/// The foca `Addr` is the dial address, so a node that restarts (new
/// incarnation, same address) wins the conflict against its stale self
/// and rejoins without waiting out suspicion timeouts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
  /// Node identity (name + boot incarnation).
  pub id: NodeId,
  /// Transport address peers use to reach this node.
  pub addr: String,
}

impl Member {
  /// Member record for a node reachable at `addr`.
  pub fn new(id: NodeId, addr: impl Into<String>) -> Self {
    Self {
      id,
      addr: addr.into(),
    }
  }
}

impl foca::Identity for Member {
  type Addr = String;

  fn renew(&self) -> Option<Self> {
    // Declared down: come back immediately as a fresh incarnation.
    Some(Self {
      id: NodeId::from_parts(self.id.name(), rand::random::<u128>()),
      addr: self.addr.clone(),
    })
  }

  fn addr(&self) -> String {
    self.addr.clone()
  }

  fn win_addr_conflict(&self, adversary: &Self) -> bool {
    // Higher incarnation is the newer boot of the same node.
    self.id.incarnation() > adversary.id.incarnation()
  }
}

/// Cluster-level membership event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MembershipEvent {
  /// A member became active.
  NodeUp(Member),
  /// An active member was declared down by the cluster.
  NodeDown(Member),
  /// This node's identity was declared down and automatically renewed.
  Rejoined(Member),
}

/// Everything one membership input produced. The caller must deliver the
/// packets, schedule the timers, and fan out the events.
#[derive(Default)]
pub struct Output {
  /// `(destination, datagram)` pairs to put on the wire.
  pub packets: Vec<(Member, Bytes)>,
  /// Timer events to feed back via [`Membership::handle_timer`] after
  /// the given delay.
  pub timers: Vec<(Timer<Member>, Duration)>,
  /// Membership changes to fan out to node monitors.
  pub events: Vec<MembershipEvent>,
}

/// Accumulating [`foca::Runtime`].
#[derive(Default)]
struct Collect {
  out: Output,
}

impl foca::Runtime<Member> for Collect {
  fn notify(&mut self, notification: Notification<'_, Member>) {
    match notification {
      Notification::MemberUp(member) => self
        .out
        .events
        .push(MembershipEvent::NodeUp(member.clone())),
      Notification::MemberDown(member) => self
        .out
        .events
        .push(MembershipEvent::NodeDown(member.clone())),
      Notification::Rejoin(member) => self
        .out
        .events
        .push(MembershipEvent::Rejoined(member.clone())),
      _ => {}
    }
  }

  fn send_to(&mut self, to: Member, data: &[u8]) {
    self.out.packets.push((to, Bytes::copy_from_slice(data)));
  }

  fn submit_after(&mut self, event: Timer<Member>, after: Duration) {
    self.out.timers.push((event, after));
  }
}

/// Membership engine failure (protocol errors from foca).
#[derive(Debug, thiserror::Error)]
#[error("membership: {0}")]
pub struct MembershipError(String);

/// Sans-io SWIM membership for one node.
pub struct Membership {
  foca: Foca<Member, PostcardCodec, StdRng, foca::NoCustomBroadcast>,
}

impl Membership {
  /// Engine with foca's LAN defaults sized for `estimated_size` nodes.
  ///
  /// `notify_down_members` is enabled so a node that was declared down
  /// while partitioned learns about it on first contact after heal and
  /// auto-renews its identity (the rejoin path).
  pub fn new(me: Member, estimated_size: u32) -> Self {
    let size = std::num::NonZeroU32::new(estimated_size.max(2)).expect("non-zero");
    let mut config = Config::new_lan(size);
    config.notify_down_members = true;
    Self::with_config(me, config)
  }

  /// Engine with an explicit foca [`Config`] (tests shrink the timers).
  pub fn with_config(me: Member, config: Config) -> Self {
    Self {
      foca: Foca::new(me, config, StdRng::from_os_rng(), PostcardCodec),
    }
  }

  /// Deterministic engine for tests: seeded RNG, explicit config.
  pub fn deterministic(me: Member, config: Config, seed: u64) -> Self {
    Self {
      foca: Foca::new(me, config, StdRng::seed_from_u64(seed), PostcardCodec),
    }
  }

  /// This node's current member record (changes on auto-rejoin).
  pub fn me(&self) -> &Member {
    self.foca.identity()
  }

  /// Currently-active members (excluding self).
  pub fn members(&self) -> Vec<Member> {
    self.foca.iter_members().map(|m| m.id().clone()).collect()
  }

  /// Join the cluster by announcing to a known peer.
  pub fn announce(&mut self, peer: Member) -> Result<Output, MembershipError> {
    let mut rt = Collect::default();
    self
      .foca
      .announce(peer, &mut rt)
      .map_err(|e| MembershipError(e.to_string()))?;
    Ok(rt.out)
  }

  /// Feed a datagram received from the network.
  pub fn handle_packet(&mut self, data: &[u8]) -> Result<Output, MembershipError> {
    let mut rt = Collect::default();
    self
      .foca
      .handle_data(data, &mut rt)
      .map_err(|e| MembershipError(e.to_string()))?;
    Ok(rt.out)
  }

  /// Fire a timer previously requested through [`Output::timers`].
  pub fn handle_timer(&mut self, event: Timer<Member>) -> Result<Output, MembershipError> {
    let mut rt = Collect::default();
    self
      .foca
      .handle_timer(event, &mut rt)
      .map_err(|e| MembershipError(e.to_string()))?;
    Ok(rt.out)
  }

  /// Proactively gossip cluster state (heartbeat).
  pub fn gossip(&mut self) -> Result<Output, MembershipError> {
    let mut rt = Collect::default();
    self
      .foca
      .gossip(&mut rt)
      .map_err(|e| MembershipError(e.to_string()))?;
    Ok(rt.out)
  }

  /// Gracefully leave the cluster.
  pub fn leave(&mut self) -> Result<Output, MembershipError> {
    let mut rt = Collect::default();
    self
      .foca
      .leave_cluster(&mut rt)
      .map_err(|e| MembershipError(e.to_string()))?;
    Ok(rt.out)
  }
}

// ── Node monitors ──────────────────────────────────────────────

/// Callback fired for every [`MembershipEvent`] (BEAM's
/// `erlang:monitor_node/2` generalized to a subscription).
pub type NodeMonitorFn = Arc<dyn Fn(&MembershipEvent) + Send + Sync>;

/// Fan-out registry for membership events.
#[derive(Clone, Default)]
pub struct NodeMonitors {
  subscribers: Arc<Mutex<Vec<NodeMonitorFn>>>,
}

impl NodeMonitors {
  /// Empty registry.
  pub fn new() -> Self {
    Self::default()
  }

  /// Subscribe to all membership events.
  pub fn subscribe(&self, f: NodeMonitorFn) {
    self
      .subscribers
      .lock()
      .expect("monitors mutex poisoned")
      .push(f);
  }

  /// Deliver events to every subscriber, in registration order.
  pub fn publish(&self, events: &[MembershipEvent]) {
    let subs = self
      .subscribers
      .lock()
      .expect("monitors mutex poisoned")
      .clone();
    for event in events {
      for sub in &subs {
        sub(event);
      }
    }
  }
}

// ── Deterministic timer wheel (test + driver building block) ─────────

/// Ordered queue of pending foca timers keyed by virtual deadline.
/// Production drivers use `tokio::time` instead; this exists for
/// deterministic tests and simulations.
#[derive(Default)]
pub struct TimerWheel {
  now: Duration,
  pending: VecDeque<(Duration, Timer<Member>)>,
}

impl TimerWheel {
  /// Empty wheel at t=0.
  pub fn new() -> Self {
    Self::default()
  }

  /// Schedule timers from an [`Output`].
  pub fn schedule(&mut self, timers: Vec<(Timer<Member>, Duration)>) {
    for (event, after) in timers {
      let deadline = self.now + after;
      // Insert sorted by deadline (stable for equal deadlines).
      let pos = self
        .pending
        .iter()
        .position(|(d, _)| *d > deadline)
        .unwrap_or(self.pending.len());
      self.pending.insert(pos, (deadline, event));
    }
  }

  /// Advance virtual time and pop every timer that became due.
  pub fn advance(&mut self, by: Duration) -> Vec<Timer<Member>> {
    self.now += by;
    let mut due = Vec::new();
    while let Some((deadline, _)) = self.pending.front() {
      if *deadline <= self.now {
        let (_, event) = self.pending.pop_front().expect("front exists");
        due.push(event);
      } else {
        break;
      }
    }
    due
  }

  /// Whether any timer is pending.
  pub fn is_empty(&self) -> bool {
    self.pending.is_empty()
  }
}
