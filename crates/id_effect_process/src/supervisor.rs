//! OTP-style supervision trees: [`ChildSpec`], [`Strategy`], restart
//! intensity on the injected `Clock`, and the shutdown protocol.
//!
//! The supervisor **is a process** (it implements [`Process`]): it traps
//! exits, links every child, and reacts to `Info::Exit` messages with the
//! configured strategy. Escalation is plain link propagation — when the
//! restart intensity is exceeded the supervisor stops with
//! [`ExitReason::Shutdown`], which its own supervisor observes like any
//! child exit.
//!
//! ## Shutdown protocol (ADR 0009)
//!
//! - [`ShutdownPolicy::BrutalKill`]: untrappable kill, wait for the exit.
//! - [`ShutdownPolicy::Timeout`]: trappable `Shutdown` signal, wait up to
//!   the deadline on the injected clock, then untrappable kill.
//! - [`ShutdownPolicy::Infinity`]: trappable `Shutdown`, wait forever
//!   (reserved for child supervisors).

use std::collections::{HashSet, VecDeque};
use std::future::poll_fn;
use std::pin::pin;
use std::sync::Arc;
use std::task::Poll;
use std::time::{Duration, Instant};

use id_effect::runtime::Never;
use id_effect::{Deferred, Effect, box_future};

use crate::addr::Addr;
use crate::exit::ExitReason;
use crate::msg::{CallError, Info, StartError};
use crate::node::ProcessNode;
use crate::pid::Pid;
use crate::process::{Next, Process, ProcessCtx};

/// Restart strategy applied when a child exits abnormally.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Strategy {
  /// Restart only the crashed child.
  OneForOne,
  /// Terminate all other children, then restart every non-temporary child.
  OneForAll,
  /// Terminate children started **after** the crashed one, then restart
  /// the crashed child and those siblings.
  RestForOne,
}

/// When a child should be restarted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestartPolicy {
  /// Always restart.
  Permanent,
  /// Restart only after an abnormal exit (not `Normal` / `Shutdown`).
  Transient,
  /// Never restart.
  Temporary,
}

/// How a child is asked to stop during supervisor shutdown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShutdownPolicy {
  /// Untrappable kill immediately.
  BrutalKill,
  /// Trappable `Shutdown`, escalating to a kill after the deadline.
  Timeout(Duration),
  /// Trappable `Shutdown`, wait forever (use for child supervisors).
  Infinity,
}

/// Worker or nested supervisor (affects conventional shutdown defaults).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChildKind {
  /// Ordinary process.
  Worker,
  /// Nested supervisor.
  Supervisor,
}

/// Start closure: spawns the child **linked to the supervisor** and
/// returns its pid. Must be re-runnable (restarts call it again).
pub type StartFn = Arc<dyn Fn(&ProcessNode, &Pid) -> Result<Pid, StartError> + Send + Sync>;

/// Declarative child description.
#[derive(Clone)]
pub struct ChildSpec {
  /// Unique id within the supervisor.
  pub id: String,
  /// Re-runnable start closure.
  pub start: StartFn,
  /// Restart policy.
  pub restart: RestartPolicy,
  /// Shutdown protocol.
  pub shutdown: ShutdownPolicy,
  /// Worker or nested supervisor.
  pub kind: ChildKind,
}

impl ChildSpec {
  /// Worker child from a behavior type and a re-runnable `(args, env)`
  /// factory. Defaults: `Permanent`, 5 s trappable shutdown.
  pub fn worker<P, F>(id: impl Into<String>, factory: F) -> Self
  where
    P: Process,
    F: Fn() -> (P::Args, P::Env) + Send + Sync + 'static,
  {
    let start: StartFn = Arc::new(move |node, supervisor| {
      let (args, env) = factory();
      Ok(
        node
          .spawn_linked_to::<P>(supervisor, args, env)
          .pid()
          .clone(),
      )
    });
    Self {
      id: id.into(),
      start,
      restart: RestartPolicy::Permanent,
      shutdown: ShutdownPolicy::Timeout(Duration::from_secs(5)),
      kind: ChildKind::Worker,
    }
  }

  /// Child from a raw start closure.
  pub fn from_start(id: impl Into<String>, start: StartFn) -> Self {
    Self {
      id: id.into(),
      start,
      restart: RestartPolicy::Permanent,
      shutdown: ShutdownPolicy::Timeout(Duration::from_secs(5)),
      kind: ChildKind::Worker,
    }
  }

  /// Nested-supervisor child from a [`SupervisorSpec`] factory.
  pub fn supervisor<F>(id: impl Into<String>, factory: F) -> Self
  where
    F: Fn() -> SupervisorSpec + Send + Sync + 'static,
  {
    let start: StartFn = Arc::new(move |node, supervisor| {
      Ok(
        node
          .spawn_linked_to::<SupervisorProcess>(supervisor, factory(), ())
          .pid()
          .clone(),
      )
    });
    Self {
      id: id.into(),
      start,
      restart: RestartPolicy::Permanent,
      shutdown: ShutdownPolicy::Infinity,
      kind: ChildKind::Supervisor,
    }
  }

  /// Override the restart policy.
  pub fn with_restart(mut self, restart: RestartPolicy) -> Self {
    self.restart = restart;
    self
  }

  /// Override the shutdown protocol.
  pub fn with_shutdown(mut self, shutdown: ShutdownPolicy) -> Self {
    self.shutdown = shutdown;
    self
  }
}

/// Declarative supervisor description.
#[derive(Clone)]
pub struct SupervisorSpec {
  /// Restart strategy.
  pub strategy: Strategy,
  /// Maximum restarts allowed inside [`SupervisorSpec::within`].
  pub max_restarts: u32,
  /// Sliding intensity window measured on the node clock.
  pub within: Duration,
  /// Children in start order.
  pub children: Vec<ChildSpec>,
}

impl SupervisorSpec {
  /// Spec with BEAM's defaults (3 restarts / 5 seconds).
  pub fn new(strategy: Strategy) -> Self {
    Self {
      strategy,
      max_restarts: 3,
      within: Duration::from_secs(5),
      children: Vec::new(),
    }
  }

  /// Set the restart intensity.
  pub fn intensity(mut self, max_restarts: u32, within: Duration) -> Self {
    self.max_restarts = max_restarts;
    self.within = within;
    self
  }

  /// Append a child (start order = append order).
  pub fn child(mut self, spec: ChildSpec) -> Self {
    self.children.push(spec);
    self
  }

  /// Start a root supervisor on `node` (unlinked).
  pub fn start(self, node: &ProcessNode) -> SupervisorHandle {
    SupervisorHandle {
      addr: node.spawn::<SupervisorProcess>(self, ()),
    }
  }

  /// Start a supervisor linked to the calling process.
  pub fn start_link(self, ctx: &ProcessCtx) -> SupervisorHandle {
    SupervisorHandle {
      addr: ctx.spawn_link::<SupervisorProcess>(self, ()),
    }
  }
}

/// Requests understood by a running supervisor.
pub enum SupCall {
  /// `(child id, current pid)` for every child.
  WhichChildren,
  /// Number of children (running or not).
  CountChildren,
  /// Add and start a new child.
  StartChild(ChildSpec),
  /// Stop a child by id and remove it from the tree.
  TerminateChild(String),
}

/// Replies from a running supervisor.
#[derive(Clone, Debug)]
pub enum SupReply {
  /// Child ids with their live pids.
  Children(Vec<(String, Option<Pid>)>),
  /// Child count.
  Count(usize),
  /// Outcome of `StartChild`.
  Started(Result<Pid, StartError>),
  /// Outcome of `TerminateChild` (`false` if the id was unknown).
  Terminated(bool),
}

/// Supervisor failure (start failures during `init`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SupError {
  /// A child failed to start during supervisor init.
  StartFailed(String, StartError),
}

struct ChildState {
  spec: ChildSpec,
  pid: Option<Pid>,
}

/// The supervisor behavior. Usually driven through [`SupervisorSpec`] and
/// [`SupervisorHandle`] rather than used directly.
pub struct SupervisorProcess {
  strategy: Strategy,
  max_restarts: u32,
  within: Duration,
  children: Vec<ChildState>,
  restarts: VecDeque<Instant>,
  /// Pids we terminated on purpose — their queued exit messages must not
  /// be mistaken for crashes or parent exits.
  expected_exits: HashSet<Pid>,
}

impl SupervisorProcess {
  fn note_restart(&mut self, now: Instant) -> bool {
    self.restarts.push_back(now);
    let window_start = now.checked_sub(self.within).unwrap_or(now);
    while let Some(front) = self.restarts.front() {
      if *front < window_start {
        self.restarts.pop_front();
      } else {
        break;
      }
    }
    self.restarts.len() <= self.max_restarts as usize
  }

  fn child_index(&self, pid: &Pid) -> Option<usize> {
    self
      .children
      .iter()
      .position(|c| c.pid.as_ref() == Some(pid))
  }

  fn should_restart(policy: RestartPolicy, reason: &ExitReason) -> bool {
    match policy {
      RestartPolicy::Permanent => true,
      RestartPolicy::Transient => !reason.is_graceful(),
      RestartPolicy::Temporary => false,
    }
  }

  /// Start one child; on failure keep retrying while intensity allows.
  /// Returns `false` when intensity is exhausted.
  fn start_child_with_intensity(&mut self, index: usize, ctx: &ProcessCtx) -> bool {
    let node = ctx.node().clone();
    let clock = node.clock();
    loop {
      let start = Arc::clone(&self.children[index].spec.start);
      match start(&node, ctx.pid()) {
        Ok(pid) => {
          self.children[index].pid = Some(pid);
          return true;
        }
        Err(_) => {
          if !self.note_restart(clock.now()) {
            return false;
          }
        }
      }
    }
  }

  /// Stop a running child following its shutdown policy. Async because it
  /// waits for the exit (bounded by the policy).
  async fn stop_child(&mut self, index: usize, ctx: &ProcessCtx) {
    let Some(pid) = self.children[index].pid.take() else {
      return;
    };
    self.expected_exits.insert(pid.clone());
    let shutdown = self.children[index].spec.shutdown;
    terminate_pid(ctx.node(), &pid, shutdown).await;
  }
}

/// Ask `pid` to stop per `policy` and wait for it to exit.
pub(crate) async fn terminate_pid(node: &ProcessNode, pid: &Pid, policy: ShutdownPolicy) {
  let Some(cell) = node.table().lookup(pid) else {
    return;
  };
  let exited: Deferred<ExitReason, Never> = cell.exited();
  match policy {
    ShutdownPolicy::BrutalKill => {
      node.exit(pid, ExitReason::Killed);
      let _ = exited.wait_future().await;
    }
    ShutdownPolicy::Infinity => {
      node.exit(pid, ExitReason::Shutdown);
      let _ = exited.wait_future().await;
    }
    ShutdownPolicy::Timeout(timeout) => {
      node.exit(pid, ExitReason::Shutdown);
      let clock = node.clock();
      let exited_in_time = {
        let mut wait = pin!(exited.wait_future());
        let mut sleep_env = ();
        let mut deadline = pin!(clock.sleep(timeout).run(&mut sleep_env));
        poll_fn(move |cx| {
          if wait.as_mut().poll(cx).is_ready() {
            return Poll::Ready(true);
          }
          if deadline.as_mut().poll(cx).is_ready() {
            return Poll::Ready(false);
          }
          Poll::Pending
        })
        .await
      };
      if !exited_in_time {
        node.exit(pid, ExitReason::Killed);
        let _ = exited.wait_future().await;
      }
    }
  }
}

impl Process for SupervisorProcess {
  type Args = SupervisorSpec;
  type Env = ();
  type Call = SupCall;
  type Reply = SupReply;
  type Cast = ();
  type Info = ();
  type Error = SupError;

  fn init(spec: SupervisorSpec, ctx: ProcessCtx) -> Effect<Self, SupError, ()> {
    Effect::new(move |_| {
      ctx.trap_exit(true);
      let mut state = SupervisorProcess {
        strategy: spec.strategy,
        max_restarts: spec.max_restarts,
        within: spec.within,
        children: spec
          .children
          .into_iter()
          .map(|spec| ChildState { spec, pid: None })
          .collect(),
        restarts: VecDeque::new(),
        expected_exits: HashSet::new(),
      };
      let node = ctx.node().clone();
      for index in 0..state.children.len() {
        let start = Arc::clone(&state.children[index].spec.start);
        match start(&node, ctx.pid()) {
          Ok(pid) => state.children[index].pid = Some(pid),
          Err(err) => {
            let id = state.children[index].spec.id.clone();
            return Err(SupError::StartFailed(id, err));
          }
        }
      }
      Ok(state)
    })
  }

  fn handle_call(
    mut self,
    msg: SupCall,
    ctx: ProcessCtx,
  ) -> Effect<(SupReply, Next<Self>), SupError, ()> {
    Effect::new_async(move |_| {
      box_future(async move {
        let reply = match msg {
          SupCall::WhichChildren => SupReply::Children(
            self
              .children
              .iter()
              .map(|c| (c.spec.id.clone(), c.pid.clone()))
              .collect(),
          ),
          SupCall::CountChildren => SupReply::Count(self.children.len()),
          SupCall::StartChild(spec) => {
            let start = Arc::clone(&spec.start);
            match start(ctx.node(), ctx.pid()) {
              Ok(pid) => {
                self.children.push(ChildState {
                  spec,
                  pid: Some(pid.clone()),
                });
                SupReply::Started(Ok(pid))
              }
              Err(err) => SupReply::Started(Err(err)),
            }
          }
          SupCall::TerminateChild(id) => match self.children.iter().position(|c| c.spec.id == id) {
            Some(index) => {
              self.stop_child(index, &ctx).await;
              self.children.remove(index);
              SupReply::Terminated(true)
            }
            None => SupReply::Terminated(false),
          },
        };
        Ok((reply, Next::Continue(self)))
      })
    })
  }

  fn handle_info(mut self, msg: Info<()>, ctx: ProcessCtx) -> Effect<Next<Self>, SupError, ()> {
    Effect::new_async(move |_| {
      box_future(async move {
        let Info::Exit { from, reason } = msg else {
          return Ok(Next::Continue(self));
        };

        // Exit we caused on purpose (shutdown protocol) — already handled.
        if self.expected_exits.remove(&from) {
          return Ok(Next::Continue(self));
        }

        let Some(index) = self.child_index(&from) else {
          // Not a child: parent or linked peer. Graceful `Normal` is
          // ignored; anything else stops the whole tree with that reason.
          if reason.is_abnormal() {
            return Ok(Next::Stop(reason, self));
          }
          return Ok(Next::Continue(self));
        };

        self.children[index].pid = None;

        if !Self::should_restart(self.children[index].spec.restart, &reason) {
          return Ok(Next::Continue(self));
        }

        let now = ctx.node().clock().now();
        if !self.note_restart(now) {
          return Ok(Next::Stop(ExitReason::Shutdown, self));
        }

        match self.strategy {
          Strategy::OneForOne => {
            if !self.start_child_with_intensity(index, &ctx) {
              return Ok(Next::Stop(ExitReason::Shutdown, self));
            }
          }
          Strategy::OneForAll => {
            for i in (0..self.children.len()).rev() {
              if i != index {
                self.stop_child(i, &ctx).await;
              }
            }
            for i in 0..self.children.len() {
              if self.children[i].spec.restart == RestartPolicy::Temporary {
                continue;
              }
              if !self.start_child_with_intensity(i, &ctx) {
                return Ok(Next::Stop(ExitReason::Shutdown, self));
              }
            }
          }
          Strategy::RestForOne => {
            for i in ((index + 1)..self.children.len()).rev() {
              self.stop_child(i, &ctx).await;
            }
            for i in index..self.children.len() {
              if self.children[i].spec.restart == RestartPolicy::Temporary {
                continue;
              }
              if !self.start_child_with_intensity(i, &ctx) {
                return Ok(Next::Stop(ExitReason::Shutdown, self));
              }
            }
          }
        }
        Ok(Next::Continue(self))
      })
    })
  }

  fn terminate(mut self, _reason: &ExitReason, ctx: ProcessCtx) -> Effect<(), Never, ()> {
    Effect::new_async(move |_| {
      box_future(async move {
        for index in (0..self.children.len()).rev() {
          self.stop_child(index, &ctx).await;
        }
        Ok(())
      })
    })
  }
}

/// Typed handle to a running supervisor.
#[derive(Clone)]
pub struct SupervisorHandle {
  addr: Addr<SupervisorProcess>,
}

impl SupervisorHandle {
  /// The supervisor's pid.
  pub fn pid(&self) -> &Pid {
    self.addr.pid()
  }

  /// The raw typed address.
  pub fn addr(&self) -> &Addr<SupervisorProcess> {
    &self.addr
  }

  /// `(child id, live pid)` pairs, in start order.
  pub fn which_children(
    &self,
    timeout: Duration,
  ) -> Effect<Vec<(String, Option<Pid>)>, CallError, ()> {
    self
      .addr
      .call(SupCall::WhichChildren, timeout)
      .map(|reply| match reply {
        SupReply::Children(children) => children,
        _ => Vec::new(),
      })
  }

  /// Number of children.
  pub fn count_children(&self, timeout: Duration) -> Effect<usize, CallError, ()> {
    self
      .addr
      .call(SupCall::CountChildren, timeout)
      .map(|reply| match reply {
        SupReply::Count(n) => n,
        _ => 0,
      })
  }

  /// Add and start a new child.
  pub fn start_child(
    &self,
    spec: ChildSpec,
    timeout: Duration,
  ) -> Effect<Result<Pid, StartError>, CallError, ()> {
    self
      .addr
      .call(SupCall::StartChild(spec), timeout)
      .map(|reply| match reply {
        SupReply::Started(result) => result,
        _ => Err(StartError::Failed("unexpected reply".to_string())),
      })
  }

  /// Stop and remove a child by id.
  pub fn terminate_child(
    &self,
    id: impl Into<String>,
    timeout: Duration,
  ) -> Effect<bool, CallError, ()> {
    self
      .addr
      .call(SupCall::TerminateChild(id.into()), timeout)
      .map(|reply| matches!(reply, SupReply::Terminated(true)))
  }

  /// Ask the whole tree to stop gracefully and wait for it.
  pub async fn stop(&self, node: &ProcessNode, policy: ShutdownPolicy) {
    terminate_pid(node, self.addr.pid(), policy).await;
  }
}
