//! Cluster placement scoring and ScaleOut job specs.

use super::policy::{ResourcePolicy, WorkProfile};
use super::supervisor::ComputeSupervisor;
use super::telemetry::{TelemetryEngine, TelemetrySnapshot};

/// How work is placed across cluster nodes.
///
/// Every variant selects among nodes that satisfy the per-node
/// [`ResourcePolicy`]; they differ only in how they choose within that eligible
/// set. Selection is pure and deterministic (see [`pick_placement`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlacementMode {
  /// Least-loaded: pick the eligible node with the most policy headroom.
  LocalFirst,
  /// Spread: pick the eligible node with the lowest raw utilization
  /// (`cpu_pct + mem_pct`), balancing absolute load rather than headroom.
  Spread,
  /// Prefer-explicit: pin to a named node, honored only if it is eligible.
  Affinity {
    /// Target node identifier.
    node: String,
  },
  /// Round-robin: deterministic rotation across the eligible nodes (ordered by
  /// name). The caller supplies `turn`; the chosen node is `eligible[turn % len]`.
  /// Stateless and pure — advancing the rotation is the caller's responsibility.
  RoundRobin {
    /// Monotonic turn counter supplied by the caller.
    turn: u64,
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

/// Load telemetry a mesh node reports about itself, the mesh-facing input to
/// placement.
///
/// `id_effect_node` gathers one [`LoadReport`] per reachable node (piggybacked
/// on membership gossip) and feeds a slice of them to [`place`]. This type is
/// deliberately dependency-free (no serde) so the core crate stays lean; the
/// mesh layer owns wire encoding and converts to/from its own transport form.
/// Convert to a [`NodeCandidate`] with [`From`] before scoring.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadReport {
  /// Logical node name (matches `Mesh::node_name`).
  pub node: String,
  /// Fraction of CPU in use, `[0.0, 1.0]`.
  pub cpu_pct: f32,
  /// Fraction of memory in use, `[0.0, 1.0]`.
  pub mem_pct: f32,
}

impl LoadReport {
  /// Build a report from a node name and raw utilization fractions.
  pub fn new(node: impl Into<String>, cpu_pct: f32, mem_pct: f32) -> Self {
    Self {
      node: node.into(),
      cpu_pct,
      mem_pct,
    }
  }
}

impl From<&LoadReport> for NodeCandidate {
  fn from(report: &LoadReport) -> Self {
    NodeCandidate {
      name: report.node.clone(),
      snapshot: TelemetrySnapshot {
        cpu_pct: report.cpu_pct,
        mem_pct: report.mem_pct,
      },
    }
  }
}

/// Score in `[0.0, 1.0]` — higher is better (more headroom).
pub fn score_candidate(candidate: &NodeCandidate, policy: &ResourcePolicy) -> f32 {
  if !candidate.snapshot.satisfies(policy) {
    return 0.0;
  }
  candidate.snapshot.min_headroom(policy).clamp(0.0, 1.0)
}

/// Pick the best eligible candidate under the policy's [`PlacementMode`].
///
/// A candidate is *eligible* when it satisfies the per-node policy
/// ([`score_candidate`] returns > 0). Ties are broken deterministically by node
/// name, so the result never depends on iteration order. Returns `None` when no
/// candidate is eligible.
pub fn pick_placement<'a, I: IntoIterator<Item = &'a NodeCandidate>>(
  candidates: I,
  policy: &ClusterResourcePolicy,
) -> Option<&'a NodeCandidate> {
  let mut eligible: Vec<(&'a NodeCandidate, f32)> = candidates
    .into_iter()
    .filter(|c| match &policy.placement {
      PlacementMode::Affinity { node } => &c.name == node,
      _ => true,
    })
    .filter_map(|c| {
      let score = score_candidate(c, &policy.per_node);
      (score > 0.0).then_some((c, score))
    })
    .collect();

  match &policy.placement {
    // Prefer-explicit: the named node survives the eligibility filter or nothing.
    PlacementMode::Affinity { .. } => eligible.into_iter().next().map(|(c, _)| c),
    // Least-loaded: most policy headroom first, lowest name to break ties.
    PlacementMode::LocalFirst => {
      eligible.sort_by(|(ca, sa), (cb, sb)| {
        sb.partial_cmp(sa)
          .unwrap_or(std::cmp::Ordering::Equal)
          .then_with(|| ca.name.cmp(&cb.name))
      });
      eligible.into_iter().next().map(|(c, _)| c)
    }
    // Spread: lowest absolute utilization first, lowest name to break ties.
    PlacementMode::Spread => {
      eligible.sort_by(|(ca, _), (cb, _)| {
        let load_a = ca.snapshot.cpu_pct + ca.snapshot.mem_pct;
        let load_b = cb.snapshot.cpu_pct + cb.snapshot.mem_pct;
        load_a
          .partial_cmp(&load_b)
          .unwrap_or(std::cmp::Ordering::Equal)
          .then_with(|| ca.name.cmp(&cb.name))
      });
      eligible.into_iter().next().map(|(c, _)| c)
    }
    // Round-robin: deterministic rotation over eligible nodes ordered by name.
    PlacementMode::RoundRobin { turn } => {
      eligible.sort_by(|(ca, _), (cb, _)| ca.name.cmp(&cb.name));
      if eligible.is_empty() {
        None
      } else {
        let idx = (*turn as usize) % eligible.len();
        Some(eligible[idx].0)
      }
    }
  }
}

/// Single placement entry point: choose a node for new work from mesh load
/// telemetry.
///
/// Adapts each [`LoadReport`] into a [`NodeCandidate`], applies the policy's
/// [`PlacementMode`], and returns the chosen node's name. Pure and deterministic
/// — no clock, no Tokio, no interior mutability — so it is unit-testable without
/// a live cluster. Returns `None` when no reported node satisfies the per-node
/// policy.
pub fn place(reports: &[LoadReport], policy: &ClusterResourcePolicy) -> Option<String> {
  let candidates: Vec<NodeCandidate> = reports.iter().map(NodeCandidate::from).collect();
  pick_placement(candidates.iter(), policy).map(|c| c.name.clone())
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

  fn cluster_with(mode: PlacementMode, per: ResourcePolicy) -> ClusterResourcePolicy {
    ClusterResourcePolicy {
      global: per.clone(),
      per_node: per,
      placement: mode,
    }
  }

  #[test]
  fn load_report_adapts_to_candidate() {
    let report = LoadReport::new("n1", 0.3, 0.6);
    let candidate = NodeCandidate::from(&report);
    assert_eq!(candidate.name, "n1");
    assert!((candidate.snapshot.cpu_pct - 0.3).abs() < 0.001);
    assert!((candidate.snapshot.mem_pct - 0.6).abs() < 0.001);
  }

  #[test]
  fn place_affinity_prefers_explicit_node() {
    let cluster = cluster_with(
      PlacementMode::Affinity { node: "b".into() },
      ResourcePolicy::memory_cap_max_cpu(0.95),
    );
    let reports = [
      // `a` is the least loaded, but affinity must still pin to `b`.
      LoadReport::new("a", 0.1, 0.1),
      LoadReport::new("b", 0.5, 0.5),
    ];
    assert_eq!(place(&reports, &cluster).as_deref(), Some("b"));
  }

  #[test]
  fn place_affinity_is_none_when_target_ineligible() {
    let cluster = cluster_with(
      PlacementMode::Affinity { node: "b".into() },
      ResourcePolicy::memory_cap_max_cpu(0.85),
    );
    let reports = [
      LoadReport::new("a", 0.1, 0.1),
      // `b` breaches the 0.85 memory ceiling, so it is ineligible.
      LoadReport::new("b", 0.9, 0.9),
    ];
    assert_eq!(place(&reports, &cluster), None);
  }

  #[test]
  fn place_local_first_picks_most_headroom() {
    let cluster = cluster_with(
      PlacementMode::LocalFirst,
      ResourcePolicy::memory_cap_max_cpu(0.95),
    );
    let reports = [
      LoadReport::new("a", 0.8, 0.8),
      LoadReport::new("b", 0.2, 0.2),
      LoadReport::new("c", 0.5, 0.5),
    ];
    assert_eq!(place(&reports, &cluster).as_deref(), Some("b"));
  }

  #[test]
  fn place_spread_and_local_first_diverge() {
    let per = ResourcePolicy::memory_cap_max_cpu(0.85);
    let reports = [
      // sum 0.80, headroom min(0.90, 0.15) = 0.15
      LoadReport::new("a", 0.10, 0.70),
      // sum 0.85, headroom min(0.45, 0.55) = 0.45
      LoadReport::new("b", 0.55, 0.30),
    ];
    // Spread minimizes absolute load -> a.
    assert_eq!(
      place(&reports, &cluster_with(PlacementMode::Spread, per.clone())).as_deref(),
      Some("a")
    );
    // LocalFirst maximizes policy headroom -> b.
    assert_eq!(
      place(&reports, &cluster_with(PlacementMode::LocalFirst, per)).as_deref(),
      Some("b")
    );
  }

  #[test]
  fn place_round_robin_rotates_over_eligible() {
    let per = ResourcePolicy::memory_cap_max_cpu(0.95);
    let reports = [
      LoadReport::new("a", 0.1, 0.1),
      LoadReport::new("b", 0.2, 0.2),
      LoadReport::new("c", 0.3, 0.3),
    ];
    let picks: Vec<Option<String>> = (0u64..4)
      .map(|turn| {
        place(
          &reports,
          &cluster_with(PlacementMode::RoundRobin { turn }, per.clone()),
        )
      })
      .collect();
    assert_eq!(
      picks,
      vec![
        Some("a".to_string()),
        Some("b".to_string()),
        Some("c".to_string()),
        Some("a".to_string()),
      ]
    );
  }

  #[test]
  fn place_returns_none_when_all_saturated() {
    let cluster = ClusterResourcePolicy::local_first(ResourcePolicy::memory_cap_max_cpu(0.85));
    let reports = [
      LoadReport::new("a", 0.99, 0.99),
      LoadReport::new("b", 0.90, 0.95),
    ];
    assert_eq!(place(&reports, &cluster), None);
  }

  #[test]
  fn place_empty_reports_is_none() {
    let cluster = ClusterResourcePolicy::local_first(ResourcePolicy::memory_cap_max_cpu(0.95));
    assert_eq!(place(&[], &cluster), None);
  }
}
