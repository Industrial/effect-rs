//! Example 133 — fan a workload out to a process group across nodes.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use id_effect::runtime::Never;
use id_effect::{Effect, succeed};
use id_effect_node::id::NodeId;
use id_effect_node::{BehaviorRegistry, Mesh, MeshFabric, ProcessGroups, fanout_cast};
use id_effect_process::{Next, NodeName, Process, ProcessCtx, ProcessNode};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
enum Cast {
  Work(u32),
}

static DONE: AtomicU32 = AtomicU32::new(0);

struct Worker;

impl Process for Worker {
  type Args = ();
  type Env = ();
  type Call = ();
  type Reply = ();
  type Cast = Cast;
  type Info = ();
  type Error = Never;

  fn init(_: (), _: ProcessCtx) -> Effect<Self, Never, ()> {
    succeed(Worker)
  }

  fn handle_call(self, _: (), _: ProcessCtx) -> Effect<((), Next<Self>), Never, ()> {
    succeed(((), Next::Continue(self)))
  }

  fn handle_cast(self, msg: Cast, _: ProcessCtx) -> Effect<Next<Self>, Never, ()> {
    Effect::new(move |_| match msg {
      Cast::Work(n) => {
        DONE.fetch_add(n, Ordering::SeqCst);
        Ok(Next::Continue(self))
      }
    })
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

  let args = postcard::to_allocvec(&()).unwrap();
  let mut groups = ProcessGroups::new("a@ex");
  for _ in 0..3 {
    let pid = a
      .remote_spawn("b@ex", "worker", args.clone(), None, Duration::from_secs(2))
      .expect("spawn");
    groups.join("workers", &pid);
  }
  std::thread::sleep(Duration::from_millis(50));

  let msg = postcard::to_allocvec(&Cast::Work(10)).unwrap();
  let n = fanout_cast(&a, &groups, "workers", &msg).expect("fanout");
  std::thread::sleep(Duration::from_millis(50));
  println!(
    "fanned out to {n} workers; total work = {}",
    DONE.load(Ordering::SeqCst)
  );
  let _ = Arc::clone(&a);
}
