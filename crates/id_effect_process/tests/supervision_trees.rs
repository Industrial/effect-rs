//! Supervision-tree semantics: strategies, restart policies, intensity
//! escalation, shutdown protocol, and DynamicSupervisor.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use id_effect::runtime::Never;
use id_effect::{Effect, run_blocking, succeed};
use id_effect_process::{
  ChildSpec, DynamicSupervisor, DynamicSupervisorSpec, ExitReason, Next, NodeName, Pid, Process,
  ProcessCtx, ProcessNode, RestartPolicy, ShutdownPolicy, StartChildFn, Strategy, SupervisorHandle,
  SupervisorSpec,
};

fn node() -> ProcessNode {
  ProcessNode::threads(NodeName::new("suptest@local"))
}

fn wait_until(deadline_ms: u64, mut check: impl FnMut() -> bool) -> bool {
  let deadline = std::time::Instant::now() + Duration::from_millis(deadline_ms);
  while std::time::Instant::now() < deadline {
    if check() {
      return true;
    }
    std::thread::sleep(Duration::from_millis(2));
  }
  check()
}

const T: Duration = Duration::from_secs(5);

/// Worker that counts its own starts through a shared witness and crashes
/// on command.
struct Worker {
  witness: Arc<AtomicU32>,
}

#[allow(dead_code)] // crashes are induced via exit signals; calls kept for API completeness
enum WorkerCall {
  Ping,
  Crash,
  StopNormal,
}

impl Process for Worker {
  type Args = Arc<AtomicU32>;
  type Env = ();
  type Call = WorkerCall;
  type Reply = u32;
  type Cast = ();
  type Info = ();
  type Error = String;

  fn init(witness: Arc<AtomicU32>, _ctx: ProcessCtx) -> Effect<Self, String, ()> {
    Effect::new(move |_| {
      witness.fetch_add(1, Ordering::SeqCst);
      Ok(Worker { witness })
    })
  }

  fn handle_call(self, msg: WorkerCall, _ctx: ProcessCtx) -> Effect<(u32, Next<Self>), String, ()> {
    Effect::new(move |_| match msg {
      WorkerCall::Ping => {
        let starts = self.witness.load(Ordering::SeqCst);
        Ok((starts, Next::Continue(self)))
      }
      WorkerCall::Crash => Err("worker crash".to_string()),
      WorkerCall::StopNormal => Ok((0, Next::Stop(ExitReason::Normal, self))),
    })
  }
}

fn worker_spec(id: &str, witness: &Arc<AtomicU32>) -> ChildSpec {
  let witness = Arc::clone(witness);
  ChildSpec::worker::<Worker, _>(id, move || (Arc::clone(&witness), ()))
}

fn child_pid(handle: &SupervisorHandle, id: &str) -> Option<Pid> {
  run_blocking(handle.which_children(T), ())
    .ok()?
    .into_iter()
    .find(|(cid, _)| cid == id)?
    .1
}

fn crash_child(node: &ProcessNode, pid: &Pid) {
  // Reach into the child mailbox through a fresh typed addr via registry-
  // free path: send an exit signal that the (non-trapping) worker cannot
  // trap. Using a distinct reason keeps crash-vs-shutdown observable.
  node.exit(pid, ExitReason::Error("induced crash".to_string()));
}

#[test]
fn one_for_one_restarts_only_the_crashed_child() {
  let node = node();
  let w1 = Arc::new(AtomicU32::new(0));
  let w2 = Arc::new(AtomicU32::new(0));
  let handle = SupervisorSpec::new(Strategy::OneForOne)
    .intensity(10, Duration::from_secs(60))
    .child(worker_spec("a", &w1))
    .child(worker_spec("b", &w2))
    .start(&node);

  assert!(wait_until(2_000, || w1.load(Ordering::SeqCst) == 1
    && w2.load(Ordering::SeqCst) == 1));

  let a_pid = child_pid(&handle, "a").expect("child a running");
  crash_child(&node, &a_pid);

  assert!(
    wait_until(2_000, || w1.load(Ordering::SeqCst) == 2),
    "crashed child must restart"
  );
  std::thread::sleep(Duration::from_millis(50));
  assert_eq!(w2.load(Ordering::SeqCst), 1, "sibling must not restart");

  let a_pid2 = child_pid(&handle, "a").expect("child a running again");
  assert_ne!(a_pid, a_pid2, "restart produces a fresh pid");
}

#[test]
fn one_for_all_restarts_every_child() {
  let node = node();
  let w1 = Arc::new(AtomicU32::new(0));
  let w2 = Arc::new(AtomicU32::new(0));
  let handle = SupervisorSpec::new(Strategy::OneForAll)
    .intensity(10, Duration::from_secs(60))
    .child(worker_spec("a", &w1))
    .child(worker_spec("b", &w2))
    .start(&node);

  assert!(wait_until(2_000, || w1.load(Ordering::SeqCst) == 1
    && w2.load(Ordering::SeqCst) == 1));
  let a_pid = child_pid(&handle, "a").expect("child a running");
  crash_child(&node, &a_pid);

  assert!(wait_until(2_000, || w1.load(Ordering::SeqCst) == 2
    && w2.load(Ordering::SeqCst) == 2));
}

#[test]
fn rest_for_one_restarts_crashed_child_and_later_siblings_only() {
  let node = node();
  let w1 = Arc::new(AtomicU32::new(0));
  let w2 = Arc::new(AtomicU32::new(0));
  let w3 = Arc::new(AtomicU32::new(0));
  let handle = SupervisorSpec::new(Strategy::RestForOne)
    .intensity(10, Duration::from_secs(60))
    .child(worker_spec("a", &w1))
    .child(worker_spec("b", &w2))
    .child(worker_spec("c", &w3))
    .start(&node);

  assert!(wait_until(2_000, || w3.load(Ordering::SeqCst) == 1));
  let b_pid = child_pid(&handle, "b").expect("child b running");
  crash_child(&node, &b_pid);

  assert!(wait_until(2_000, || w2.load(Ordering::SeqCst) == 2
    && w3.load(Ordering::SeqCst) == 2));
  std::thread::sleep(Duration::from_millis(50));
  assert_eq!(
    w1.load(Ordering::SeqCst),
    1,
    "earlier sibling must not restart"
  );
}

#[test]
fn transient_child_is_not_restarted_after_graceful_stop() {
  let node = node();
  let w = Arc::new(AtomicU32::new(0));
  let handle = SupervisorSpec::new(Strategy::OneForOne)
    .child(worker_spec("a", &w).with_restart(RestartPolicy::Transient))
    .start(&node);

  assert!(wait_until(2_000, || w.load(Ordering::SeqCst) == 1));
  let a_pid = child_pid(&handle, "a").expect("running");
  node.exit(&a_pid, ExitReason::Shutdown);

  std::thread::sleep(Duration::from_millis(100));
  assert_eq!(
    w.load(Ordering::SeqCst),
    1,
    "transient + graceful: no restart"
  );
}

#[test]
fn intensity_exceeded_stops_supervisor_and_escalates() {
  let node = node();
  let w = Arc::new(AtomicU32::new(0));
  // 1 restart per minute allowed; the second crash breaches intensity.
  let handle = SupervisorSpec::new(Strategy::OneForOne)
    .intensity(1, Duration::from_secs(60))
    .child(worker_spec("a", &w))
    .start(&node);

  assert!(wait_until(2_000, || w.load(Ordering::SeqCst) == 1));

  for _ in 0..2 {
    if let Some(pid) = child_pid(&handle, "a") {
      crash_child(&node, &pid);
      std::thread::sleep(Duration::from_millis(50));
    }
  }

  assert!(
    wait_until(2_000, || node.table().lookup(handle.pid()).is_none()),
    "supervisor must stop once intensity is exceeded"
  );
}

#[test]
fn nested_supervisor_shutdown_stops_grandchildren() {
  let node = node();
  let w = Arc::new(AtomicU32::new(0));
  let inner_witness = Arc::clone(&w);

  let root = SupervisorSpec::new(Strategy::OneForOne)
    .child(ChildSpec::supervisor("inner", move || {
      SupervisorSpec::new(Strategy::OneForOne).child(worker_spec("leaf", &inner_witness))
    }))
    .start(&node);

  assert!(wait_until(2_000, || w.load(Ordering::SeqCst) == 1));
  let before = node.table().len();
  assert!(before >= 3, "root + inner + leaf alive, got {before}");

  pollster::block_on(root.stop(&node, ShutdownPolicy::Timeout(Duration::from_secs(2))));

  assert!(
    wait_until(2_000, || node.table().is_empty()),
    "whole tree must be gone, {} processes remain",
    node.table().len()
  );
}

#[test]
fn start_child_and_terminate_child_manage_dynamic_members() {
  let node = node();
  let w = Arc::new(AtomicU32::new(0));
  let handle = SupervisorSpec::new(Strategy::OneForOne).start(&node);

  assert_eq!(run_blocking(handle.count_children(T), ()), Ok(0));

  let started = run_blocking(handle.start_child(worker_spec("a", &w), T), ())
    .expect("call ok")
    .expect("start ok");
  assert!(node.table().lookup(&started).is_some());
  assert_eq!(run_blocking(handle.count_children(T), ()), Ok(1));

  assert_eq!(run_blocking(handle.terminate_child("a", T), ()), Ok(true));
  assert!(wait_until(2_000, || node
    .table()
    .lookup(&started)
    .is_none()));
  assert_eq!(run_blocking(handle.count_children(T), ()), Ok(0));
}

// ── DynamicSupervisor ──────────────────────────────────────────

struct Echo;

impl Process for Echo {
  type Args = ();
  type Env = ();
  type Call = ();
  type Reply = ();
  type Cast = ();
  type Info = ();
  type Error = Never;

  fn init(_: (), _ctx: ProcessCtx) -> Effect<Self, Never, ()> {
    succeed(Echo)
  }

  fn handle_call(self, _: (), _ctx: ProcessCtx) -> Effect<((), Next<Self>), Never, ()> {
    succeed(((), Next::Continue(self)))
  }
}

#[test]
fn dynamic_supervisor_starts_restarts_and_counts_children() {
  let node = node();
  let sup = DynamicSupervisor::start(
    &node,
    DynamicSupervisorSpec {
      restart: RestartPolicy::Permanent,
      ..DynamicSupervisorSpec::default()
    },
  );

  let start: StartChildFn =
    Arc::new(|node, sup_pid| Ok(node.spawn_linked_to::<Echo>(sup_pid, (), ()).pid().clone()));

  let pid = run_blocking(sup.start_dyn_child(Arc::clone(&start), T), ())
    .expect("call ok")
    .expect("start ok");
  assert_eq!(run_blocking(sup.count_dyn_children(T), ()), Ok(1));

  // Crash the child: Permanent policy must replace it.
  node.exit(&pid, ExitReason::Error("boom".to_string()));
  assert!(wait_until(2_000, || {
    node.table().lookup(&pid).is_none() && run_blocking(sup.count_dyn_children(T), ()) == Ok(1)
  }));
}
