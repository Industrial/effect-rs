//! [`InMemoryTransport`] — deterministic in-process transport with
//! scripted partitions.
//!
//! All nodes in a test share one [`MemoryNetwork`]. Each node's transport
//! carries a **local identity** (its listen address), so partitions can be
//! scripted between node addresses and affect every connection between the
//! two nodes — dialed or accepted. Partitions are imposed and healed
//! synchronously, making netsplit tests fully deterministic (ADR 0009
//! § verification).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use bytes::Bytes;

use super::{Connection, Listener, Transport, TransportError};

/// One half of a duplex in-memory pipe.
struct Pipe {
  tx: flume::Sender<Bytes>,
  rx: flume::Receiver<Bytes>,
  /// Node addresses of both endpoints, used for partition checks on every
  /// frame.
  local: String,
  remote: String,
  net: Arc<NetworkInner>,
}

#[async_trait::async_trait]
impl Connection for Pipe {
  async fn send(&self, frame: Bytes) -> Result<(), TransportError> {
    if self.net.is_partitioned(&self.local, &self.remote) {
      return Err(TransportError::Closed);
    }
    self
      .tx
      .send_async(frame)
      .await
      .map_err(|_| TransportError::Closed)
  }

  async fn recv(&self) -> Result<Bytes, TransportError> {
    // Poll both the queue and the partition state: a partition must also
    // fail receivers that are already parked waiting.
    loop {
      if self.net.is_partitioned(&self.local, &self.remote) {
        return Err(TransportError::Closed);
      }
      match tokio::time::timeout(std::time::Duration::from_millis(10), self.rx.recv_async()).await {
        Ok(Ok(frame)) => return Ok(frame),
        Ok(Err(_)) => return Err(TransportError::Closed),
        Err(_elapsed) => continue,
      }
    }
  }

  async fn close(&self) {
    // Both flume halves disconnect when this Pipe drops; nothing to do
    // eagerly — peers observe `Closed` on their next operation.
  }

  fn peer_addr(&self) -> String {
    self.remote.clone()
  }
}

struct MemoryListener {
  addr: String,
  inbound: flume::Receiver<Box<dyn Connection>>,
}

#[async_trait::async_trait]
impl Listener for MemoryListener {
  async fn accept(&self) -> Result<Box<dyn Connection>, TransportError> {
    self
      .inbound
      .recv_async()
      .await
      .map_err(|_| TransportError::Closed)
  }

  fn local_addr(&self) -> String {
    self.addr.clone()
  }
}

struct NetworkInner {
  listeners: Mutex<HashMap<String, flume::Sender<Box<dyn Connection>>>>,
  /// Unordered pairs of currently partitioned node addresses.
  partitions: Mutex<HashSet<(String, String)>>,
}

impl NetworkInner {
  fn key(a: &str, b: &str) -> (String, String) {
    if a <= b {
      (a.to_string(), b.to_string())
    } else {
      (b.to_string(), a.to_string())
    }
  }

  fn is_partitioned(&self, a: &str, b: &str) -> bool {
    self
      .partitions
      .lock()
      .expect("partitions mutex poisoned")
      .contains(&Self::key(a, b))
  }
}

/// Shared in-memory “network”: create one per test, hand a
/// [`InMemoryTransport`] to each simulated node via
/// [`MemoryNetwork::transport`].
#[derive(Clone)]
pub struct MemoryNetwork {
  inner: Arc<NetworkInner>,
}

impl Default for MemoryNetwork {
  fn default() -> Self {
    Self::new()
  }
}

impl MemoryNetwork {
  /// Fresh, fully connected network.
  pub fn new() -> Self {
    Self {
      inner: Arc::new(NetworkInner {
        listeners: Mutex::new(HashMap::new()),
        partitions: Mutex::new(HashSet::new()),
      }),
    }
  }

  /// Transport endpoint for the simulated node whose address is `local`.
  /// Use the same string the node passes to `listen`.
  pub fn transport(&self, local: impl Into<String>) -> InMemoryTransport {
    InMemoryTransport {
      local: local.into(),
      net: Arc::clone(&self.inner),
    }
  }

  /// Cut the link between two node addresses (both directions). Existing
  /// connections observe [`TransportError::Closed`]; new dials fail.
  pub fn partition(&self, a: &str, b: &str) {
    self
      .inner
      .partitions
      .lock()
      .expect("partitions mutex poisoned")
      .insert(NetworkInner::key(a, b));
  }

  /// Heal the link between two node addresses. Connections killed by the
  /// partition stay dead (like real TCP); new dials succeed again.
  pub fn heal(&self, a: &str, b: &str) {
    self
      .inner
      .partitions
      .lock()
      .expect("partitions mutex poisoned")
      .remove(&NetworkInner::key(a, b));
  }
}

/// The per-node endpoint of a [`MemoryNetwork`].
#[derive(Clone)]
pub struct InMemoryTransport {
  local: String,
  net: Arc<NetworkInner>,
}

#[async_trait::async_trait]
impl Transport for InMemoryTransport {
  async fn connect(&self, addr: &str) -> Result<Box<dyn Connection>, TransportError> {
    if self.net.is_partitioned(&self.local, addr) {
      return Err(TransportError::Connect("partitioned".to_string()));
    }
    let sender = {
      let listeners = self.net.listeners.lock().expect("listeners mutex poisoned");
      listeners.get(addr).cloned()
    };
    let Some(sender) = sender else {
      return Err(TransportError::Connect(format!("no listener at {addr}")));
    };

    let (a_tx, a_rx) = flume::unbounded::<Bytes>();
    let (b_tx, b_rx) = flume::unbounded::<Bytes>();

    let dialer: Box<dyn Connection> = Box::new(Pipe {
      tx: a_tx,
      rx: b_rx,
      local: self.local.clone(),
      remote: addr.to_string(),
      net: Arc::clone(&self.net),
    });
    let accepted: Box<dyn Connection> = Box::new(Pipe {
      tx: b_tx,
      rx: a_rx,
      local: addr.to_string(),
      remote: self.local.clone(),
      net: Arc::clone(&self.net),
    });

    sender
      .send_async(accepted)
      .await
      .map_err(|_| TransportError::Connect("listener dropped".to_string()))?;
    Ok(dialer)
  }

  async fn listen(&self, addr: &str) -> Result<Box<dyn Listener>, TransportError> {
    let (tx, rx) = flume::unbounded();
    let mut listeners = self.net.listeners.lock().expect("listeners mutex poisoned");
    if listeners.contains_key(addr) {
      return Err(TransportError::Bind(format!("{addr} already bound")));
    }
    listeners.insert(addr.to_string(), tx);
    Ok(Box::new(MemoryListener {
      addr: addr.to_string(),
      inbound: rx,
    }))
  }
}
