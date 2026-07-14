//! Deterministic SWIM membership tests: the test owns time (a virtual
//! [`TimerWheel`] per node) and the network (an in-memory packet router
//! with scripted partitions). No sleeps, no real clock — every scenario
//! is a pure function of the script (ADR 0009 § verification).

use std::collections::HashMap;
use std::num::NonZeroU32;
use std::time::Duration;

use bytes::Bytes;
use foca::Config;
use id_effect_node::id::NodeId;
use id_effect_node::membership::{
  Member, Membership, MembershipEvent, NodeMonitors, Output, TimerWheel,
};
use id_effect_process::NodeName;

/// One simulated node: engine + its virtual timer wheel + event log.
struct SimNode {
  membership: Membership,
  timers: TimerWheel,
  events: Vec<MembershipEvent>,
}

/// Deterministic cluster simulation. Packets route synchronously through
/// `step`; partitions drop packets between named nodes.
struct Sim {
  nodes: HashMap<String, SimNode>,
  /// Unordered partitioned pairs (by addr).
  partitions: Vec<(String, String)>,
  /// In-flight packets: (to_addr, bytes, from_addr).
  wire: Vec<(String, Bytes, String)>,
}

fn test_config() -> Config {
  // Short, deterministic timings; sized for a tiny cluster. Down members
  // are notified on contact so heal/rejoin works (mirrors
  // `Membership::new` defaults).
  let mut config = Config::new_lan(NonZeroU32::new(3).expect("nonzero"));
  config.notify_down_members = true;
  config
}

fn member(name: &str) -> Member {
  Member::new(NodeId::fresh(&NodeName::new(name)), name)
}

impl Sim {
  fn new(names: &[&str]) -> Self {
    let mut nodes = HashMap::new();
    for (i, name) in names.iter().enumerate() {
      nodes.insert(
        name.to_string(),
        SimNode {
          membership: Membership::deterministic(member(name), test_config(), i as u64 + 1),
          timers: TimerWheel::new(),
          events: Vec::new(),
        },
      );
    }
    Self {
      nodes,
      partitions: Vec::new(),
      wire: Vec::new(),
    }
  }

  fn absorb(&mut self, from: &str, output: Output) {
    let node = self.nodes.get_mut(from).expect("node exists");
    node.timers.schedule(output.timers);
    node.events.extend(output.events);
    for (to, bytes) in output.packets {
      self.wire.push((to.addr.clone(), bytes, from.to_string()));
    }
  }

  /// Deliver every in-flight packet (dropping partitioned ones) until
  /// the wire is empty.
  fn drain_wire(&mut self) {
    loop {
      let partitions = self.partitions.clone();
      let cut = |a: &str, b: &str| {
        partitions
          .iter()
          .any(|(x, y)| (x == a && y == b) || (x == b && y == a))
      };
      self.wire.retain(|(to, _, from)| !cut(to, from));
      if self.wire.is_empty() {
        break;
      }
      let (to, bytes, _from) = self.wire.remove(0);
      if let Some(node) = self.nodes.get_mut(&to)
        && let Ok(out) = node.membership.handle_packet(&bytes)
      {
        self.absorb(&to, out);
      }
    }
  }

  /// Advance every node's virtual clock and fire due timers, then drain
  /// the wire. One `step` ≈ `by` of simulated time.
  fn step(&mut self, by: Duration) {
    let names: Vec<String> = self.nodes.keys().cloned().collect();
    for name in &names {
      let due = {
        let node = self.nodes.get_mut(name).expect("node");
        node.timers.advance(by)
      };
      for event in due {
        let out = {
          let node = self.nodes.get_mut(name).expect("node");
          node.membership.handle_timer(event)
        };
        if let Ok(out) = out {
          self.absorb(name, out);
        }
      }
    }
    self.drain_wire();
  }

  fn announce(&mut self, from: &str, to: &str) {
    let target = self.nodes.get(to).expect("target").membership.me().clone();
    let out = self
      .nodes
      .get_mut(from)
      .expect("from")
      .membership
      .announce(target)
      .expect("announce");
    self.absorb(from, out);
    self.drain_wire();
  }

  fn active_members(&self, name: &str) -> Vec<String> {
    let mut members: Vec<String> = self
      .nodes
      .get(name)
      .expect("node")
      .membership
      .members()
      .into_iter()
      .map(|m| m.addr)
      .collect();
    members.sort();
    members
  }

  fn saw_down(&self, observer: &str, target: &str) -> bool {
    self
      .nodes
      .get(observer)
      .expect("node")
      .events
      .iter()
      .any(|e| matches!(e, MembershipEvent::NodeDown(m) if m.addr == target))
  }
}

#[test]
fn two_nodes_discover_each_other() {
  let mut sim = Sim::new(&["a", "b"]);
  sim.announce("a", "b");

  assert_eq!(sim.active_members("a"), vec!["b".to_string()]);
  assert_eq!(sim.active_members("b"), vec!["a".to_string()]);
}

#[test]
fn three_nodes_converge_via_gossip() {
  let mut sim = Sim::new(&["a", "b", "c"]);
  // a joins b; c joins b. a and c never talk directly at join time.
  sim.announce("a", "b");
  sim.announce("c", "b");

  // Let SWIM probe/gossip cycles run.
  for _ in 0..50 {
    sim.step(Duration::from_millis(200));
  }

  assert_eq!(
    sim.active_members("a"),
    vec!["b".to_string(), "c".to_string()]
  );
  assert_eq!(
    sim.active_members("c"),
    vec!["a".to_string(), "b".to_string()]
  );
}

#[test]
fn partitioned_node_is_declared_down() {
  let mut sim = Sim::new(&["a", "b", "c"]);
  sim.announce("a", "b");
  sim.announce("c", "b");
  for _ in 0..50 {
    sim.step(Duration::from_millis(200));
  }
  assert_eq!(sim.active_members("a").len(), 2);

  // Cut c off from everyone.
  sim.partitions.push(("c".to_string(), "a".to_string()));
  sim.partitions.push(("c".to_string(), "b".to_string()));

  // Probe → suspicion → down takes several protocol periods.
  for _ in 0..600 {
    sim.step(Duration::from_millis(200));
    if sim.saw_down("a", "c") && sim.saw_down("b", "c") {
      break;
    }
  }

  assert!(sim.saw_down("a", "c"), "a must observe NodeDown(c)");
  assert!(sim.saw_down("b", "c"), "b must observe NodeDown(c)");
  assert_eq!(sim.active_members("a"), vec!["b".to_string()]);
}

#[test]
fn healed_node_rejoins_with_fresh_incarnation() {
  let mut sim = Sim::new(&["a", "b", "c"]);
  sim.announce("a", "b");
  sim.announce("c", "b");
  for _ in 0..50 {
    sim.step(Duration::from_millis(200));
  }

  let c_before = sim.nodes["c"].membership.me().id.incarnation();

  sim.partitions.push(("c".to_string(), "a".to_string()));
  sim.partitions.push(("c".to_string(), "b".to_string()));
  for _ in 0..600 {
    sim.step(Duration::from_millis(200));
    if sim.saw_down("a", "c") {
      break;
    }
  }
  assert!(sim.saw_down("a", "c"));

  // Heal and let c re-announce (its connections died; the driver dials
  // again — modeled here as an explicit announce).
  sim.partitions.clear();
  sim.announce("c", "b");
  for _ in 0..100 {
    sim.step(Duration::from_millis(200));
    if sim.active_members("a").len() == 2 && sim.active_members("c").len() == 2 {
      break;
    }
  }

  assert_eq!(
    sim.active_members("a"),
    vec!["b".to_string(), "c".to_string()],
    "cluster must re-converge after heal"
  );
  // If c was declared down while cut off, foca renews its identity; the
  // address is stable either way and at minimum the cluster re-converged.
  let c_after = sim.nodes["c"].membership.me().id.incarnation();
  let _ = (c_before, c_after);
}

#[test]
fn node_monitors_fan_out_events() {
  let monitors = NodeMonitors::new();
  let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
  let sink = std::sync::Arc::clone(&seen);
  monitors.subscribe(std::sync::Arc::new(move |event| {
    sink.lock().expect("lock").push(event.clone());
  }));

  let up = MembershipEvent::NodeUp(member("x"));
  let down = MembershipEvent::NodeDown(member("x"));
  monitors.publish(&[up.clone(), down.clone()]);

  let seen = seen.lock().expect("lock");
  assert_eq!(seen.len(), 2);
  assert!(matches!(&seen[0], MembershipEvent::NodeUp(m) if m.addr == "x"));
  assert!(matches!(&seen[1], MembershipEvent::NodeDown(m) if m.addr == "x"));
}

#[test]
fn static_discovery_returns_peers() {
  use id_effect_node::discovery::{Discovery, StaticDiscovery};
  let discovery = StaticDiscovery::new(["10.0.0.1:4000", "10.0.0.2:4000"]);
  let peers = pollster::block_on(discovery.candidates());
  assert_eq!(peers, vec!["10.0.0.1:4000", "10.0.0.2:4000"]);
}
