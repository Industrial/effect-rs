//! Example 131 — two in-process meshes with remote cast/call.

use std::sync::Arc;
use std::time::Duration;

use id_effect::runtime::Never;
use id_effect::{Effect, succeed};
use id_effect_node::{BehaviorRegistry, Mesh, MeshFabric, NodeId};
use id_effect_process::{Next, NodeName, Process, ProcessCtx, ProcessNode};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
enum Call {
  Get,
}

#[derive(Serialize, Deserialize)]
enum Cast {
  Set(u32),
}

struct BoxNum {
  n: u32,
}

impl Process for BoxNum {
  type Args = ();
  type Env = ();
  type Call = Call;
  type Reply = u32;
  type Cast = Cast;
  type Info = ();
  type Error = Never;

  fn init(_: (), _: ProcessCtx) -> Effect<Self, Never, ()> {
    succeed(BoxNum { n: 0 })
  }

  fn handle_call(self, msg: Call, _: ProcessCtx) -> Effect<(u32, Next<Self>), Never, ()> {
    Effect::new(move |_| match msg {
      Call::Get => Ok((self.n, Next::Continue(self))),
    })
  }

  fn handle_cast(mut self, msg: Cast, _: ProcessCtx) -> Effect<Next<Self>, Never, ()> {
    Effect::new(move |_| match msg {
      Cast::Set(n) => {
        self.n = n;
        Ok(Next::Continue(self))
      }
    })
  }
}

fn main() {
  let fabric = MeshFabric::new();
  let mut ra = BehaviorRegistry::new();
  ra.register::<BoxNum>("box");
  let mut rb = BehaviorRegistry::new();
  rb.register::<BoxNum>("box");
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

  let pid = a
    .remote_spawn(
      "b@ex",
      "box",
      postcard::to_allocvec(&()).unwrap(),
      None,
      Duration::from_secs(2),
    )
    .expect("spawn");
  std::thread::sleep(Duration::from_millis(50));
  a.remote_cast(&pid, postcard::to_allocvec(&Cast::Set(42)).unwrap())
    .unwrap();
  std::thread::sleep(Duration::from_millis(20));
  let bytes = a
    .remote_call(
      &pid,
      postcard::to_allocvec(&Call::Get).unwrap(),
      Duration::from_secs(2),
    )
    .unwrap();
  let n: u32 = postcard::from_bytes(&bytes).unwrap();
  println!("remote box = {n}");
  let _ = Arc::clone(&a);
}
