//! AP process-group CRDT + fanout dispatch (ADR 0009 § cluster registry).
//! Deterministic over MeshFabric — no sockets.

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use id_effect::runtime::Never;
use id_effect::{Effect, succeed};
use id_effect_node::id::NodeId;
use id_effect_node::{BehaviorRegistry, Mesh, MeshFabric, OrSwot, ProcessGroups, fanout_cast};
use id_effect_process::{Next, NodeName, Pid, Process, ProcessCtx, ProcessNode};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
enum Cast {
  Tick,
}

struct Sink;

static HITS: AtomicU32 = AtomicU32::new(0);

impl Process for Sink {
  type Args = ();
  type Env = ();
  type Call = ();
  type Reply = ();
  type Cast = Cast;
  type Info = ();
  type Error = Never;

  fn init(_: (), _: ProcessCtx) -> Effect<Self, Never, ()> {
    succeed(Sink)
  }

  fn handle_call(self, _: (), _: ProcessCtx) -> Effect<((), Next<Self>), Never, ()> {
    succeed(((), Next::Continue(self)))
  }

  fn handle_cast(self, msg: Cast, _: ProcessCtx) -> Effect<Next<Self>, Never, ()> {
    Effect::new(move |_| match msg {
      Cast::Tick => {
        HITS.fetch_add(1, Ordering::SeqCst);
        Ok(Next::Continue(self))
      }
    })
  }
}

fn pid(n: &str, id: u64) -> Pid {
  Pid::from_parts(NodeName::new(n), id, 1)
}

#[test]
fn groups_join_leave_members() {
  let mut g = ProcessGroups::new("a@test");
  let p1 = pid("a@test", 1);
  let p2 = pid("a@test", 2);
  g.join("workers", &p1);
  g.join("workers", &p2);
  assert_eq!(g.members("workers").len(), 2);
  g.leave("workers", &p1);
  assert_eq!(g.members("workers").len(), 1);
  assert!(g.group_names().contains("workers"));
}

#[test]
fn crdt_merge_converges_across_replicas() {
  let mut a = ProcessGroups::new("a");
  let mut b = ProcessGroups::new("b");
  a.join("g", &pid("a@test", 1));
  b.join("g", &pid("b@test", 2));

  let snap_b: OrSwot = b.snapshot("g");
  let snap_a: OrSwot = a.snapshot("g");
  a.merge_group("g", &snap_b);
  b.merge_group("g", &snap_a);

  assert_eq!(a.members("g").len(), 2);
  assert_eq!(b.members("g").len(), 2);
}

#[test]
fn fanout_cast_reaches_all_group_members() {
  HITS.store(0, Ordering::SeqCst);
  let fabric = MeshFabric::new();
  let mut ra = BehaviorRegistry::new();
  ra.register::<Sink>("sink");
  let mut rb = BehaviorRegistry::new();
  rb.register::<Sink>("sink");
  let a = Mesh::new(
    NodeId::fresh(&NodeName::new("a@test")),
    ProcessNode::threads(NodeName::new("a@test")),
    ra,
  )
  .with_fabric(fabric.clone());
  let _b = Mesh::new(
    NodeId::fresh(&NodeName::new("b@test")),
    ProcessNode::threads(NodeName::new("b@test")),
    rb,
  )
  .with_fabric(fabric);

  let args = postcard::to_allocvec(&()).unwrap();
  let p1 = a
    .remote_spawn("b@test", "sink", args.clone(), None, Duration::from_secs(2))
    .expect("spawn 1");
  let p2 = a
    .remote_spawn("b@test", "sink", args, None, Duration::from_secs(2))
    .expect("spawn 2");
  std::thread::sleep(Duration::from_millis(50));

  let mut groups = ProcessGroups::new("a@test");
  groups.join("sinks", &p1);
  groups.join("sinks", &p2);

  let msg = postcard::to_allocvec(&Cast::Tick).unwrap();
  let delivered = fanout_cast(&a, &groups, "sinks", &msg).expect("fanout");
  assert_eq!(delivered, 2, "cast reached both members");

  let deadline = std::time::Instant::now() + Duration::from_secs(2);
  while HITS.load(Ordering::SeqCst) < 2 && std::time::Instant::now() < deadline {
    std::thread::sleep(Duration::from_millis(5));
  }
  assert_eq!(HITS.load(Ordering::SeqCst), 2, "both sinks ticked");
}
