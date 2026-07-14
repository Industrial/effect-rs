//! Control-plane introspection types (Axum mounts these as JSON).
//!
//! Kept dependency-free so `id_effect_axum` (or any HTTP stack) can serialize
//! them. Admin actions (`drain`, `rebalance`) are represented as request DTOs.

use serde::{Deserialize, Serialize};

/// Cluster membership view for `/cluster/members`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MemberView {
  /// Logical node name.
  pub name: String,
  /// Boot incarnation (hex).
  pub incarnation: String,
  /// Dial address if known.
  pub addr: Option<String>,
}

/// Process table row for `/cluster/processes`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProcessView {
  /// Display pid.
  pub pid: String,
  /// Hosting node.
  pub node: String,
  /// Registered names, if any.
  pub names: Vec<String>,
}

/// Supervision-tree node for introspection.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TreeNodeView {
  /// Child id or process display name.
  pub id: String,
  /// Pid if running.
  pub pid: Option<String>,
  /// Nested children.
  pub children: Vec<TreeNodeView>,
}

/// Admin: drain this node (stop accepting work, migrate children).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DrainRequest {
  /// Grace period seconds before brutal kill.
  pub grace_secs: u64,
}

/// Admin: trigger Fabric rebalance / ScaleOut pass.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RebalanceRequest {
  /// Optional target node; omit for automatic placement.
  pub target_node: Option<String>,
}

/// Bundle of cluster introspection snapshots.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ClusterStatus {
  /// Members.
  pub members: Vec<MemberView>,
  /// Local processes.
  pub processes: Vec<ProcessView>,
  /// Optional supervision tree root.
  pub tree: Option<TreeNodeView>,
  /// Whether this node is draining.
  pub draining: bool,
}

impl ClusterStatus {
  /// Empty status.
  pub fn new() -> Self {
    Self::default()
  }
}
