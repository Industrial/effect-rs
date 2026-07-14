//! Example 130 — local process supervision tree.

use std::time::Duration;

use id_effect::runtime::Never;
use id_effect::{Effect, succeed};
use id_effect_process::{
  ChildSpec, Next, NodeName, Process, ProcessCtx, ProcessNode, Strategy, SupervisorSpec,
};

struct Ping;

impl Process for Ping {
  type Args = ();
  type Env = ();
  type Call = ();
  type Reply = &'static str;
  type Cast = ();
  type Info = ();
  type Error = Never;

  fn init(_: (), _: ProcessCtx) -> Effect<Self, Never, ()> {
    succeed(Ping)
  }

  fn handle_call(self, _: (), _: ProcessCtx) -> Effect<(&'static str, Next<Self>), Never, ()> {
    succeed(("pong", Next::Continue(self)))
  }
}

fn main() {
  let node = ProcessNode::threads(NodeName::new("ex130@local"));
  let handle = SupervisorSpec::new(Strategy::OneForOne)
    .child(ChildSpec::worker::<Ping, _>("ping", || ((), ())))
    .start(&node);
  std::thread::sleep(Duration::from_millis(50));
  println!("supervisor pid {} children={:?}", handle.pid(), node.table().len());
}
