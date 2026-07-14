//! Cluster placement scoring and ScaleOut job specs.

use super::policy::{ResourcePolicy, WorkProfile};
use super::supervisor::ComputeSupervisor;
use super::telemetry::{TelemetryEngine, TelemetrySnapshot};

/// How work is placed across cluster nodes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlacementMode {
  /// Prefer local Fabric until saturated.
  LocalFirst,
  /// Spread load evenly across workers.
  Spread,
  /// Pin to a named node.
  Affinity {
    /// Target node identifier.
    node: String,
  },
}

/// Cluster-wide and per-node resource rules.
#[derive(Debug, Clone, PartialEq)]
pub struct ClusterResourcePolicy {
  /// Cluster aggregate caps.
  pub global: ResourcePolicy,
  /// Node-local caps.
  pub per_node: ResourcePolicy,
  /// Placement strategy when scaling out.
  pub placement: PlacementMode,
}

impl ClusterResourcePolicy {
  /// Local-first placement with identical global and per-node caps.
  pub fn local_first(policy: ResourcePolicy) -> Self {
    Self {
      global: policy.clone(),
      per_node: policy,
      placement: PlacementMode::LocalFirst,
    }
  }
}

/// Candidate node with a telemetry snapshot for scoring.
#[derive(Debug, Clone)]
pub struct NodeCandidate {
  /// Logical node name.
  pub name: String,
  /// Latest telemetry.
  pub snapshot: TelemetrySnapshot,
}

/// Score in `[0.0, 1.0]` — higher is better (more headroom).
pub fn score_candidate(candidate: &NodeCandidate, policy: &ResourcePolicy) -> f32 {
  if !candidate.snapshot.satisfies(policy) {
    return 0.0;
  }
  candidate.snapshot.min_headroom(policy).clamp(0.0, 1.0)
}

/// Pick the best candidate under `mode` (empty → None).
pub fn pick_placement<'a, I: IntoIterator<Item = &'a NodeCandidate>>(
  candidates: I,
  policy: &ClusterResourcePolicy,
) -> Option<&'a NodeCandidate> {
  let mut best: Option<(&NodeCandidate, f32)> = None;
  for c in candidates {
    match &policy.placement {
      PlacementMode::Affinity { node } if &c.name != node => continue,
      PlacementMode::Affinity { .. } | PlacementMode::LocalFirst | PlacementMode::Spread => {}
    }
    let s = score_candidate(c, &policy.per_node);
    if s <= 0.0 {
      continue;
    }
    match best {
      None => best = Some((c, s)),
      Some((_, bs)) if s > bs => best = Some((c, s)),
      _ => {}
    }
  }
  best.map(|(c, _)| c)
}

/// Serializable job payload produced by [`ComputeSupervisor::scale_out`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FabricJobSpec {
  /// Handler dispatch key.
  pub name: String,
  /// Opaque effect / env bytes.
  pub payload: Vec<u8>,
  /// Work classification for worker placement.
  pub work_profile: WorkProfile,
  /// Optional target node from placement scoring.
  pub target_node: Option<String>,
}

impl<E: TelemetryEngine> ComputeSupervisor<E> {
  /// Build a cluster offload job when local Fabric is saturated.
  pub fn scale_out(
    &self,
    name: impl Into<String>,
    payload: Vec<u8>,
    profile: WorkProfile,
  ) -> FabricJobSpec {
    FabricJobSpec {
      name: name.into(),
      payload,
      work_profile: profile,
      target_node: None,
    }
  }

  /// Scale out with placement scoring across `candidates`.
  pub fn scale_out_placed(
    &self,
    name: impl Into<String>,
    payload: Vec<u8>,
    profile: WorkProfile,
    cluster: &ClusterResourcePolicy,
    candidates: &[NodeCandidate],
  ) -> FabricJobSpec {
    let target = pick_placement(candidates, cluster).map(|c| c.name.clone());
    FabricJobSpec {
      name: name.into(),
      payload,
      work_profile: profile,
      target_node: target,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::compute::{AdmissionController, MockTelemetry};

  #[test]
  fn scale_out_returns_fabric_job_spec() {
    let telemetry = MockTelemetry::new(0.9, 0.9);
    let admission = std::sync::Arc::new(AdmissionController::new(1, 4));
    let policy = ResourcePolicy::memory_cap_max_cpu(0.85);
    let sup = ComputeSupervisor::new(policy, telemetry, admission);
    let job = sup.scale_out(
      "remote-work",
      b"payload".to_vec(),
      WorkProfile::CpuIntensive,
    );
    assert_eq!(job.name, "remote-work");
    assert_eq!(job.work_profile, WorkProfile::CpuIntensive);
    assert_eq!(job.payload, b"payload");
    assert!(job.target_node.is_none());
  }

  #[test]
  fn pick_placement_prefers_more_headroom() {
    let policy = ClusterResourcePolicy::local_first(ResourcePolicy::memory_cap_max_cpu(0.95));
    let a = NodeCandidate {
      name: "a".into(),
      snapshot: TelemetrySnapshot {
        cpu_pct: 0.8,
        mem_pct: 0.8,
      },
    };
    let b = NodeCandidate {
      name: "b".into(),
      snapshot: TelemetrySnapshot {
        cpu_pct: 0.2,
        mem_pct: 0.2,
      },
    };
    let picked = pick_placement([&a, &b], &policy).expect("pick");
    assert_eq!(picked.name, "b");
  }

  #[test]
  fn local_first_policy_clones_caps() {
    let policy = ResourcePolicy::memory_cap_max_cpu(0.85);
    let cluster = ClusterResourcePolicy::local_first(policy.clone());
    assert_eq!(cluster.global, policy);
    assert_eq!(cluster.per_node, policy);
    assert_eq!(cluster.placement, PlacementMode::LocalFirst);
  }
}
