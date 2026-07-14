/// Derive `(key1, key2)` for `pg_try_advisory_lock(key1, key2)` from a singleton name.
///
/// Production `SingletonLease` backends run:
/// `SELECT pg_try_advisory_lock($1, $2)` / `pg_advisory_unlock` with these keys
/// against `id_effect_sql_pg` pools (ADR 0009 CP path).
pub fn advisory_lock_keys(name: &str) -> (i32, i32) {
  use std::hash::{Hash, Hasher};
  let mut h = std::collections::hash_map::DefaultHasher::new();
  name.hash(&mut h);
  let v = h.finish();
  ((v >> 32) as i32, v as i32)
}

/// CP [`ClusterSingleton`] — at most one holder cluster-wide.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use id_effect_process::Pid;

/// Why a singleton acquire failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SingletonError {
  /// Another holder currently owns the lease.
  HeldBy(String),
  /// Lease storage unavailable (no PG quorum / pool down).
  Unavailable(String),
}

impl std::fmt::Display for SingletonError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      SingletonError::HeldBy(id) => write!(f, "held by {id}"),
      SingletonError::Unavailable(e) => write!(f, "unavailable: {e}"),
    }
  }
}

impl std::error::Error for SingletonError {}

/// Abstract lease backend.
pub trait SingletonLease: Send + Sync {
  /// Try to acquire `name` for `holder` until `ttl` elapses.
  fn try_acquire(
    &self,
    name: &str,
    holder: &str,
    ttl: Duration,
  ) -> Result<(), SingletonError>;

  /// Renew an existing lease (must already be held by `holder`).
  fn renew(&self, name: &str, holder: &str, ttl: Duration) -> Result<(), SingletonError>;

  /// Release if (and only if) `holder` owns it.
  fn release(&self, name: &str, holder: &str) -> Result<(), SingletonError>;

  /// Current holder id, if any and not expired.
  fn holder(&self, name: &str) -> Option<String>;
}

struct MemEntry {
  holder: String,
  until: Instant,
}

/// In-process lease table (tests / single-node).
#[derive(Clone, Default)]
pub struct MemoryLease {
  inner: Arc<Mutex<HashMap<String, MemEntry>>>,
}

impl MemoryLease {
  /// Empty table.
  pub fn new() -> Self {
    Self::default()
  }

  fn sweep(map: &mut HashMap<String, MemEntry>) {
    let now = Instant::now();
    map.retain(|_, e| e.until > now);
  }
}

impl SingletonLease for MemoryLease {
  fn try_acquire(
    &self,
    name: &str,
    holder: &str,
    ttl: Duration,
  ) -> Result<(), SingletonError> {
    let mut map = self.inner.lock().expect("lease");
    Self::sweep(&mut map);
    match map.get(name) {
      Some(e) if e.holder != holder => Err(SingletonError::HeldBy(e.holder.clone())),
      _ => {
        map.insert(
          name.to_string(),
          MemEntry {
            holder: holder.to_string(),
            until: Instant::now() + ttl,
          },
        );
        Ok(())
      }
    }
  }

  fn renew(&self, name: &str, holder: &str, ttl: Duration) -> Result<(), SingletonError> {
    let mut map = self.inner.lock().expect("lease");
    Self::sweep(&mut map);
    match map.get_mut(name) {
      Some(e) if e.holder == holder => {
        e.until = Instant::now() + ttl;
        Ok(())
      }
      Some(e) => Err(SingletonError::HeldBy(e.holder.clone())),
      None => Err(SingletonError::Unavailable("no lease".into())),
    }
  }

  fn release(&self, name: &str, holder: &str) -> Result<(), SingletonError> {
    let mut map = self.inner.lock().expect("lease");
    match map.get(name) {
      Some(e) if e.holder == holder => {
        map.remove(name);
        Ok(())
      }
      Some(e) => Err(SingletonError::HeldBy(e.holder.clone())),
      None => Ok(()),
    }
  }

  fn holder(&self, name: &str) -> Option<String> {
    let mut map = self.inner.lock().expect("lease");
    Self::sweep(&mut map);
    map.get(name).map(|e| e.holder.clone())
  }
}

/// Cluster singleton handle: lease + optional local pid for the holder.
pub struct ClusterSingleton {
  name: String,
  holder_id: String,
  lease: Arc<dyn SingletonLease>,
  ttl: Duration,
  local_pid: Mutex<Option<Pid>>,
}

impl ClusterSingleton {
  /// Bind to a lease backend.
  pub fn new(
    name: impl Into<String>,
    holder_id: impl Into<String>,
    lease: Arc<dyn SingletonLease>,
    ttl: Duration,
  ) -> Self {
    Self {
      name: name.into(),
      holder_id: holder_id.into(),
      lease,
      ttl,
      local_pid: Mutex::new(None),
    }
  }

  /// Attempt to become the singleton; records `pid` on success.
  pub fn try_become(&self, pid: &Pid) -> Result<(), SingletonError> {
    self.lease.try_acquire(&self.name, &self.holder_id, self.ttl)?;
    *self.local_pid.lock().expect("pid") = Some(pid.clone());
    Ok(())
  }

  /// Renew the lease (call from a heartbeat fiber).
  pub fn heartbeat(&self) -> Result<(), SingletonError> {
    self.lease.renew(&self.name, &self.holder_id, self.ttl)
  }

  /// Step down.
  pub fn resign(&self) -> Result<(), SingletonError> {
    self.lease.release(&self.name, &self.holder_id)?;
    *self.local_pid.lock().expect("pid") = None;
    Ok(())
  }

  /// Whether this node currently holds the singleton.
  pub fn is_holder(&self) -> bool {
    self.lease.holder(&self.name).as_deref() == Some(self.holder_id.as_str())
  }

  /// Local process running the singleton role, if we are the holder.
  pub fn local_pid(&self) -> Option<Pid> {
    self.local_pid.lock().expect("pid").clone()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use id_effect_process::NodeName;

  #[test]
  fn only_one_holder() {
    let lease = Arc::new(MemoryLease::new());
    let a = ClusterSingleton::new("leader", "a", Arc::clone(&lease) as _, Duration::from_secs(5));
    let b = ClusterSingleton::new("leader", "b", Arc::clone(&lease) as _, Duration::from_secs(5));
    let pid = Pid::from_parts(NodeName::new("a@t"), 1, 1);
    a.try_become(&pid).unwrap();
    assert!(a.is_holder());
    assert!(matches!(b.try_become(&pid), Err(SingletonError::HeldBy(_))));
    a.resign().unwrap();
    b.try_become(&pid).unwrap();
    assert!(b.is_holder());
  }
}
