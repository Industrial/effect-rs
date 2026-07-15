//! # id_effect_process — BEAM/OTP-style processes for id_effect
//!
//! The local half of the Process Mesh (ADR 0009): typed mailboxes,
//! GenServer-shaped behaviors, links, monitors, `trap_exit`, exit-signal
//! propagation, and multi-strategy supervision trees — all effect-native
//! and deterministic under `TestClock`.
//!
//! ## Quick tour
//!
//! - [`Process`]: the behavior trait (`init` / `handle_call` /
//!   `handle_cast` / `handle_info` / `terminate`, each returning
//!   `Effect<_, Error, Env>`).
//! - [`ProcessNode`]: spawns processes, owns the process table, links,
//!   monitors, exit signals, and the name registry.
//! - [`Addr`]: typed `send` / `cast` / `call` handle.
//! - [`SupervisorSpec`] / [`ChildSpec`] / [`Strategy`]: OTP supervision
//!   trees with restart intensity measured on the injected `Clock`.
//! - [`DynamicSupervisor`]: homogeneous, on-demand children.
//!
//! ## Guarantees (normative, ADR 0009)
//!
//! - Local send: ordered per sender→receiver pair; never lost while the
//!   receiver is alive.
//! - Monitors: exactly one `Down` delivery (crash, normal, or `noproc`).
//! - Links: BEAM rules — trapping processes receive exits as messages;
//!   non-trapping processes ignore `Normal`, die on abnormal reasons;
//!   direct `kill` is untrappable.
//! - Kills are **cooperative**: they take effect at the next message
//!   boundary (handlers that never yield cannot be preempted — the same
//!   caveat as a NIF in BEAM).

#![deny(missing_docs)]

pub mod addr;
pub mod cell;
pub mod clock_util;
pub mod dynamic;
pub mod exit;
pub mod msg;
pub mod node;
pub mod pid;
pub mod process;
pub mod supervisor;

pub use addr::Addr;
pub use cell::{MonitorSink, ProcessCell, ProcessTable};
pub use clock_util::SpinClock;
pub use dynamic::{DynCall, DynReply, DynamicSupervisor, DynamicSupervisorSpec, StartChildFn};
pub use exit::ExitReason;
pub use msg::{CallError, Info, ProcMsg, SendError, StartError, SysMsg};
pub use node::{Job, ProcessNode, Spawner};
pub use pid::{MonitorRef, NodeName, Pid};
pub use process::{Next, Process, ProcessCtx};
pub use supervisor::{
  ChildKind, ChildSpec, RestartPolicy, ShutdownPolicy, StartFn, Strategy, SupCall, SupError,
  SupReply, SupervisorHandle, SupervisorProcess, SupervisorSpec,
};
