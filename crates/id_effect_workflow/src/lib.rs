//! Durable **append-only** step log with **resume** semantics.
//!
//! | Feature | Backend |
//! |---------|---------|
//! | `memory` (default) | SQLite [`DurableWorkflowLog`] |
//! | `duroxide` | `DuroxideStepJournal` on shared PostgreSQL + duroxide-pg |

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![cfg_attr(
  test,
  allow(
    clippy::bool_assert_comparison,
    clippy::unwrap_used,
    clippy::expect_used
  )
)]

mod error;
mod journal;

#[cfg(feature = "memory")]
mod sqlite_log;

#[cfg(feature = "duroxide")]
mod duroxide_journal;

#[cfg(feature = "duroxide")]
mod providers;

pub use error::WorkflowError;
pub use journal::{
  DistributedJournalConfig, DistributedStepJournal, JournalServer, JournalTransport,
  LoopbackTransport, NetworkJournalStub, RemoteStepJournal, StepJournal,
};

#[cfg(feature = "memory")]
pub use sqlite_log::DurableWorkflowLog;

#[cfg(feature = "duroxide")]
pub use duroxide_journal::{
  DUROXIDE_STEP_JOURNAL_DDL, DuroxideStepJournal, DuroxideWorkflowRuntime,
  bootstrap_duroxide_schema,
};

#[cfg(feature = "duroxide")]
pub use providers::{DuroxideProvider, WorkflowRuntime, provide_duroxide_pg};
