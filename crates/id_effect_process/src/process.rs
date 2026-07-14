//! The [`Process`] behavior trait, [`ProcessCtx`], and the process loop.
//!
//! A process is a supervised fiber draining a typed mailbox. Behavior
//! callbacks are effect-native: each returns `Effect<_, Error, Env>` and
//! state threads through by value (Rust ownership replaces BEAM's
//! per-process heap).
//!
//! ## Loop semantics
//!
//! 1. `init` runs first; failure exits the process with `Error(..)`.
//! 2. Each mailbox message dispatches to `handle_call` / `handle_cast` /
//!    `handle_info`. Handler failure exits the process abnormally
//!    (**without** running `terminate` — the state was consumed by the
//!    failing handler; this is the honest Rust-ownership variant of
//!    BEAM's crash semantics).
//! 3. A cooperative kill (link cascade, `exit/2`, supervisor shutdown)
//!    closes the mailbox; the loop notices at the next message boundary
//!    and runs `terminate` with the intact state.
//! 4. On every exit path the process is finalized exactly once: exit
//!    signals fan out to links, monitors fire, and registered names are
//!    dropped.

use id_effect::runtime::Never;
use id_effect::{Effect, Queue, box_future, succeed};

use crate::addr::Addr;
use crate::exit::ExitReason;
use crate::msg::{Info, ProcMsg};
use crate::node::ProcessNode;
use crate::pid::{MonitorRef, Pid};

/// Continue-or-stop decision returned by behavior callbacks.
pub enum Next<P> {
  /// Keep processing messages with the new state.
  Continue(P),
  /// Stop with `reason`; `terminate` runs with the final state.
  Stop(ExitReason, P),
}

/// An OTP-style behavior: GenServer-shaped, effect-native.
///
/// All callbacks run inside the process fiber with exclusive access to the
/// environment `Env` (capabilities, clients, config).
pub trait Process: Sized + 'static {
  /// Arguments passed to [`Process::init`] (crosses the spawn boundary).
  type Args: Send + 'static;
  /// Effect environment for all callbacks.
  type Env: Send + 'static;
  /// Synchronous request payload.
  type Call: Send + 'static;
  /// Reply to a [`Process::handle_call`].
  type Reply: Clone + Send + Sync + 'static;
  /// Fire-and-forget payload.
  type Cast: Send + 'static;
  /// User out-of-band message payload.
  type Info: Send + 'static;
  /// Behavior error; rendered into [`ExitReason::Error`] on crash.
  type Error: std::fmt::Debug + Clone + Send + Sync + 'static;

  /// Build the initial state. Failure exits the process immediately.
  fn init(args: Self::Args, ctx: ProcessCtx) -> Effect<Self, Self::Error, Self::Env>;

  /// Handle a synchronous request; the reply completes the caller's wait.
  #[allow(clippy::type_complexity)]
  fn handle_call(
    self,
    msg: Self::Call,
    ctx: ProcessCtx,
  ) -> Effect<(Self::Reply, Next<Self>), Self::Error, Self::Env>;

  /// Handle a fire-and-forget message. Default: ignore and continue.
  fn handle_cast(
    self,
    msg: Self::Cast,
    ctx: ProcessCtx,
  ) -> Effect<Next<Self>, Self::Error, Self::Env> {
    let _ = (msg, ctx);
    succeed(Next::Continue(self))
  }

  /// Handle exit signals, monitor downs, and user info messages.
  /// Default: ignore and continue.
  fn handle_info(
    self,
    msg: Info<Self::Info>,
    ctx: ProcessCtx,
  ) -> Effect<Next<Self>, Self::Error, Self::Env> {
    let _ = (msg, ctx);
    succeed(Next::Continue(self))
  }

  /// Graceful-cleanup hook. Runs on [`Next::Stop`] and on cooperative
  /// kills; **not** on handler crashes (state already consumed).
  fn terminate(self, reason: &ExitReason, ctx: ProcessCtx) -> Effect<(), Never, Self::Env> {
    let _ = (reason, ctx);
    succeed(())
  }
}

/// Handle to the current process, passed to every behavior callback.
///
/// Cloneable and cheap; also usable from outside the callbacks (e.g. by
/// custom start logic) as long as the process is alive.
#[derive(Clone)]
pub struct ProcessCtx {
  pid: Pid,
  node: ProcessNode,
}

impl ProcessCtx {
  pub(crate) fn new(pid: Pid, node: ProcessNode) -> Self {
    Self { pid, node }
  }

  /// This process's pid.
  pub fn pid(&self) -> &Pid {
    &self.pid
  }

  /// The node this process runs on.
  pub fn node(&self) -> &ProcessNode {
    &self.node
  }

  /// Convert link exits into [`Info::Exit`] messages instead of dying
  /// (BEAM's `process_flag(trap_exit, true)`).
  pub fn trap_exit(&self, trap: bool) {
    if let Some(cell) = self.node.table().lookup(&self.pid) {
      cell.set_trap_exit(trap);
    }
  }

  /// Bidirectionally link this process with `peer`.
  pub fn link(&self, peer: &Pid) {
    self.node.link(&self.pid, peer);
  }

  /// Remove a link previously created with [`ProcessCtx::link`].
  pub fn unlink(&self, peer: &Pid) {
    self.node.unlink(&self.pid, peer);
  }

  /// Monitor `peer`; a [`Info::Down`] message arrives in this process's
  /// mailbox exactly once when `peer` exits.
  pub fn monitor(&self, peer: &Pid) -> MonitorRef {
    self.node.monitor_into(&self.pid, peer)
  }

  /// Remove a monitor before it fires.
  pub fn demonitor(&self, peer: &Pid, monitor: MonitorRef) {
    self.node.demonitor(peer, monitor);
  }

  /// Spawn a child process linked to this one.
  pub fn spawn_link<P: Process>(&self, args: P::Args, env: P::Env) -> Addr<P> {
    self.node.spawn_linked_to::<P>(&self.pid, args, env)
  }

  /// Spawn an unlinked process on the same node.
  pub fn spawn<P: Process>(&self, args: P::Args, env: P::Env) -> Addr<P> {
    self.node.spawn::<P>(args, env)
  }

  /// Whether this process has been asked to die (cooperative kill).
  pub fn is_killed(&self) -> bool {
    self
      .node
      .table()
      .lookup(&self.pid)
      .map(|cell| cell.recorded_kill().is_some())
      .unwrap_or(true)
  }
}

/// Build the complete process-loop effect for behavior `P`.
///
/// The effect never fails (`E = Never`); every failure mode is converted
/// into an [`ExitReason`] and fanned out by finalization.
pub(crate) fn process_loop<P: Process>(
  args: P::Args,
  ctx: ProcessCtx,
  mailbox: Queue<ProcMsg<P>>,
) -> Effect<(), Never, P::Env> {
  Effect::new_async(move |env| {
    box_future(async move {
      let pid = ctx.pid().clone();
      let node = ctx.node().clone();
      let cell = node.table().lookup(&pid);

      let recorded = |fallback: ExitReason| -> ExitReason {
        cell
          .as_ref()
          .and_then(|c| c.recorded_kill())
          .unwrap_or(fallback)
      };

      let reason: ExitReason = match P::init(args, ctx.clone()).run(env).await {
        Err(e) => ExitReason::error(e),
        Ok(mut state) => {
          let exit: ExitReason;
          loop {
            // Kill checkpoint: a cooperative kill may have arrived while a
            // handler ran or while messages were still buffered.
            if let Some(c) = cell.as_ref()
              && let Some(r) = c.recorded_kill()
            {
              let _ = P::terminate(state, &r, ctx.clone()).run(env).await;
              exit = r;
              break;
            }
            let msg = match mailbox.take().run(&mut ()).await {
              Ok(m) => m,
              Err(_) => {
                // Mailbox closed: either a kill (reason recorded) or all
                // senders dropped (treated as a graceful stop).
                let r = recorded(ExitReason::Normal);
                let _ = P::terminate(state, &r, ctx.clone()).run(env).await;
                exit = r;
                break;
              }
            };
            let next = match msg {
              ProcMsg::Call(m, reply) => {
                match P::handle_call(state, m, ctx.clone()).run(env).await {
                  Ok((rep, next)) => {
                    reply.try_succeed(rep);
                    next
                  }
                  Err(e) => {
                    exit = ExitReason::error(e);
                    break;
                  }
                }
              }
              ProcMsg::Cast(m) => match P::handle_cast(state, m, ctx.clone()).run(env).await {
                Ok(next) => next,
                Err(e) => {
                  exit = ExitReason::error(e);
                  break;
                }
              },
              ProcMsg::Info(i) => match P::handle_info(state, i, ctx.clone()).run(env).await {
                Ok(next) => next,
                Err(e) => {
                  exit = ExitReason::error(e);
                  break;
                }
              },
            };
            match next {
              Next::Continue(s) => state = s,
              Next::Stop(r, s) => {
                let _ = P::terminate(s, &r, ctx.clone()).run(env).await;
                exit = r;
                break;
              }
            }
          }
          exit
        }
      };

      node.finalize_process(&pid, reason);
      Ok(())
    })
  })
}
