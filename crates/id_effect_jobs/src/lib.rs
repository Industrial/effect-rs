//! Production async messaging for [`id_effect`](https://github.com/Industrial/id_effect).
//!
//! | Feature | Types |
//! |---------|-------|
//! | `memory` (default) | [`MemoryJobRunner`], [`MemoryOutbox`], [`MemoryBroker`] |
//! | `apalis` | `ApalisJobQueue`, `JobQueue` |
//! | `obix` | `ObixOutbox` (relay via `register_event_handler`), `ObixInbox` |
//! | `kafka` | `RdKafkaBroker` |

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

mod broker;
mod error;
mod outbox;
mod runner;

#[cfg(feature = "apalis")]
mod apalis;
#[cfg(feature = "kafka")]
mod kafka;
#[cfg(feature = "obix")]
mod obix_inbox;
#[cfg(feature = "obix")]
mod obix_outbox;

pub use broker::MessageBroker;
pub use error::{JobError, OutboxError};
pub use outbox::{OutboxRecord, OutboxTable};
pub use runner::{FabricJobRunner, JobRecord, JobRunner, JobSpec, JobState};

#[cfg(feature = "memory")]
pub use broker::{KafkaBrokerStub, MemoryBroker};
#[cfg(feature = "memory")]
pub use outbox::{MemoryOutbox, relay_outbox};
#[cfg(feature = "memory")]
pub use runner::{MemoryJobRunner, drain_jobs};

#[cfg(feature = "apalis")]
pub use apalis::{ApalisJobPayload, ApalisJobQueue, JobQueue};

#[cfg(feature = "obix")]
pub use obix_inbox::{InboxPersistResult, ObixInbox};
#[cfg(feature = "obix")]
pub use obix_outbox::{JobsOutboxEvent, ObixOutbox};

#[cfg(feature = "kafka")]
pub use kafka::{RdKafkaBroker, RdKafkaConfig};
