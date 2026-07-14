//! [`DynamicSupervisor`] — on-demand, homogeneous children
//! (BEAM's `DynamicSupervisor` / Horde's dynamic children).

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use id_effect::runtime::Never;
use id_effect::{Effect, box_future};

use crate::addr::Addr;
use crate::exit::ExitReason;
use crate::msg::{CallError, Info, StartError};
use crate::node::ProcessNode;
use crate::pid::Pid;
use crate::process::{Next, Process, ProcessCtx};
use crate::supervisor::{RestartPolicy, ShutdownPolicy, terminate_pid};

/// Re-runnable start closure for a dynamic child: spawns the child linked
/// to the supervisor and returns its pid.
pub type StartChildFn = Arc<dyn Fn(&ProcessNode, &Pid) -> Result<Pid, StartError> + Send + Sync>;

/// Configuration for a [`DynamicSupervisor`].
#[derive(Clone)]
pub struct DynamicSupervisorSpec {
  /// Restart policy applied to every child.
  pub restart: RestartPolicy,
  /// Shutdown protocol applied to every child.
  pub shutdown: ShutdownPolicy,
  /// Maximum restarts allowed inside `within`.
  pub max_restarts: u32,
  /// Sliding intensity window measured on the node clock.
  pub within: Duration,
}

impl Default for DynamicSupervisorSpec {
  fn default() -> Self {
    Self {
      restart: RestartPolicy::Transient,
      shutdown: ShutdownPolicy::Timeout(Duration::from_secs(5)),
      max_restarts: 3,
      within: Duration::from_secs(5),
    }
  }
}

/// Requests understood by a running [`DynamicSupervisor`].
pub enum DynCall {
  /// Start a new child from a re-runnable closure.
  StartChild(StartChildFn),
  /// Stop a child by pid and remove it.
  TerminateChild(Pid),
  /// Number of live children.
  Count,
}

/// Replies from a running [`DynamicSupervisor`].
#[derive(Clone, Debug)]
pub enum DynReply {
  /// Outcome of `StartChild`.
  Started(Result<Pid, StartError>),
  /// Outcome of `TerminateChild` (`false` if unknown pid).
  Terminated(bool),
  /// Live child count.
  Count(usize),
}

/// Homogeneous dynamic supervisor behavior.
pub struct DynamicSupervisor {
  spec: DynamicSupervisorSpec,
  /// Live children with their re-runnable start closures (for restarts).
  children: HashMap<Pid, StartChildFn>,
  restarts: VecDeque<Instant>,
  expected_exits: HashSet<Pid>,
}

impl DynamicSupervisor {
  fn note_restart(&mut self, now: Instant) -> bool {
    self.restarts.push_back(now);
    let window_start = now.checked_sub(self.spec.within).unwrap_or(now);
    while let Some(front) = self.restarts.front() {
      if *front < window_start {
        self.restarts.pop_front();
      } else {
        break;
      }
    }
    self.restarts.len() <= self.spec.max_restarts as usize
  }

  /// Start a [`DynamicSupervisor`] on `node` (unlinked).
  pub fn start(node: &ProcessNode, spec: DynamicSupervisorSpec) -> Addr<DynamicSupervisor> {
    node.spawn::<DynamicSupervisor>(spec, ())
  }

  /// Start a [`DynamicSupervisor`] linked to the calling process.
  pub fn start_link(ctx: &ProcessCtx, spec: DynamicSupervisorSpec) -> Addr<DynamicSupervisor> {
    ctx.spawn_link::<DynamicSupervisor>(spec, ())
  }
}

impl Process for DynamicSupervisor {
  type Args = DynamicSupervisorSpec;
  type Env = ();
  type Call = DynCall;
  type Reply = DynReply;
  type Cast = ();
  type Info = ();
  type Error = Never;

  fn init(spec: DynamicSupervisorSpec, ctx: ProcessCtx) -> Effect<Self, Never, ()> {
    Effect::new(move |_| {
      ctx.trap_exit(true);
      Ok(Self {
        spec,
        children: HashMap::new(),
        restarts: VecDeque::new(),
        expected_exits: HashSet::new(),
      })
    })
  }

  fn handle_call(
    mut self,
    msg: DynCall,
    ctx: ProcessCtx,
  ) -> Effect<(DynReply, Next<Self>), Never, ()> {
    Effect::new_async(move |_| {
      box_future(async move {
        let reply = match msg {
          DynCall::StartChild(start) => match start(ctx.node(), ctx.pid()) {
            Ok(pid) => {
              self.children.insert(pid.clone(), start);
              DynReply::Started(Ok(pid))
            }
            Err(err) => DynReply::Started(Err(err)),
          },
          DynCall::TerminateChild(pid) => {
            if self.children.remove(&pid).is_some() {
              self.expected_exits.insert(pid.clone());
              terminate_pid(ctx.node(), &pid, self.spec.shutdown).await;
              DynReply::Terminated(true)
            } else {
              DynReply::Terminated(false)
            }
          }
          DynCall::Count => DynReply::Count(self.children.len()),
        };
        Ok((reply, Next::Continue(self)))
      })
    })
  }

  fn handle_info(mut self, msg: Info<()>, ctx: ProcessCtx) -> Effect<Next<Self>, Never, ()> {
    Effect::new(move |_| {
      let Info::Exit { from, reason } = msg else {
        return Ok(Next::Continue(self));
      };
      if self.expected_exits.remove(&from) {
        return Ok(Next::Continue(self));
      }
      let Some(start) = self.children.remove(&from) else {
        if reason.is_abnormal() {
          return Ok(Next::Stop(reason, self));
        }
        return Ok(Next::Continue(self));
      };

      let restart = match self.spec.restart {
        RestartPolicy::Permanent => true,
        RestartPolicy::Transient => !reason.is_graceful(),
        RestartPolicy::Temporary => false,
      };
      if !restart {
        return Ok(Next::Continue(self));
      }

      let clock = ctx.node().clock();
      if !self.note_restart(clock.now()) {
        return Ok(Next::Stop(ExitReason::Shutdown, self));
      }
      loop {
        match start(ctx.node(), ctx.pid()) {
          Ok(pid) => {
            self.children.insert(pid, start);
            break;
          }
          Err(_) => {
            if !self.note_restart(clock.now()) {
              return Ok(Next::Stop(ExitReason::Shutdown, self));
            }
          }
        }
      }
      Ok(Next::Continue(self))
    })
  }

  fn terminate(mut self, _reason: &ExitReason, ctx: ProcessCtx) -> Effect<(), Never, ()> {
    Effect::new_async(move |_| {
      box_future(async move {
        let pids: Vec<Pid> = self.children.keys().cloned().collect();
        for pid in pids {
          self.expected_exits.insert(pid.clone());
          terminate_pid(ctx.node(), &pid, self.spec.shutdown).await;
        }
        Ok(())
      })
    })
  }
}

/// Convenience: `call`-based facade over a running [`DynamicSupervisor`].
impl Addr<DynamicSupervisor> {
  /// Start a new dynamic child.
  pub fn start_dyn_child(
    &self,
    start: StartChildFn,
    timeout: Duration,
  ) -> Effect<Result<Pid, StartError>, CallError, ()> {
    self
      .call(DynCall::StartChild(start), timeout)
      .map(|reply| match reply {
        DynReply::Started(result) => result,
        _ => Err(StartError::Failed("unexpected reply".to_string())),
      })
  }

  /// Number of live dynamic children.
  pub fn count_dyn_children(&self, timeout: Duration) -> Effect<usize, CallError, ()> {
    self.call(DynCall::Count, timeout).map(|reply| match reply {
      DynReply::Count(n) => n,
      _ => 0,
    })
  }
}
