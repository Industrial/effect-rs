//! **Stratum 7 — Concurrency Primitives**
//!
//! Lightweight threads of execution and cooperative cancellation, built from Strata 0–6.
//!
//! | Submodule | Provides | Depends on |
//! |-----------|----------|------------|
//! | [`fiber_id`] | [`FiberId`] branded `u64` | `std::sync::atomic` only |
//! | [`cancel`] | [`CancellationToken`], [`check_interrupt`] | Stratum 6 (`runtime::Never`, `runtime::run_*`), `async_notify` |
//! | [`fiber_handle`] | [`FiberHandle`], [`FiberStatus`], fiber utilities | Stratum 6 (`runtime::{run_blocking, run_async, Never}`), Stratum 4 (`failure::{Cause,Exit}`), [`fiber_id`] (this stratum), `deferred`, `scope` |
//! | [`supervisor`] | [`Supervisor`], [`SupervisorPolicy`], [`supervised`] | [`Scope`], [`CancellationToken`], [`scheduling`](crate::scheduling), [`failure::Cause`](crate::failure::Cause) |
//!
//! ## Design
//!
//! Concurrency in this system is modelled through typed, observable _fibers_:
//!
//! ```text
//! FiberId       — stable, branded identifier (monotonic u64)
//! FiberHandle   — typed reference to a spawned fiber's completion rendezvous
//! FiberStatus   — non-blocking state snapshot (Running | Succeeded | Failed | Interrupted)
//! CancellationToken — cooperative propagating interrupt signal
//! ```
//!
//! The key operations follow the Effect.ts fiber algebra:
//!
//! ```text
//! join[A, E]:     FiberHandle[A, E] → Result[A, Cause[E]]
//! awaitExit[A, E]: FiberHandle[A, E] → Effect[Exit[A, E], Never, ()]
//! interrupt[A, E]: FiberHandle[A, E] → Bool
//! zip, orElse, map — combinators over handles
//! ```
//!
//! ## Public API
//!
//! Re-exported at the crate root (via [`crate::runtime`] for backward compatibility):
//! [`FiberId`], [`FiberHandle`], [`FiberStatus`], [`CancellationToken`], [`check_interrupt`],
//! [`fiber_all`], [`interrupt_all`], [`fiber_succeed`], [`fiber_never`].

mod async_notify;
pub mod cancel;
pub mod fiber_handle;
pub mod fiber_id;
pub mod fiber_ref;
pub mod supervisor;

pub use cancel::{CancellationToken, check_interrupt};
pub use fiber_handle::{
  FiberHandle, FiberStatus, fiber_all, fiber_never, fiber_succeed, interrupt_all,
};
pub use fiber_id::FiberId;
pub use fiber_ref::{FiberRef, with_fiber_id};
pub use supervisor::{Supervisor, SupervisorPolicy, supervised};
