//! Chaos suite (ADR 0009 § verification): asymmetric partition, flapping,
//! and memory-transport partition scenarios on a fully deterministic
//! in-memory SWIM sim (each node owns a virtual [`TimerWheel`]; the test
//! owns the network). No sleeps, no real clock — every scenario is a pure
//! function of the script.

use std::collections::HashMap;
use std::num::NonZeroU32;
use std::time::Duration;

use bytes::Bytes;
use foca::Config;
use id_effect_node::id::NodeId;
use id_effect_node::membership::{Member, Membership, MembershipEvent, Output, TimerWheel};
use id_effect_node::transport::Transport;
use id_effect_node::transport::memory::MemoryNetwork;
use id_effect_process::NodeName;

struct SimNode {
  membership: Membership,
  timers: TimerWheel,
  events: Vec<MembershipEvent>,
}

/// Deterministic cluster with directional cuts (asymmetric partitions).
struct Sim {
  nodes: HashMap<String, SimNode>,
  /// Directional drops: `(from, to)` means `from`→`to` packets vanish.
  cuts: Vec<(String, String)>,
  wire: Vec<(String, Bytes, String)>,
}

fn test_config() -> Config {
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
      cuts: Vec::new(),
      wire: Vec::new(),
    }
  }

  fn cut_dir(&mut self, from: &str, to: &str) {
    self.cuts.push((from.to_string(), to.to_string()));
  }

  fn absorb(&mut self, from: &str, output: Output) {
    let node = self.nodes.get_mut(from).expect("node exists");
    node.timers.schedule(output.timers);
    node.events.extend(output.events);
    for (to, bytes) in output.packets {
      self.wire.push((to.addr.clone(), bytes, from.to_string()));
    }
  }

  fn drain_wire(&mut self) {
    loop {
      let cuts = self.cuts.clone();
      self
        .wire
        .retain(|(to, _, from)| !cuts.iter().any(|(x, y)| x == from && y == to));
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

  fn active_len(&self, name: &str) -> usize {
    self
      .nodes
      .get(name)
      .expect("node")
      .membership
      .members()
      .len()
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

  fn converge(&mut self, steps: usize) {
    for _ in 0..steps {
      self.step(Duration::from_millis(200));
    }
  }
}

#[test]
fn asymmetric_partition_still_declares_down() {
  // Only c's *outbound* packets vanish: a/b probes reach c, but c's
  // replies never arrive, so the majority side must declare c down.
  let mut sim = Sim::new(&["a", "b", "c"]);
  sim.announce("a", "b");
  sim.announce("c", "b");
  sim.converge(60);
  assert_eq!(sim.active_len("a"), 2);

  sim.cut_dir("c", "a");
  sim.cut_dir("c", "b");

  for _ in 0..800 {
    sim.step(Duration::from_millis(200));
    if sim.saw_down("a", "c") && sim.saw_down("b", "c") {
      break;
    }
  }
  assert!(sim.saw_down("a", "c"), "a must declare c down (asymmetric)");
  assert!(sim.saw_down("b", "c"), "b must declare c down (asymmetric)");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn memory_transport_partition_and_heal() {
  let net = MemoryNetwork::new();
  let a = net.transport("a");
  let b = net.transport("b");
  let _la = a.listen("a").await.expect("listen a");
  let _lb = b.listen("b").await.expect("listen b");

  net.partition("a", "b");
  assert!(a.connect("b").await.is_err(), "partitioned dial fails");

  net.heal("a", "b");
  let dial = a.connect("b").await.expect("healed dial");
  dial.close().await;

  net.partition("a", "b");
  assert!(a.connect("b").await.is_err(), "re-partitioned dial fails");
}
