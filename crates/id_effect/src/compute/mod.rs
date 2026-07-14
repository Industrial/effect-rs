//! Compute Fabric
#![allow(missing_docs)]
// stratum 5.5 — expand rustdoc in book pass — resource-aware execution substrate (Stratum 5.5).
//!
//! Every effect passes through Fabric at the runtime boundary. The supervisor compares live
//! telemetry against [`ResourcePolicy`] and adjusts admission, pool sizing, and placement.

mod adaptive;
mod admission;
mod cluster;
mod dispatch;
mod fabric;
mod fiber_pool;
mod policy;
mod rayon_pool;
mod spread;
mod supervisor;
mod telemetry;

pub use adaptive::{
  AdaptiveContext, current_adaptive_context, ensure_run_context, install_fabric,
  refresh_adaptive_context,
};
pub use admission::AdmissionController;
pub use cluster::{
  ClusterResourcePolicy, FabricJobSpec, NodeCandidate, PlacementMode, pick_placement,
  score_candidate,
};
pub(crate) use dispatch::parallel_if_profitable;
pub use fabric::ComputeFabric;
pub use fiber_pool::FiberPool;
pub use policy::{MetricMode, MetricPolicy, RebalanceStrategy, ResourcePolicy, WorkProfile};
pub use rayon_pool::{configure_rayon_threads, install_parallel};
pub use spread::CpuSpreadBucket;
pub use supervisor::ComputeSupervisor;
pub use telemetry::{
  ClusterTelemetry, MockTelemetry, SysinfoTelemetry, TelemetryEngine, TelemetrySnapshot,
};
