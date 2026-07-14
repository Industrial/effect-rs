//! Live hardware feedback for Compute Fabric.

use std::sync::{Arc, Mutex};

use super::policy::ResourcePolicy;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TelemetrySnapshot {
  pub cpu_pct: f32,
  pub mem_pct: f32,
}

impl TelemetrySnapshot {
  pub fn satisfies(&self, policy: &ResourcePolicy) -> bool {
    policy.cpu.mode.is_within(self.cpu_pct) && policy.memory.mode.is_within(self.mem_pct)
  }

  pub fn min_headroom(&self, policy: &ResourcePolicy) -> f32 {
    policy
      .cpu
      .mode
      .headroom(self.cpu_pct)
      .min(policy.memory.mode.headroom(self.mem_pct))
  }
}

pub trait TelemetryEngine: Send + Sync {
  fn snapshot(&self) -> TelemetrySnapshot;
}

#[derive(Debug, Default)]
pub struct SysinfoTelemetry;

impl TelemetryEngine for SysinfoTelemetry {
  fn snapshot(&self) -> TelemetrySnapshot {
    use sysinfo::{MemoryRefreshKind, RefreshKind, System};

    let mut sys = System::new_with_specifics(
      RefreshKind::nothing()
        .with_memory(MemoryRefreshKind::everything())
        .with_cpu(sysinfo::CpuRefreshKind::everything()),
    );
    sys.refresh_memory();
    sys.refresh_cpu_usage();
    std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
    sys.refresh_cpu_usage();

    let mem_pct = if sys.total_memory() > 0 {
      sys.used_memory() as f32 / sys.total_memory() as f32
    } else {
      0.0
    };
    let cpus = sys.cpus();
    let cpu_pct = if cpus.is_empty() {
      0.0
    } else {
      cpus.iter().map(|c| c.cpu_usage()).sum::<f32>() / (cpus.len() as f32 * 100.0)
    };

    TelemetrySnapshot { cpu_pct, mem_pct }
  }
}

#[derive(Debug)]
pub struct MockTelemetry {
  inner: Mutex<TelemetrySnapshot>,
}

impl MockTelemetry {
  pub fn new(cpu_pct: f32, mem_pct: f32) -> Arc<Self> {
    Arc::new(Self {
      inner: Mutex::new(TelemetrySnapshot { cpu_pct, mem_pct }),
    })
  }

  pub fn set(&self, cpu_pct: f32, mem_pct: f32) {
    *self.inner.lock().expect("mock telemetry lock") = TelemetrySnapshot { cpu_pct, mem_pct };
  }
}

impl TelemetryEngine for MockTelemetry {
  fn snapshot(&self) -> TelemetrySnapshot {
    *self.inner.lock().expect("mock telemetry lock")
  }
}

/// Cluster-wide telemetry table (gossip-piggybacked snapshots).
#[derive(Debug, Default)]
pub struct ClusterTelemetry {
  nodes: Mutex<std::collections::HashMap<String, TelemetrySnapshot>>,
}

impl ClusterTelemetry {
  /// Empty peer table.
  pub fn new() -> Self {
    Self::default()
  }

  /// Merge gossiped remote snapshots; last writer for same node wins.
  pub fn ingest(&self, remote: &[(String, TelemetrySnapshot)]) {
    let mut map = self.nodes.lock().expect("cluster tel");
    for (name, snap) in remote {
      map.insert(name.clone(), *snap);
    }
  }

  /// Build placement candidates from known peers plus local.
  pub fn candidates(
    &self,
    local_name: &str,
    local: TelemetrySnapshot,
  ) -> Vec<crate::compute::NodeCandidate> {
    let mut out = vec![crate::compute::NodeCandidate {
      name: local_name.to_string(),
      snapshot: local,
    }];
    let map = self.nodes.lock().expect("cluster tel");
    for (name, snap) in map.iter() {
      if name != local_name {
        out.push(crate::compute::NodeCandidate {
          name: name.clone(),
          snapshot: *snap,
        });
      }
    }
    out
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::compute::policy::{MetricMode, MetricPolicy, RebalanceStrategy};

  #[test]
  fn mock_telemetry_returns_configured_values() {
    let engine = MockTelemetry::new(0.4, 0.61);
    let snap = engine.snapshot();
    assert!((snap.cpu_pct - 0.4).abs() < 0.001);
    assert!((snap.mem_pct - 0.61).abs() < 0.001);
  }

  #[test]
  fn snapshot_satisfies_policy_under_ceiling() {
    let snap = TelemetrySnapshot {
      cpu_pct: 0.4,
      mem_pct: 0.61,
    };
    let policy = crate::compute::policy::ResourcePolicy {
      memory: MetricPolicy::new(MetricMode::Max { ceiling: 0.85 }),
      cpu: MetricPolicy::new(MetricMode::Max { ceiling: 1.0 }),
      rebalance: RebalanceStrategy::ThrottleAdmission,
    };
    assert!(snap.satisfies(&policy));
    assert!(snap.min_headroom(&policy) > 0.2);
  }

  #[test]
  fn snapshot_rejects_breach_and_mock_set_updates() {
    let engine = MockTelemetry::new(0.4, 0.61);
    engine.set(0.95, 0.95);
    let snap = engine.snapshot();
    let policy = crate::compute::policy::ResourcePolicy::memory_cap_max_cpu(0.85);
    assert!(!snap.satisfies(&policy));
    assert_eq!(snap.min_headroom(&policy), 0.0);
  }
}
