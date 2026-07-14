//! [`DistributedSupervisor`] — Horde-style takeover on NodeDown.
//!
//! Children are placed on mesh nodes; when a hosting node departs,
//! surviving supervisors re-spawn the child via the behavior registry
//! (ADR 0009 § distributed supervision).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use id_effect_process::Pid;

use crate::envelope::RemoteFault;
use crate::membership::MembershipEvent;
use crate::mesh::Mesh;

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
  children: Mutex<HashMap<String, Running>>,
}

impl DistributedSupervisor {
  /// Bind to a mesh.
  pub fn new(mesh: Arc<Mesh>) -> Self {
    Self {
      mesh,
      children: Mutex::new(HashMap::new()),
    }
  }

  /// Start (or restart) a child on `node_name` (or local if none).
  pub fn start_child(
    &self,
    spec: DistChildSpec,
    node_name: Option<&str>,
  ) -> Result<Pid, RemoteFault> {
    let target = node_name
      .map(|s| s.to_string())
      .or_else(|| spec.prefer.clone())
      .unwrap_or_else(|| self.mesh.node_name().to_string());
    let pid = self.mesh.remote_spawn(
      &target,
      &spec.behavior,
      spec.args.clone(),
      None,
      std::time::Duration::from_secs(5),
    )?;
    self.children.lock().expect("children").insert(
      spec.id.clone(),
      Running {
        spec,
        pid: pid.clone(),
        node: target,
      },
    );
    Ok(pid)
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
      // Takeover on self (Horde-style: any survivor can claim).
      let _ = self.start_child(spec, Some(self.mesh.node_name()));
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
