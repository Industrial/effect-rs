//! Example 132 — DistributedSupervisor takeover when a node goes down.

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

fn main() {
  let fabric = MeshFabric::new();
  let mut ra = BehaviorRegistry::new();
  ra.register::<Worker>("worker");
  let mut rb = BehaviorRegistry::new();
  rb.register::<Worker>("worker");
  let a = Mesh::new(
    NodeId::fresh(&NodeName::new("a@ex")),
    ProcessNode::threads(NodeName::new("a@ex")),
    ra,
  )
  .with_fabric(fabric.clone());
  let _b = Mesh::new(
    NodeId::fresh(&NodeName::new("b@ex")),
    ProcessNode::threads(NodeName::new("b@ex")),
    rb,
  )
  .with_fabric(fabric);

  let sup = DistributedSupervisor::new(Arc::clone(&a));
  let pid = sup
    .start_child(
      DistChildSpec {
        id: "w1".into(),
        behavior: "worker".into(),
        args: postcard::to_allocvec(&()).unwrap(),
        prefer: Some("b@ex".into()),
      },
      None,
    )
    .expect("start");
  println!("child started on {}", pid.node().as_str());

  let down = Member::new(NodeId::fresh(&NodeName::new("b@ex")), "b@ex");
  sup.on_membership(&MembershipEvent::NodeDown(down));
  std::thread::sleep(Duration::from_millis(50));

  let listed = sup.list();
  println!("after failover: child {} on {}", listed[0].0, listed[0].2);
}
