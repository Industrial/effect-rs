//! [`DistributedSupervisor`] — Horde-style takeover on NodeDown.
//!
//! Children are placed on mesh nodes; when a hosting node departs,
//! surviving supervisors re-spawn the child via the behavior registry
//! (ADR 0009 § distributed supervision).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use id_effect::{ClusterResourcePolicy, LoadReport, PlacementMode, ResourcePolicy};
use id_effect_process::Pid;

use crate::envelope::RemoteFault;
use crate::membership::MembershipEvent;
use crate::mesh::Mesh;

/// How long to wait for a remote spawn during placement/takeover.
const SPAWN_TIMEOUT: Duration = Duration::from_secs(5);

/// A child managed across the cluster.
#[derive(Clone, Debug)]
pub struct DistChildSpec {
  /// Stable child id within the supervisor.
  pub id: String,
  /// Registered behavior name.
  pub behavior: String,
  /// Postcard-encoded init args.
  pub args: Vec<u8>,
  /// Preferred node name; `None` = any healthy member.
  pub prefer: Option<String>,
}

#[derive(Clone, Debug)]
struct Running {
  spec: DistChildSpec,
  pid: Pid,
  node: String,
}

/// Distributed supervisor state for one logical tree node.
pub struct DistributedSupervisor {
  mesh: Arc<Mesh>,
  policy: ClusterResourcePolicy,
  children: Mutex<HashMap<String, Running>>,
}

impl DistributedSupervisor {
  /// Bind to a mesh with a permissive local-first placement policy.
  ///
  /// The default admits any node below 100% CPU and memory and prefers the
  /// least-loaded one; override with [`DistributedSupervisor::with_policy`] to
  /// enforce tighter caps or a different [`PlacementMode`].
  pub fn new(mesh: Arc<Mesh>) -> Self {
    Self::with_policy(
      mesh,
      ClusterResourcePolicy::local_first(ResourcePolicy::memory_cap_max_cpu(1.0)),
    )
  }

  /// Bind to a mesh with an explicit cluster placement policy.
  pub fn with_policy(mesh: Arc<Mesh>, policy: ClusterResourcePolicy) -> Self {
    Self {
      mesh,
      policy,
      children: Mutex::new(HashMap::new()),
    }
  }

  /// Start (or restart) a child.
  ///
  /// With `node_name` set, the child is pinned to that node. Otherwise the
  /// target is chosen by the unified placement API ([`id_effect::place`]) from
  /// the mesh's load telemetry and this supervisor's policy — a `prefer` hint
  /// becomes an [`PlacementMode::Affinity`] bias, and the call falls back to
  /// `prefer` then the local node when no telemetry is available.
  pub fn start_child(
    &self,
    spec: DistChildSpec,
    node_name: Option<&str>,
  ) -> Result<Pid, RemoteFault> {
    match node_name {
      Some(node) => {
        let pid =
          self
            .mesh
            .remote_spawn(node, &spec.behavior, spec.args.clone(), None, SPAWN_TIMEOUT)?;
        self.insert_running(spec, pid.clone());
        Ok(pid)
      }
      None => self.place_and_run(spec, None),
    }
  }

  /// Resolve a target via placement (excluding `exclude`, e.g. a downed node)
  /// and spawn, falling back to `prefer`/local when placement yields nothing.
  fn place_and_run(&self, spec: DistChildSpec, exclude: Option<&str>) -> Result<Pid, RemoteFault> {
    let reports: Vec<LoadReport> = self
      .mesh
      .load_reports()
      .into_iter()
      .filter(|r| Some(r.node.as_str()) != exclude)
      .collect();

    // A live `prefer` hint biases placement toward that node; a `prefer` equal to
    // the excluded (downed) node is ignored so takeover never targets it.
    let mut policy = self.policy.clone();
    if let Some(pref) = &spec.prefer
      && Some(pref.as_str()) != exclude
    {
      policy.placement = PlacementMode::Affinity { node: pref.clone() };
    }

    let pid = match self.mesh.remote_spawn_placed(
      &reports,
      &policy,
      &spec.behavior,
      spec.args.clone(),
      None,
      SPAWN_TIMEOUT,
    ) {
      Ok(pid) => pid,
      Err(_) => {
        let target = spec
          .prefer
          .clone()
          .filter(|p| Some(p.as_str()) != exclude)
          .unwrap_or_else(|| self.mesh.node_name().to_string());
        self.mesh.remote_spawn(
          &target,
          &spec.behavior,
          spec.args.clone(),
          None,
          SPAWN_TIMEOUT,
        )?
      }
    };
    self.insert_running(spec, pid.clone());
    Ok(pid)
  }

  /// Record a running child keyed by spec id; node is taken from the live pid.
  fn insert_running(&self, spec: DistChildSpec, pid: Pid) {
    let node = pid.node().as_str().to_string();
    self
      .children
      .lock()
      .expect("children")
      .insert(spec.id.clone(), Running { spec, pid, node });
  }

  /// React to membership: re-place children whose host went down.
  pub fn on_membership(&self, event: &MembershipEvent) {
    let MembershipEvent::NodeDown(member) = event else {
      return;
    };
    let down = member.id.name().to_string();
    let survivors: Vec<DistChildSpec> = {
      let mut kids = self.children.lock().expect("children");
      let doomed: Vec<String> = kids
        .iter()
        .filter(|(_, r)| r.node == down || r.pid.node().as_str() == down)
        .map(|(id, _)| id.clone())
        .collect();
      doomed
        .into_iter()
        .filter_map(|id| kids.remove(&id).map(|r| r.spec))
        .collect()
    };
    for spec in survivors {
      // Takeover: re-place over survivors, excluding the downed node so the
      // child never lands back on it (falls back to the local node).
      let _ = self.place_and_run(spec, Some(&down));
    }
  }

  /// Running child count.
  pub fn len(&self) -> usize {
    self.children.lock().expect("children").len()
  }

  /// Whether any children are managed.
  pub fn is_empty(&self) -> bool {
    self.children.lock().expect("children").is_empty()
  }

  /// Snapshot of (child id, pid, node).
  pub fn list(&self) -> Vec<(String, Pid, String)> {
    self
      .children
      .lock()
      .expect("children")
      .iter()
      .map(|(id, r)| (id.clone(), r.pid.clone(), r.node.clone()))
      .collect()
  }
}
