//! Peer discovery strategies (libcluster's shape).
//!
//! Discovery answers one question: *which addresses should I announce to
//! when joining?* Three strategies per ADR 0009: a static list, DNS
//! resolution (Kubernetes headless services), and “gossip seed” — any
//! member already known from a previous epoch.

use async_trait::async_trait;

/// Strategy for finding join candidates.
#[async_trait]
pub trait Discovery: Send + Sync {
  /// Addresses to announce to, in preference order. An empty result
  /// means “I am the first node” — the caller forms a new cluster.
  async fn candidates(&self) -> Vec<String>;
}

/// Fixed peer list (config-file clusters, tests).
pub struct StaticDiscovery {
  peers: Vec<String>,
}

impl StaticDiscovery {
  /// Discovery from a fixed address list.
  pub fn new(peers: impl IntoIterator<Item = impl Into<String>>) -> Self {
    Self {
      peers: peers.into_iter().map(Into::into).collect(),
    }
  }
}

#[async_trait]
impl Discovery for StaticDiscovery {
  async fn candidates(&self) -> Vec<String> {
    self.peers.clone()
  }
}

/// DNS-based discovery: resolve one hostname to many peers (Kubernetes
/// headless services, SRV-less round-robin DNS).
pub struct DnsDiscovery {
  /// `host:port` — the host part is resolved, the port is applied to
  /// every resolved address.
  query: String,
}

impl DnsDiscovery {
  /// Discovery resolving `host:port`.
  pub fn new(query: impl Into<String>) -> Self {
    Self {
      query: query.into(),
    }
  }
}

#[async_trait]
impl Discovery for DnsDiscovery {
  async fn candidates(&self) -> Vec<String> {
    match tokio::net::lookup_host(&self.query).await {
      Ok(addrs) => addrs.map(|a| a.to_string()).collect(),
      Err(_) => Vec::new(),
    }
  }
}

/// Chain multiple strategies: first non-empty answer wins.
pub struct ChainedDiscovery {
  strategies: Vec<Box<dyn Discovery>>,
}

impl ChainedDiscovery {
  /// Chain in priority order.
  pub fn new(strategies: Vec<Box<dyn Discovery>>) -> Self {
    Self { strategies }
  }
}

#[async_trait]
impl Discovery for ChainedDiscovery {
  async fn candidates(&self) -> Vec<String> {
    for strategy in &self.strategies {
      let found = strategy.candidates().await;
      if !found.is_empty() {
        return found;
      }
    }
    Vec::new()
  }
}
