//! Remote process semantics over MeshFabric (deterministic, no sockets).

use std::sync::Arc;
use std::time::Duration;

use id_effect::runtime::Never;
use id_effect::{Effect, succeed};
use id_effect_node::{
  BehaviorRegistry, Mesh, MeshFabric, NodeId, RemoteFault,
};
use id_effect_process::{
  ExitReason, Next, NodeName, Process, ProcessCtx, ProcessNode,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
enum CounterCall {
  Get,
  Inc,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
enum CounterCast {
  Add(u32),
}

struct Counter {
  n: u32,
}

impl Process for Counter {
  type Args = ();
  type Env = ();
  type Call = CounterCall;
  type Reply = u32;
  type Cast = CounterCast;
  type Info = ();
  type Error = Never;

  fn init(_args: (), _ctx: ProcessCtx) -> Effect<Self, Never, ()> {
    succeed(Counter { n: 0 })
  }

  fn handle_call(
    mut self,
    msg: CounterCall,
    _ctx: ProcessCtx,
  ) -> Effect<(u32, Next<Self>), Never, ()> {
    Effect::new(move |_| match msg {
      CounterCall::Get => Ok((self.n, Next::Continue(self))),
      CounterCall::Inc => {
        self.n += 1;
        Ok((self.n, Next::Continue(self)))
      }
    })
  }

  fn handle_cast(mut self, msg: CounterCast, _ctx: ProcessCtx) -> Effect<Next<Self>, Never, ()> {
    Effect::new(move |_| match msg {
      CounterCast::Add(x) => {
        self.n += x;
        Ok(Next::Continue(self))
      }
    })
  }
}

fn mesh_pair() -> (Arc<Mesh>, Arc<Mesh>, MeshFabric) {
  let fabric = MeshFabric::new();
  let mut reg_a = BehaviorRegistry::new();
  reg_a.register::<Counter>("counter");
  let mut reg_b = BehaviorRegistry::new();
  reg_b.register::<Counter>("counter");

  let name_a = NodeName::new("a@test");
  let name_b = NodeName::new("b@test");
  let a = Mesh::new(
    NodeId::fresh(&name_a),
    ProcessNode::threads(name_a),
    reg_a,
  )
  .with_fabric(fabric.clone());
  let b = Mesh::new(
    NodeId::fresh(&name_b),
    ProcessNode::threads(name_b),
    reg_b,
  )
  .with_fabric(fabric.clone());
  (a, b, fabric)
}

#[test]
fn remote_spawn_cast_call_round_trip() {
  let (a, b, _fabric) = mesh_pair();
  let args = postcard::to_allocvec(&()).expect("args");

  let pid = a
    .remote_spawn(
      "b@test",
      "counter",
      args,
      None,
      Duration::from_secs(2),
    )
    .expect("spawn");
  assert_eq!(pid.node().as_str(), "b@test");

  // Allow init to finish.
  std::thread::sleep(Duration::from_millis(50));

  let cast = postcard::to_allocvec(&CounterCast::Add(5)).unwrap();
  a.remote_cast(&pid, cast).expect("cast");
  std::thread::sleep(Duration::from_millis(20));

  let call = postcard::to_allocvec(&CounterCall::Get).unwrap();
  let reply = a
    .remote_call(&pid, call, Duration::from_secs(2))
    .expect("call");
  let n: u32 = postcard::from_bytes(&reply).unwrap();
  assert_eq!(n, 5);

  let _ = b;
}

#[test]
fn remote_monitor_fires_noconnection_on_peer_down() {
  let (a, b, _fabric) = mesh_pair();
  let args = postcard::to_allocvec(&()).unwrap();
  let pid = a
    .remote_spawn(
      "b@test",
      "counter",
      args,
      None,
      Duration::from_secs(2),
    )
    .expect("spawn");

  let seen = Arc::new(std::sync::Mutex::new(None));
  let sink_seen = Arc::clone(&seen);
  let _mon = a.remote_monitor(
    &pid,
    Box::new(move |_m, _p, reason| {
      *sink_seen.lock().unwrap() = Some(reason);
    }),
  );

  a.peer_down("b@test");
  assert!(wait_until(2_000, || {
    matches!(
      seen.lock().unwrap().as_ref(),
      Some(ExitReason::NoConnection)
    )
  }));
  let _ = b;
}

#[test]
fn unknown_behavior_is_rejected() {
  let (a, _b, _) = mesh_pair();
  let err = a
    .remote_spawn(
      "b@test",
      "nope",
      vec![],
      None,
      Duration::from_secs(1),
    )
    .unwrap_err();
  assert!(matches!(err, RemoteFault::UnknownBehavior(_)));
}

fn wait_until(deadline_ms: u64, mut check: impl FnMut() -> bool) -> bool {
  let deadline = std::time::Instant::now() + Duration::from_millis(deadline_ms);
  while std::time::Instant::now() < deadline {
    if check() {
      return true;
    }
    std::thread::sleep(Duration::from_millis(5));
  }
  check()
}

#[allow(dead_code)]
fn _silence_succeed() {
  let _: Effect<(), Never, ()> = succeed(());
}
