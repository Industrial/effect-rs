//! Message envelopes and error types for the process layer.

use std::fmt;

use id_effect::Deferred;

use crate::exit::ExitReason;
use crate::pid::{MonitorRef, Pid};
use crate::process::Process;

/// System-level notification delivered to a process mailbox.
///
/// These arrive through [`crate::process::Process::handle_info`] wrapped in
/// [`Info`]. Like BEAM, **trapped** exit signals and monitor downs are
/// ordinary mailbox messages — they queue in order behind user messages.
/// Untrapped abnormal exits bypass the mailbox entirely (cooperative kill).
#[derive(Clone, Debug)]
pub enum SysMsg {
  /// A linked process exited (delivered only when `trap_exit` is set,
  /// except for `Normal` reasons which are always message-delivered).
  Exit {
    /// The process that exited.
    from: Pid,
    /// Its exit reason.
    reason: ExitReason,
  },
  /// A monitored process exited. Delivered exactly once per monitor.
  Down {
    /// The monitor this notification belongs to.
    monitor: MonitorRef,
    /// The process that exited.
    pid: Pid,
    /// Its exit reason.
    reason: ExitReason,
  },
}

/// Out-of-band message passed to [`crate::process::Process::handle_info`].
#[derive(Clone, Debug)]
pub enum Info<I> {
  /// A linked process exited and this process traps exits.
  Exit {
    /// The process that exited.
    from: Pid,
    /// Its exit reason.
    reason: ExitReason,
  },
  /// A monitored process exited.
  Down {
    /// The monitor this notification belongs to.
    monitor: MonitorRef,
    /// The process that exited.
    pid: Pid,
    /// Its exit reason.
    reason: ExitReason,
  },
  /// A plain user message (BEAM's bare `send`).
  User(I),
}

impl<I> From<SysMsg> for Info<I> {
  fn from(sys: SysMsg) -> Self {
    match sys {
      SysMsg::Exit { from, reason } => Info::Exit { from, reason },
      SysMsg::Down {
        monitor,
        pid,
        reason,
      } => Info::Down {
        monitor,
        pid,
        reason,
      },
    }
  }
}

/// Typed mailbox envelope for a process implementing behavior `P`.
pub enum ProcMsg<P: Process> {
  /// Synchronous request; the caller awaits the [`Deferred`] reply.
  Call(P::Call, Deferred<P::Reply, CallError>),
  /// Fire-and-forget request.
  Cast(P::Cast),
  /// Out-of-band message (exit signals, monitor downs, user info).
  Info(Info<P::Info>),
}

/// Failure modes of [`crate::addr::Addr::call`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallError {
  /// No reply arrived within the deadline. The request may still be
  /// processed — at-most-once semantics, exactly like `GenServer.call`.
  Timeout,
  /// The target exited before replying.
  Down(ExitReason),
  /// The target's mailbox is closed (process dead or dying).
  MailboxClosed,
}

impl fmt::Display for CallError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      CallError::Timeout => f.write_str("call timed out"),
      CallError::Down(r) => write!(f, "callee exited: {r}"),
      CallError::MailboxClosed => f.write_str("mailbox closed"),
    }
  }
}

impl std::error::Error for CallError {}

/// Failure modes of sending to a mailbox.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SendError {
  /// The target process is dead or its mailbox rejected the message
  /// (bounded mailbox full, or shut down).
  Rejected,
}

impl fmt::Display for SendError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      SendError::Rejected => f.write_str("message rejected: mailbox full or closed"),
    }
  }
}

impl std::error::Error for SendError {}

/// Failure modes of starting a child process.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartError {
  /// The start closure refused to start the child.
  Failed(String),
}

impl fmt::Display for StartError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      StartError::Failed(msg) => write!(f, "start failed: {msg}"),
    }
  }
}

impl std::error::Error for StartError {}
