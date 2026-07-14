//! DistributedSupervisor takeover on NodeDown (ADR 0009 § distributed
//! supervision). Deterministic over MeshFabric — no sockets.

use std::sync::Arc;
use std::time::Duration;

use id_effect::runtime::Never;
use id_effect::{Effect, succeed};
use id_effect_node::id::NodeId;
use id_effect_node::membership::{Member, MembershipEvent};
use id_effect_node::{BehaviorRegistry, DistChildSpec, DistributedSupervisor, Mesh, MeshFabric};
use id_effect_process::{Next, NodeName, Process, ProcessCtx, ProcessNode};

struct Worker;

impl Process for Worker {
  type Args = ();
  type Env = ();
  type Call = ();
  type Reply = ();
  type Cast = ();
  type Info = ();
  type Error = Never;

  fn init(_: (), _: ProcessCtx) -> Effect<Self, Never, ()> {
    succeed(Worker)
  }

  fn handle_call(self, _: (), _: ProcessCtx) -> Effect<((), Next<Self>), Never, ()> {
    succeed(((), Next::Continue(self)))
  }
}

fn mesh_pair() -> (Arc<Mesh>, Arc<Mesh>) {
  let fabric = MeshFabric::new();
  let mut ra = BehaviorRegistry::new();
  ra.register::<Worker>("worker");
  let mut rb = BehaviorRegistry::new();
  rb.register::<Worker>("worker");
  let a = Mesh::new(
    NodeId::fresh(&NodeName::new("a@test")),
    ProcessNode::threads(NodeName::new("a@test")),
    ra,
  )
  .with_fabric(fabric.clone());
  let b = Mesh::new(
    NodeId::fresh(&NodeName::new("b@test")),
    ProcessNode::threads(NodeName::new("b@test")),
    rb,
  )
  .with_fabric(fabric);
  (a, b)
}

fn mesh_trio() -> (Arc<Mesh>, Arc<Mesh>, Arc<Mesh>) {
  let fabric = MeshFabric::new();
  let make = |name: &str| {
    let mut r = BehaviorRegistry::new();
    r.register::<Worker>("worker");
    Mesh::new(
      NodeId::fresh(&NodeName::new(name)),
      ProcessNode::threads(NodeName::new(name)),
      r,
    )
    .with_fabric(fabric.clone())
  };
  (make("a@test"), make("b@test"), make("c@test"))
}

fn spec() -> DistChildSpec {
  DistChildSpec {
    id: "w1".into(),
    behavior: "worker".into(),
    args: postcard::to_allocvec(&()).unwrap(),
    prefer: Some("b@test".into()),
  }
}

fn spec_anywhere() -> DistChildSpec {
  DistChildSpec {
    id: "w1".into(),
    behavior: "worker".into(),
    args: postcard::to_allocvec(&()).unwrap(),
    prefer: None,
  }
}

#[test]
fn child_starts_on_preferred_node() {
  let (a, _b) = mesh_pair();
  let sup = DistributedSupervisor::new(Arc::clone(&a));
  let pid = sup.start_child(spec(), None).expect("start");
  assert_eq!(pid.node().as_str(), "b@test");
  assert_eq!(sup.len(), 1);
  let listed = sup.list();
  assert_eq!(listed[0].2, "b@test");
}

#[test]
fn takeover_replaces_child_on_node_down() {
  let (a, _b) = mesh_pair();
  let sup = DistributedSupervisor::new(Arc::clone(&a));
  let original = sup.start_child(spec(), None).expect("start");
  assert_eq!(original.node().as_str(), "b@test");

  // b departs the cluster.
  let down = Member::new(NodeId::fresh(&NodeName::new("b@test")), "b@test");
  sup.on_membership(&MembershipEvent::NodeDown(down));

  std::thread::sleep(Duration::from_millis(50));

  // Exactly one child, now hosted locally (Horde-style takeover on self).
  assert_eq!(sup.len(), 1, "child re-placed, not duplicated");
  let listed = sup.list();
  assert_eq!(listed[0].0, "w1", "same logical child id");
  assert_eq!(listed[0].2, "a@test", "took over on surviving node");
  assert_eq!(listed[0].1.node().as_str(), "a@test");
}

#[test]
fn placement_picks_least_loaded_node() {
  let (a, _b, _c) = mesh_trio();
  // Skewed telemetry: a busy, c mid, b idle (most policy headroom).
  a.report_load("a@test", 0.9, 0.9);
  a.report_load("b@test", 0.1, 0.1);
  a.report_load("c@test", 0.5, 0.5);

  let sup = DistributedSupervisor::new(Arc::clone(&a));
  // No `prefer`: the unified placement API decides purely on load.
  let pid = sup.start_child(spec_anywhere(), None).expect("start");

  assert_eq!(
    pid.node().as_str(),
    "b@test",
    "least-loaded eligible node wins under LocalFirst"
  );
  assert_eq!(sup.len(), 1);
  assert_eq!(sup.list()[0].2, "b@test");
}

#[test]
fn node_down_for_unrelated_node_is_noop() {
  let (a, _b) = mesh_pair();
  let sup = DistributedSupervisor::new(Arc::clone(&a));
  sup.start_child(spec(), None).expect("start");
  let down = Member::new(NodeId::fresh(&NodeName::new("z@test")), "z@test");
  sup.on_membership(&MembershipEvent::NodeDown(down));
  let listed = sup.list();
  assert_eq!(listed.len(), 1);
  assert_eq!(
    listed[0].2, "b@test",
    "unrelated NodeDown must not move child"
  );
}
