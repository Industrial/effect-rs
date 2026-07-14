//! [`ProcessCell`] and [`ProcessTable`] — the node-local process registry
//! and the exit-signal / monitor fan-out machinery.
//!
//! Every live process has exactly one cell in the table. The cell is
//! type-erased: typed mailbox access lives in [`crate::addr::Addr`]; the
//! cell only knows how to (a) deliver a [`SysMsg`] into the mailbox and
//! (b) kill the process cooperatively (record reason, cancel, close the
//! mailbox).
//!
//! ## Delivery guarantees (ADR 0009)
//!
//! - Monitors fire **exactly once**: the monitor map is drained once at
//!   finalization under a lock that also serializes late registrations
//!   (a monitor added after death fires immediately, never twice).
//! - Link exit signals honour BEAM rules: trapping processes receive all
//!   link exits as messages; non-trapping processes ignore `Normal`,
//!   die on abnormal reasons, and cannot trap a direct `kill`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use id_effect::runtime::Never;
use id_effect::{CancellationToken, Deferred, run_blocking};

use crate::exit::ExitReason;
use crate::msg::SysMsg;
use crate::pid::{MonitorRef, Pid};

/// Callback fired when a monitored process exits. Public so distribution
/// layers can register wire-forwarding monitors.
pub type MonitorSink = Box<dyn Fn(MonitorRef, Pid, ExitReason) + Send + Sync>;

/// Deliver a system message into the typed mailbox; `false` if the mailbox
/// is closed.
pub(crate) type SysDeliver = Box<dyn Fn(SysMsg) -> bool + Send + Sync>;

/// Close the mailbox to producers (buffered messages stay readable).
pub(crate) type MailboxClose = Box<dyn Fn() + Send + Sync>;

/// Monitor bookkeeping guarded by one lock so "fire exactly once" holds
/// even when a monitor races with process death.
struct MonitorState {
  /// `Some(reason)` once the process has finalized.
  fired: Option<ExitReason>,
  sinks: HashMap<MonitorRef, MonitorSink>,
}

/// Link bookkeeping under the same discipline as [`MonitorState`].
struct LinkState {
  done: Option<ExitReason>,
  set: Vec<Pid>,
}

/// Type-erased control block for one live process.
pub struct ProcessCell {
  pid: Pid,
  deliver: SysDeliver,
  close_mailbox: MailboxClose,
  cancel: CancellationToken,
  trap_exit: AtomicBool,
  /// First kill reason wins; read by the process loop between messages.
  kill_reason: Mutex<Option<ExitReason>>,
  monitors: Mutex<MonitorState>,
  links: Mutex<LinkState>,
  /// Completed exactly once at finalization.
  exited: Deferred<ExitReason, Never>,
  /// Names registered for this pid (cleaned up at finalization).
  registered_names: Mutex<Vec<String>>,
}

impl ProcessCell {
  pub(crate) fn new(
    pid: Pid,
    deliver: SysDeliver,
    close_mailbox: MailboxClose,
    cancel: CancellationToken,
  ) -> Self {
    let exited = run_blocking(Deferred::<ExitReason, Never>::make(), ())
      .unwrap_or_else(|_| unreachable!("Deferred::make is infallible"));
    Self {
      pid,
      deliver,
      close_mailbox,
      cancel,
      trap_exit: AtomicBool::new(false),
      kill_reason: Mutex::new(None),
      monitors: Mutex::new(MonitorState {
        fired: None,
        sinks: HashMap::new(),
      }),
      links: Mutex::new(LinkState {
        done: None,
        set: Vec::new(),
      }),
      exited,
      registered_names: Mutex::new(Vec::new()),
    }
  }

  /// This process's pid.
  pub fn pid(&self) -> &Pid {
    &self.pid
  }

  /// Cooperative-cancellation token cancelled when the process is killed.
  pub fn cancellation(&self) -> &CancellationToken {
    &self.cancel
  }

  /// Whether this process converts link exits into [`SysMsg::Exit`]
  /// messages instead of dying.
  pub fn traps_exits(&self) -> bool {
    self.trap_exit.load(Ordering::SeqCst)
  }

  pub(crate) fn set_trap_exit(&self, trap: bool) {
    self.trap_exit.store(trap, Ordering::SeqCst);
  }

  /// Deliver a system message into the mailbox (`false` if closed).
  pub(crate) fn deliver_sys(&self, msg: SysMsg) -> bool {
    (self.deliver)(msg)
  }

  /// Kill cooperatively: record `reason` (first wins), cancel the token,
  /// and close the mailbox so the loop stops at its next checkpoint.
  pub(crate) fn kill(&self, reason: ExitReason) {
    {
      let mut guard = self.kill_reason.lock().expect("kill_reason mutex poisoned");
      if guard.is_none() {
        *guard = Some(reason);
      }
    }
    self.cancel.cancel();
    (self.close_mailbox)();
  }

  /// The recorded kill reason, if this process has been asked to die.
  pub(crate) fn recorded_kill(&self) -> Option<ExitReason> {
    self
      .kill_reason
      .lock()
      .expect("kill_reason mutex poisoned")
      .clone()
  }

  /// One-shot deferred completed with the final exit reason.
  pub fn exited(&self) -> Deferred<ExitReason, Never> {
    self.exited.clone()
  }

  /// Register a monitor sink. If the process already finalized, the sink
  /// fires immediately with the recorded reason.
  pub(crate) fn add_monitor(&self, sink: MonitorSink) -> MonitorRef {
    let monitor = MonitorRef::fresh();
    let fire_now = {
      let mut state = self.monitors.lock().expect("monitors mutex poisoned");
      match &state.fired {
        Some(reason) => Some((reason.clone(), sink)),
        None => {
          state.sinks.insert(monitor, sink);
          None
        }
      }
    };
    if let Some((reason, sink)) = fire_now {
      sink(monitor, self.pid.clone(), reason);
    }
    monitor
  }

  /// Remove a monitor before it fires. Returns whether it was present.
  pub(crate) fn remove_monitor(&self, monitor: MonitorRef) -> bool {
    self
      .monitors
      .lock()
      .expect("monitors mutex poisoned")
      .sinks
      .remove(&monitor)
      .is_some()
  }

  /// Add a bidirectional-link peer. Returns the exit reason immediately if
  /// this process already finalized (caller must deliver the exit signal).
  pub(crate) fn add_link(&self, peer: Pid) -> Result<(), ExitReason> {
    let mut state = self.links.lock().expect("links mutex poisoned");
    match &state.done {
      Some(reason) => Err(reason.clone()),
      None => {
        if !state.set.contains(&peer) {
          state.set.push(peer);
        }
        Ok(())
      }
    }
  }

  /// Remove a link peer (unlink or peer death).
  pub(crate) fn remove_link(&self, peer: &Pid) {
    let mut state = self.links.lock().expect("links mutex poisoned");
    state.set.retain(|p| p != peer);
  }

  /// Mark finalized; returns the drained links and monitors so the caller
  /// can fan out exit signals outside the locks.
  pub(crate) fn finalize(
    &self,
    reason: &ExitReason,
  ) -> (Vec<Pid>, Vec<(MonitorRef, MonitorSink)>, Vec<String>) {
    let links = {
      let mut state = self.links.lock().expect("links mutex poisoned");
      state.done = Some(reason.clone());
      std::mem::take(&mut state.set)
    };
    let monitors = {
      let mut state = self.monitors.lock().expect("monitors mutex poisoned");
      state.fired = Some(reason.clone());
      state.sinks.drain().collect::<Vec<_>>()
    };
    let names = std::mem::take(
      &mut *self
        .registered_names
        .lock()
        .expect("registered_names mutex poisoned"),
    );
    self.exited.try_succeed(reason.clone());
    (links, monitors, names)
  }

  pub(crate) fn note_registered_name(&self, name: String) {
    self
      .registered_names
      .lock()
      .expect("registered_names mutex poisoned")
      .push(name);
  }

  pub(crate) fn forget_registered_name(&self, name: &str) {
    self
      .registered_names
      .lock()
      .expect("registered_names mutex poisoned")
      .retain(|n| n != name);
  }
}

/// Node-local table of live processes plus the name registry.
pub struct ProcessTable {
  processes: Mutex<HashMap<u64, Arc<ProcessCell>>>,
  registry: Mutex<HashMap<String, RegistryEntry>>,
}

pub(crate) struct RegistryEntry {
  pub(crate) pid: Pid,
  /// `Addr<P>` boxed as `Any` for typed lookup.
  pub(crate) addr: Box<dyn std::any::Any + Send + Sync>,
}

impl ProcessTable {
  pub(crate) fn new() -> Self {
    Self {
      processes: Mutex::new(HashMap::new()),
      registry: Mutex::new(HashMap::new()),
    }
  }

  pub(crate) fn insert(&self, cell: Arc<ProcessCell>) {
    self
      .processes
      .lock()
      .expect("process table mutex poisoned")
      .insert(cell.pid().local_id(), cell);
  }

  /// Look up a live process cell by pid.
  pub fn lookup(&self, pid: &Pid) -> Option<Arc<ProcessCell>> {
    self
      .processes
      .lock()
      .expect("process table mutex poisoned")
      .get(&pid.local_id())
      .filter(|cell| cell.pid() == pid)
      .cloned()
  }

  pub(crate) fn remove(&self, pid: &Pid) {
    self
      .processes
      .lock()
      .expect("process table mutex poisoned")
      .remove(&pid.local_id());
  }

  /// Number of live processes.
  pub fn len(&self) -> usize {
    self
      .processes
      .lock()
      .expect("process table mutex poisoned")
      .len()
  }

  /// Whether no processes are alive.
  pub fn is_empty(&self) -> bool {
    self.len() == 0
  }

  /// Pids of all live processes (snapshot).
  pub fn pids(&self) -> Vec<Pid> {
    self
      .processes
      .lock()
      .expect("process table mutex poisoned")
      .values()
      .map(|c| c.pid().clone())
      .collect()
  }

  pub(crate) fn registry(&self) -> &Mutex<HashMap<String, RegistryEntry>> {
    &self.registry
  }
}
