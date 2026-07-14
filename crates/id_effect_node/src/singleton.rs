//! CP [`ClusterSingleton`] — at most one holder cluster-wide.
//!
//! Netsplit story (ADR 0009): unavailable without quorum. Production path
//! uses Postgres advisory-lock leases (`pg` feature via `id_effect_sql_pg`).
//! This module ships an **in-memory lease** suitable for single-process
//! tests and as the interface the PG backend implements.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use id_effect_process::Pid;

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
  fn try_acquire(&self, name: &str, holder: &str, ttl: Duration) -> Result<(), SingletonError>;

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

/// **Test-only** in-process lease table (single process, TTL-based).
///
/// Provides `SingletonLease` semantics without a database for unit tests and single-node demos.
/// It cannot coordinate across processes — use `PgAdvisoryLease` (feature `pg`) for the CP
/// production path.
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
  fn try_acquire(&self, name: &str, holder: &str, ttl: Duration) -> Result<(), SingletonError> {
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
    self
      .lease
      .try_acquire(&self.name, &self.holder_id, self.ttl)?;
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

// ---------------------------------------------------------------------------
// Postgres advisory-lock lease (feature `pg`)
// ---------------------------------------------------------------------------

/// Command sent from a [`PgAdvisoryLease`] handle to its dedicated-session worker.
#[cfg(feature = "pg")]
enum LeaseCmd {
  Acquire {
    k1: i32,
    k2: i32,
    resp: flume::Sender<Result<bool, String>>,
  },
  Unlock {
    k1: i32,
    k2: i32,
    resp: flume::Sender<Result<bool, String>>,
  },
  Ping {
    resp: flume::Sender<Result<(), String>>,
  },
}

/// Owns a single Postgres connection and runs advisory-lock SQL on it.
///
/// The lock is session-scoped: it stays held for exactly as long as this loop (and therefore the
/// connection) lives. When every [`PgAdvisoryLease`] handle drops, `commands` closes, the loop
/// ends, the connection closes, and Postgres releases all advisory locks — the failover trigger.
#[cfg(feature = "pg")]
fn run_lease_worker(
  url: String,
  commands: flume::Receiver<LeaseCmd>,
  ready: flume::Sender<Result<(), String>>,
) {
  use sqlx::Connection as _;

  let runtime = match tokio::runtime::Builder::new_current_thread()
    .enable_all()
    .build()
  {
    Ok(rt) => rt,
    Err(e) => {
      let _ = ready.send(Err(format!("tokio runtime: {e}")));
      return;
    }
  };

  runtime.block_on(async move {
    let mut conn = match sqlx::postgres::PgConnection::connect(&url).await {
      Ok(conn) => conn,
      Err(e) => {
        let _ = ready.send(Err(format!("connect: {e}")));
        return;
      }
    };
    let _ = ready.send(Ok(()));

    while let Ok(cmd) = commands.recv_async().await {
      match cmd {
        LeaseCmd::Acquire { k1, k2, resp } => {
          let outcome = sqlx::query_scalar::<_, bool>("SELECT pg_try_advisory_lock($1, $2)")
            .bind(k1)
            .bind(k2)
            .fetch_one(&mut conn)
            .await
            .map_err(|e| e.to_string());
          let _ = resp.send(outcome);
        }
        LeaseCmd::Unlock { k1, k2, resp } => {
          let outcome = sqlx::query_scalar::<_, bool>("SELECT pg_advisory_unlock($1, $2)")
            .bind(k1)
            .bind(k2)
            .fetch_one(&mut conn)
            .await
            .map_err(|e| e.to_string());
          let _ = resp.send(outcome);
        }
        LeaseCmd::Ping { resp } => {
          let outcome = sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(&mut conn)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string());
          let _ = resp.send(outcome);
        }
      }
    }
    // `conn` drops here: session ends, advisory locks released.
  });
}

/// Postgres session-level advisory-lock [`SingletonLease`] — the CP production path (ADR 0009).
///
/// Holds a **dedicated** connection on a worker thread for the lease lifetime so
/// `pg_try_advisory_lock` stays session-scoped and is auto-released on connection loss or drop,
/// enabling failover. TTLs are ignored: ownership tracks session liveness, not wall-clock time.
///
/// Each node process should hold its own lease (one connection). Enabled by the `pg` feature;
/// construct via [`PgAdvisoryLease::connect`] or [`PgAdvisoryLease::from_pool_config`].
#[cfg(feature = "pg")]
pub struct PgAdvisoryLease {
  commands: flume::Sender<LeaseCmd>,
  held: Mutex<HashMap<String, String>>,
}

#[cfg(feature = "pg")]
impl PgAdvisoryLease {
  /// Open a dedicated advisory-lock session at `url` (`postgres://…`).
  ///
  /// Blocks until the connection is established; returns [`SingletonError::Unavailable`] if the
  /// worker cannot connect.
  pub fn connect(url: impl Into<String>) -> Result<Self, SingletonError> {
    let url = url.into();
    let (commands, rx) = flume::unbounded();
    let (ready_tx, ready_rx) = flume::bounded(1);
    std::thread::Builder::new()
      .name("pg-advisory-lease".into())
      .spawn(move || run_lease_worker(url, rx, ready_tx))
      .map_err(|e| SingletonError::Unavailable(format!("spawn lease worker: {e}")))?;
    match ready_rx.recv() {
      Ok(Ok(())) => Ok(Self {
        commands,
        held: Mutex::new(HashMap::new()),
      }),
      Ok(Err(e)) => Err(SingletonError::Unavailable(e)),
      Err(_) => Err(SingletonError::Unavailable(
        "lease worker exited before ready".into(),
      )),
    }
  }

  /// Open a lease using an [`id_effect_sql_pg::PgPoolConfig`]'s connection URL.
  pub fn from_pool_config(config: id_effect_sql_pg::PgPoolConfig) -> Result<Self, SingletonError> {
    Self::connect(config.url)
  }

  fn command_bool(
    &self,
    cmd: LeaseCmd,
    rx: flume::Receiver<Result<bool, String>>,
  ) -> Result<bool, SingletonError> {
    self
      .commands
      .send(cmd)
      .map_err(|_| SingletonError::Unavailable("lease worker gone".into()))?;
    rx.recv()
      .map_err(|_| SingletonError::Unavailable("lease worker dropped response".into()))?
      .map_err(SingletonError::Unavailable)
  }

  fn acquire_lock(&self, k1: i32, k2: i32) -> Result<bool, SingletonError> {
    let (tx, rx) = flume::bounded(1);
    self.command_bool(LeaseCmd::Acquire { k1, k2, resp: tx }, rx)
  }

  fn unlock(&self, k1: i32, k2: i32) -> Result<bool, SingletonError> {
    let (tx, rx) = flume::bounded(1);
    self.command_bool(LeaseCmd::Unlock { k1, k2, resp: tx }, rx)
  }

  fn ping(&self) -> Result<(), SingletonError> {
    let (tx, rx) = flume::bounded(1);
    self
      .commands
      .send(LeaseCmd::Ping { resp: tx })
      .map_err(|_| SingletonError::Unavailable("lease worker gone".into()))?;
    rx.recv()
      .map_err(|_| SingletonError::Unavailable("lease worker dropped response".into()))?
      .map_err(SingletonError::Unavailable)
  }
}

#[cfg(feature = "pg")]
impl SingletonLease for PgAdvisoryLease {
  fn try_acquire(&self, name: &str, holder: &str, _ttl: Duration) -> Result<(), SingletonError> {
    let mut held = self
      .held
      .lock()
      .map_err(|_| SingletonError::Unavailable("lease state poisoned".into()))?;
    if let Some(existing) = held.get(name) {
      return if existing == holder {
        Ok(())
      } else {
        Err(SingletonError::HeldBy(existing.clone()))
      };
    }
    let (k1, k2) = advisory_lock_keys(name);
    if self.acquire_lock(k1, k2)? {
      held.insert(name.to_string(), holder.to_string());
      Ok(())
    } else {
      Err(SingletonError::HeldBy("another session".into()))
    }
  }

  fn renew(&self, name: &str, holder: &str, _ttl: Duration) -> Result<(), SingletonError> {
    {
      let held = self
        .held
        .lock()
        .map_err(|_| SingletonError::Unavailable("lease state poisoned".into()))?;
      match held.get(name) {
        Some(h) if h == holder => {}
        Some(h) => return Err(SingletonError::HeldBy(h.clone())),
        None => return Err(SingletonError::Unavailable("no lease".into())),
      }
    }
    // Ownership is session liveness: a failed ping means the session (and lock) is gone.
    self.ping()
  }

  fn release(&self, name: &str, holder: &str) -> Result<(), SingletonError> {
    let mut held = self
      .held
      .lock()
      .map_err(|_| SingletonError::Unavailable("lease state poisoned".into()))?;
    match held.get(name) {
      Some(h) if h == holder => {
        let (k1, k2) = advisory_lock_keys(name);
        let _ = self.unlock(k1, k2);
        held.remove(name);
        Ok(())
      }
      Some(h) => Err(SingletonError::HeldBy(h.clone())),
      None => Ok(()),
    }
  }

  fn holder(&self, name: &str) -> Option<String> {
    let owner = {
      let held = self.held.lock().ok()?;
      held.get(name)?.clone()
    };
    // Confirm the session is still alive; otherwise the lock has already been lost.
    if self.ping().is_ok() {
      Some(owner)
    } else {
      None
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use id_effect_process::NodeName;

  #[test]
  fn only_one_holder() {
    let lease = Arc::new(MemoryLease::new());
    let a = ClusterSingleton::new(
      "leader",
      "a",
      Arc::clone(&lease) as _,
      Duration::from_secs(5),
    );
    let b = ClusterSingleton::new(
      "leader",
      "b",
      Arc::clone(&lease) as _,
      Duration::from_secs(5),
    );
    let pid = Pid::from_parts(NodeName::new("a@t"), 1, 1);
    a.try_become(&pid).unwrap();
    assert!(a.is_holder());
    assert!(matches!(b.try_become(&pid), Err(SingletonError::HeldBy(_))));
    a.resign().unwrap();
    b.try_become(&pid).unwrap();
    assert!(b.is_holder());
  }
}
