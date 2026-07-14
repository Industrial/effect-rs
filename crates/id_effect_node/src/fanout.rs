//! Cast postcard payloads to every member of a process group (stream fanout).

use std::sync::Arc;

use id_effect_process::Pid;

use crate::envelope::RemoteFault;
use crate::groups::ProcessGroups;
use crate::mesh::Mesh;

/// Fire-and-forget cast to every pid currently in `group`.
///
/// Partial failure: returns the first error after attempting all members;
/// at-most-once per member (ADR 0009).
pub fn fanout_cast(
  mesh: &Mesh,
  groups: &ProcessGroups,
  group: &str,
  msg: &[u8],
) -> Result<usize, RemoteFault> {
  let members = groups.members(group);
  let mut ok = 0usize;
  let mut err: Option<RemoteFault> = None;
  for pid in &members {
    match mesh.remote_cast(pid, msg.to_vec()) {
      Ok(()) => ok += 1,
      Err(e) if err.is_none() => err = Some(e),
      Err(_) => {}
    }
  }
  match err {
    Some(e) if ok == 0 => Err(e),
    _ => Ok(ok),
  }
}

/// Cast only to pids whose node matches `prefer` (affinity dispatch).
pub fn fanout_cast_on_node(
  mesh: &Mesh,
  groups: &ProcessGroups,
  group: &str,
  prefer: &str,
  msg: &[u8],
) -> Result<usize, RemoteFault> {
  let members: Vec<Pid> = groups
    .members(group)
    .into_iter()
    .filter(|p| p.node().as_str() == prefer)
    .collect();
  let mut ok = 0usize;
  for pid in &members {
    mesh.remote_cast(pid, msg.to_vec())?;
    ok += 1;
  }
  Ok(ok)
}

/// Shared group table for a mesh node (tests / control plane).
pub type SharedGroups = Arc<std::sync::Mutex<ProcessGroups>>;
