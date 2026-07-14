//! [`ExitReason`] — the data carried by exit signals and monitor downs.
//!
//! Exit reasons are plain data (strings for errors) so they can cross node
//! boundaries without code mobility, exactly like BEAM exit terms. The
//! normative semantics table lives in ADR 0009.

use std::fmt;

use id_effect::Cause;

/// Why a process exited. This is the payload of link exit signals and
/// monitor `Down` notifications.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExitReason {
  /// Graceful completion. Does **not** kill linked processes.
  Normal,
  /// Graceful stop requested by a supervisor. Trappable.
  Shutdown,
  /// Abnormal termination described by the process's error type
  /// (rendered to a string — exit reasons are data, not code).
  Error(String),
  /// Untrappable kill (`exit(pid, kill)` / brutal supervisor shutdown).
  Killed,
  /// The fiber was interrupted by the runtime.
  Interrupted,
  /// The target process did not exist when a link/monitor was set up.
  NoProc,
  /// The node hosting the process became unreachable (distribution layer).
  NoConnection,
}

impl ExitReason {
  /// Build an [`ExitReason::Error`] from any debuggable error value.
  pub fn error(err: impl fmt::Debug) -> Self {
    Self::Error(format!("{err:?}"))
  }

  /// `true` for reasons that propagate over links to non-trapping
  /// processes (everything except [`ExitReason::Normal`]).
  pub fn is_abnormal(&self) -> bool {
    !matches!(self, ExitReason::Normal)
  }

  /// `true` for reasons a supervisor treats as a *successful* stop when
  /// deciding `Transient` restarts (`Normal` and `Shutdown`).
  pub fn is_graceful(&self) -> bool {
    matches!(self, ExitReason::Normal | ExitReason::Shutdown)
  }

  /// Map a structured [`Cause`] into a wire-safe reason.
  pub fn from_cause<E: fmt::Debug>(cause: &Cause<E>) -> Self {
    match cause {
      Cause::Fail(e) => Self::Error(format!("{e:?}")),
      Cause::Die(msg) => Self::Error(msg.clone()),
      Cause::Interrupt(_) => Self::Interrupted,
      Cause::Both(l, _) | Cause::Then(l, _) => Self::from_cause(l),
    }
  }
}

impl fmt::Display for ExitReason {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      ExitReason::Normal => f.write_str("normal"),
      ExitReason::Shutdown => f.write_str("shutdown"),
      ExitReason::Error(e) => write!(f, "error: {e}"),
      ExitReason::Killed => f.write_str("killed"),
      ExitReason::Interrupted => f.write_str("interrupted"),
      ExitReason::NoProc => f.write_str("noproc"),
      ExitReason::NoConnection => f.write_str("noconnection"),
    }
  }
}
