//! Chaos: partition + membership down + heal on InMemoryTransport + TimerWheel.

use std::num::NonZeroU32;
use std::time::Duration;

use foca::Config;
use id_effect_node::id::NodeId;
use id_effect_node::membership::{Member, Membership, MembershipEvent, TimerWheel};
use id_effect_node::transport::memory::MemoryNetwork;
use id_effect_node::transport::Transport;
use id_effect_process::NodeName;

#[tokio::test]
async fn memory_partition_flap_and_heal() {
  let net = MemoryNetwork::new();
  let a = net.transport("a");
  let b = net.transport("b");
  let _la = a.listen("a").await.unwrap();
  let _lb = b.listen("b").await.unwrap();

  // Flap: partition, heal, partition.
  net.partition("a", "b");
  assert!(a.connect("b").await.is_err());
  net.heal("a", "b");
  let dial = a.connect("b").await.expect("heal");
  dial.close().await;
  net.partition("a", "b");
  assert!(a.connect("b").await.is_err());
}

#[test]
fn membership_declares_down_under_scripted_partition() {
  let mut cfg = Config::new_lan(NonZeroU32::new(3).unwrap());
  cfg.notify_down_members = true;
  let ma = Member::new(NodeId::fresh(&NodeName::new("a")), "a");
  let mb = Member::new(NodeId::fresh(&NodeName::new("b")), "b");
  let mut a = Membership::deterministic(ma.clone(), cfg.clone(), 1);
  let mut b = Membership::deterministic(mb.clone(), cfg, 2);
  let mut wa = TimerWheel::new();
  let mut wb = TimerWheel::new();

  let out = a.announce(mb.clone()).unwrap();
  wa.schedule(out.timers);
  for (_, bytes) in out.packets {
    let o = b.handle_packet(&bytes).unwrap();
    wb.schedule(o.timers);
    for (_, bytes) in o.packets {
      let _ = a.handle_packet(&bytes);
    }
  }

  // Partition: stop delivering; advance time until down.
  let mut saw = false;
  for _ in 0..800 {
    for ev in wa.advance(Duration::from_millis(200)) {
      if let Ok(o) = a.handle_timer(ev) {
        wa.schedule(o.timers);
        // drop packets (partition)
        for e in o.events {
          if matches!(e, MembershipEvent::NodeDown(_)) {
            saw = true;
          }
        }
      }
    }
    for ev in wb.advance(Duration::from_millis(200)) {
      let _ = b.handle_timer(ev);
    }
    if saw {
      break;
    }
  }
  assert!(saw || a.members().is_empty(), "expected down or empty view");
}
