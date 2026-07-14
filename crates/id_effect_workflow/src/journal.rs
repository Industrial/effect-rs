//! Pluggable step journal backends — SQLite today, transport-backed journal for clusters.
//!
//! [`DurableWorkflowLog`] remains the default SQLite implementation. [`StepJournal`] abstracts
//! the `(workflow_id, seq)` contract so a Postgres or RPC-backed journal can swap in without
//! changing FSM or saga call sites.
//!
//! [`RemoteStepJournal`] is the real distributed implementor: it speaks a serialized
//! request/response protocol over a [`JournalTransport`] to a [`JournalServer`]. The transport
//! moves opaque byte frames, so the same client works over an in-process [`LoopbackTransport`]
//! (tests, single-node embedding) or a production `id_effect_rpc` / mesh transport that ferries
//! the frames between hosts. This is the client/server seam distributed workflows resume across.
//!
//! [`NetworkJournalStub`] is retained only as a lightweight test double (a shared in-memory map,
//! no serialization boundary); reach for [`RemoteStepJournal`] for anything cluster-facing.

use crate::error::WorkflowError;

#[cfg(feature = "memory")]
use crate::DurableWorkflowLog;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::collections::HashMap;
use std::fmt::Debug;
#[cfg(feature = "memory")]
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Minimal contract for durable workflow step storage.
pub trait StepJournal: Send {
  /// Registers a workflow id (idempotent error if duplicate).
  fn register_workflow(&mut self, id: &str) -> Result<(), WorkflowError>;

  /// Whether `id` was registered.
  fn has_workflow(&self, id: &str) -> Result<bool, WorkflowError>;

  /// Persisted JSON for a completed step, if any.
  fn completed_json(&self, workflow_id: &str, seq: u32) -> Result<Option<String>, WorkflowError>;

  /// Runs or resumes a typed step (cache hit skips `compute`).
  fn run_step_typed<T, F>(
    &mut self,
    workflow_id: &str,
    seq: u32,
    step_name: &str,
    compute: F,
  ) -> Result<T, WorkflowError>
  where
    T: Serialize + DeserializeOwned,
    F: FnOnce() -> Result<T, WorkflowError>;

  /// Count of completed steps for a workflow.
  fn completed_step_count(&self, workflow_id: &str) -> Result<u32, WorkflowError>;
}

/// Durable distributed step execution across Compute Fabric cluster workers.
///
/// When long-running effect steps are offloaded (e.g. by
/// [`id_effect::compute::ComputeSupervisor::scale_out`]), the executing node records the
/// placement here so a resuming node can attribute the step. Implemented by
/// [`RemoteStepJournal`] over a [`JournalTransport`].
pub trait DistributedStepJournal: StepJournal {
  /// Records that step `seq` executed on cluster node `node_id`.
  fn record_distributed_step(
    &mut self,
    workflow_id: &str,
    seq: u32,
    node_id: &str,
  ) -> Result<(), WorkflowError>;
}

#[cfg(feature = "memory")]
impl StepJournal for DurableWorkflowLog {
  fn register_workflow(&mut self, id: &str) -> Result<(), WorkflowError> {
    DurableWorkflowLog::register_workflow(self, id)
  }

  fn has_workflow(&self, id: &str) -> Result<bool, WorkflowError> {
    DurableWorkflowLog::has_workflow(self, id)
  }

  fn completed_json(&self, workflow_id: &str, seq: u32) -> Result<Option<String>, WorkflowError> {
    DurableWorkflowLog::completed_json(self, workflow_id, seq)
  }

  fn run_step_typed<T, F>(
    &mut self,
    workflow_id: &str,
    seq: u32,
    step_name: &str,
    compute: F,
  ) -> Result<T, WorkflowError>
  where
    T: Serialize + DeserializeOwned,
    F: FnOnce() -> Result<T, WorkflowError>,
  {
    DurableWorkflowLog::run_step_typed(self, workflow_id, seq, step_name, compute)
  }

  fn completed_step_count(&self, workflow_id: &str) -> Result<u32, WorkflowError> {
    DurableWorkflowLog::completed_step_count(self, workflow_id)
  }
}

/// Endpoint/transport configuration for a [`RemoteStepJournal`].
///
/// The `endpoint` is the address a production [`JournalTransport`] (rpc/mesh) dials; the
/// in-process [`LoopbackTransport`] ignores it and dispatches straight to a [`JournalServer`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistributedJournalConfig {
  /// Logical journal service name (e.g. `workflow-journal.default.svc`).
  pub endpoint: String,
  /// TLS required for remote append.
  pub require_tls: bool,
}

impl Default for DistributedJournalConfig {
  fn default() -> Self {
    Self {
      endpoint: "http://127.0.0.1:8090".to_string(),
      require_tls: false,
    }
  }
}

/// **Test double only.** An in-memory journal backed by a shared map — no serialization
/// boundary, no client/server split.
///
/// It proves the [`StepJournal`] trait is sufficient for FSM resume without SQLite, but it does
/// **not** exercise a real transport. For distributed workflows use [`RemoteStepJournal`], which
/// speaks a serialized protocol over a [`JournalTransport`] to a [`JournalServer`].
#[derive(Debug, Default)]
pub struct NetworkJournalStub {
  config: DistributedJournalConfig,
  inner: Arc<Mutex<HashMap<String, HashMap<u32, String>>>>,
  registered: Arc<Mutex<HashMap<String, ()>>>,
}

impl NetworkJournalStub {
  /// Creates a stub with default local endpoint metadata.
  #[inline]
  pub fn new() -> Self {
    Self::with_config(DistributedJournalConfig::default())
  }

  /// Creates a stub tagged with the endpoint that a future RPC client would call.
  #[inline]
  pub fn with_config(config: DistributedJournalConfig) -> Self {
    Self {
      config,
      inner: Arc::new(Mutex::new(HashMap::new())),
      registered: Arc::new(Mutex::new(HashMap::new())),
    }
  }

  /// Endpoint recorded for observability / config tests.
  #[inline]
  pub fn endpoint(&self) -> &str {
    &self.config.endpoint
  }

  /// Opens a SQLite-backed journal at `path` for side-by-side comparison in spikes.
  #[cfg(feature = "memory")]
  #[inline]
  pub fn open_sqlite(path: &Path) -> Result<DurableWorkflowLog, WorkflowError> {
    DurableWorkflowLog::open(path)
  }
}

impl StepJournal for NetworkJournalStub {
  fn register_workflow(&mut self, id: &str) -> Result<(), WorkflowError> {
    if id.trim().is_empty() {
      return Err(WorkflowError::InvalidWorkflowId);
    }
    let mut reg = self
      .registered
      .lock()
      .map_err(|_| WorkflowError::InvalidWorkflowId)?;
    if reg.contains_key(id) {
      return Err(WorkflowError::WorkflowAlreadyExists(id.to_string()));
    }
    reg.insert(id.to_string(), ());
    Ok(())
  }

  fn has_workflow(&self, id: &str) -> Result<bool, WorkflowError> {
    let reg = self
      .registered
      .lock()
      .map_err(|_| WorkflowError::InvalidWorkflowId)?;
    Ok(reg.contains_key(id))
  }

  fn completed_json(&self, workflow_id: &str, seq: u32) -> Result<Option<String>, WorkflowError> {
    if !self.has_workflow(workflow_id)? {
      return Err(WorkflowError::UnknownWorkflow(workflow_id.to_string()));
    }
    let store = self
      .inner
      .lock()
      .map_err(|_| WorkflowError::InvalidWorkflowId)?;
    Ok(store.get(workflow_id).and_then(|m| m.get(&seq)).cloned())
  }

  fn run_step_typed<T, F>(
    &mut self,
    workflow_id: &str,
    seq: u32,
    step_name: &str,
    compute: F,
  ) -> Result<T, WorkflowError>
  where
    T: Serialize + DeserializeOwned,
    F: FnOnce() -> Result<T, WorkflowError>,
  {
    let _ = step_name;
    if !self.has_workflow(workflow_id)? {
      return Err(WorkflowError::UnknownWorkflow(workflow_id.to_string()));
    }
    if let Some(json) = self.completed_json(workflow_id, seq)? {
      return serde_json::from_str(&json).map_err(WorkflowError::Json);
    }
    let value = compute()?;
    let json = serde_json::to_string(&value)?;
    let mut store = self
      .inner
      .lock()
      .map_err(|_| WorkflowError::InvalidWorkflowId)?;
    store
      .entry(workflow_id.to_string())
      .or_default()
      .insert(seq, json);
    Ok(value)
  }

  fn completed_step_count(&self, workflow_id: &str) -> Result<u32, WorkflowError> {
    if !self.has_workflow(workflow_id)? {
      return Err(WorkflowError::UnknownWorkflow(workflow_id.to_string()));
    }
    let store = self
      .inner
      .lock()
      .map_err(|_| WorkflowError::InvalidWorkflowId)?;
    Ok(store.get(workflow_id).map(|m| m.len() as u32).unwrap_or(0))
  }
}

// ---------------------------------------------------------------------------
// Transport-backed distributed journal
// ---------------------------------------------------------------------------

/// Wire request from a [`RemoteStepJournal`] client to a [`JournalServer`].
#[derive(Debug, Clone, Serialize, Deserialize)]
enum JournalRequest {
  Register {
    id: String,
  },
  HasWorkflow {
    id: String,
  },
  CompletedJson {
    workflow_id: String,
    seq: u32,
  },
  Append {
    workflow_id: String,
    seq: u32,
    json: String,
  },
  CompletedStepCount {
    workflow_id: String,
  },
  RecordDistributedStep {
    workflow_id: String,
    seq: u32,
    node_id: String,
  },
  DistributedNode {
    workflow_id: String,
    seq: u32,
  },
}

/// Wire response from a [`JournalServer`] back to the client.
#[derive(Debug, Clone, Serialize, Deserialize)]
enum JournalResponse {
  Registered,
  HasWorkflow(bool),
  CompletedJson(Option<String>),
  Appended,
  CompletedStepCount(u32),
  RecordedDistributedStep,
  DistributedNode(Option<String>),
  Err(JournalWireError),
}

/// Serializable projection of [`WorkflowError`] carried over the wire.
#[derive(Debug, Clone, Serialize, Deserialize)]
enum JournalWireError {
  InvalidWorkflowId,
  AlreadyExists(String),
  Unknown(String),
  Backend(String),
}

impl From<JournalWireError> for WorkflowError {
  fn from(err: JournalWireError) -> Self {
    match err {
      JournalWireError::InvalidWorkflowId => WorkflowError::InvalidWorkflowId,
      JournalWireError::AlreadyExists(id) => WorkflowError::WorkflowAlreadyExists(id),
      JournalWireError::Unknown(id) => WorkflowError::UnknownWorkflow(id),
      JournalWireError::Backend(msg) => WorkflowError::Transport(msg),
    }
  }
}

/// Byte-frame transport moving journal requests/responses between a client and a server.
///
/// The frames are opaque to the transport: an in-process [`LoopbackTransport`] dispatches to a
/// co-located [`JournalServer`], while a production implementation would relay the same bytes
/// over `id_effect_rpc` or a mesh channel to a remote [`JournalServer::handle_frame`].
pub trait JournalTransport: Send + Sync {
  /// Sends a request frame and returns the response frame.
  fn round_trip(&self, request: &[u8]) -> Result<Vec<u8>, WorkflowError>;
}

#[derive(Debug, Default)]
struct JournalStore {
  registered: HashMap<String, ()>,
  steps: HashMap<String, HashMap<u32, String>>,
  placements: HashMap<(String, u32), String>,
}

/// Server side of the distributed journal: owns durable state and answers wire frames.
///
/// Share one instance behind an [`Arc`] across a [`LoopbackTransport`] (or a real rpc handler
/// that forwards bytes to [`JournalServer::handle_frame`]).
#[derive(Debug, Default)]
pub struct JournalServer {
  store: Mutex<JournalStore>,
}

impl JournalServer {
  /// Creates an empty journal server.
  #[inline]
  pub fn new() -> Self {
    Self::default()
  }

  /// Decodes a request frame, applies it, and encodes the response frame.
  ///
  /// Never fails at the frame level: decode failures and application errors are encoded into an
  /// error response so the transport only ever moves bytes.
  pub fn handle_frame(&self, frame: &[u8]) -> Vec<u8> {
    let response = match serde_json::from_slice::<JournalRequest>(frame) {
      Ok(request) => self.dispatch(request),
      Err(e) => JournalResponse::Err(JournalWireError::Backend(format!("decode request: {e}"))),
    };
    serde_json::to_vec(&response).unwrap_or_else(|e| {
      // Errors are the only responses that can carry an unserializable payload; encode a
      // last-resort backend error frame so the client still sees a decodable response.
      serde_json::to_vec(&JournalResponse::Err(JournalWireError::Backend(format!(
        "encode response: {e}"
      ))))
      .unwrap_or_default()
    })
  }

  fn dispatch(&self, request: JournalRequest) -> JournalResponse {
    let mut store = match self.store.lock() {
      Ok(store) => store,
      Err(_) => {
        return JournalResponse::Err(JournalWireError::Backend("journal store poisoned".into()));
      }
    };
    match request {
      JournalRequest::Register { id } => {
        if id.trim().is_empty() {
          return JournalResponse::Err(JournalWireError::InvalidWorkflowId);
        }
        if store.registered.contains_key(&id) {
          return JournalResponse::Err(JournalWireError::AlreadyExists(id));
        }
        store.registered.insert(id, ());
        JournalResponse::Registered
      }
      JournalRequest::HasWorkflow { id } => {
        JournalResponse::HasWorkflow(store.registered.contains_key(&id))
      }
      JournalRequest::CompletedJson { workflow_id, seq } => {
        if !store.registered.contains_key(&workflow_id) {
          return JournalResponse::Err(JournalWireError::Unknown(workflow_id));
        }
        JournalResponse::CompletedJson(
          store
            .steps
            .get(&workflow_id)
            .and_then(|m| m.get(&seq))
            .cloned(),
        )
      }
      JournalRequest::Append {
        workflow_id,
        seq,
        json,
      } => {
        if !store.registered.contains_key(&workflow_id) {
          return JournalResponse::Err(JournalWireError::Unknown(workflow_id));
        }
        store
          .steps
          .entry(workflow_id)
          .or_default()
          .insert(seq, json);
        JournalResponse::Appended
      }
      JournalRequest::CompletedStepCount { workflow_id } => {
        if !store.registered.contains_key(&workflow_id) {
          return JournalResponse::Err(JournalWireError::Unknown(workflow_id));
        }
        JournalResponse::CompletedStepCount(
          store
            .steps
            .get(&workflow_id)
            .map(|m| m.len() as u32)
            .unwrap_or(0),
        )
      }
      JournalRequest::RecordDistributedStep {
        workflow_id,
        seq,
        node_id,
      } => {
        if !store.registered.contains_key(&workflow_id) {
          return JournalResponse::Err(JournalWireError::Unknown(workflow_id));
        }
        store.placements.insert((workflow_id, seq), node_id);
        JournalResponse::RecordedDistributedStep
      }
      JournalRequest::DistributedNode { workflow_id, seq } => {
        JournalResponse::DistributedNode(store.placements.get(&(workflow_id, seq)).cloned())
      }
    }
  }
}

/// In-process [`JournalTransport`] dispatching straight to a shared [`JournalServer`].
///
/// Serializes/deserializes real frames (so it exercises the wire protocol) but skips the
/// network. Multiple clients cloning the same `Arc<JournalServer>` share durable state — this is
/// how two endpoints resume the same workflow in tests and single-process embeddings.
#[derive(Debug, Clone)]
pub struct LoopbackTransport {
  server: Arc<JournalServer>,
}

impl LoopbackTransport {
  /// Binds a transport to a shared server.
  #[inline]
  pub fn new(server: Arc<JournalServer>) -> Self {
    Self { server }
  }
}

impl JournalTransport for LoopbackTransport {
  fn round_trip(&self, request: &[u8]) -> Result<Vec<u8>, WorkflowError> {
    Ok(self.server.handle_frame(request))
  }
}

/// Real distributed [`StepJournal`]: a client that speaks the wire protocol over a
/// [`JournalTransport`] to a [`JournalServer`].
///
/// Swap the transport to change where the journal lives: [`LoopbackTransport`] for in-process,
/// an rpc/mesh transport for a remote server — the FSM/saga call sites are unchanged.
#[derive(Debug, Clone)]
pub struct RemoteStepJournal<T: JournalTransport> {
  config: DistributedJournalConfig,
  transport: T,
}

impl<T: JournalTransport> RemoteStepJournal<T> {
  /// Creates a client with default endpoint metadata.
  #[inline]
  pub fn new(transport: T) -> Self {
    Self::with_config(transport, DistributedJournalConfig::default())
  }

  /// Creates a client tagged with the endpoint the transport dials.
  #[inline]
  pub fn with_config(transport: T, config: DistributedJournalConfig) -> Self {
    Self { config, transport }
  }

  /// Endpoint the underlying transport targets.
  #[inline]
  pub fn endpoint(&self) -> &str {
    &self.config.endpoint
  }

  /// Reads back the node that executed step `seq`, if a placement was recorded.
  pub fn distributed_node(
    &self,
    workflow_id: &str,
    seq: u32,
  ) -> Result<Option<String>, WorkflowError> {
    match self.call(JournalRequest::DistributedNode {
      workflow_id: workflow_id.to_string(),
      seq,
    })? {
      JournalResponse::DistributedNode(node) => Ok(node),
      JournalResponse::Err(e) => Err(e.into()),
      other => Err(unexpected(&other)),
    }
  }

  fn call(&self, request: JournalRequest) -> Result<JournalResponse, WorkflowError> {
    let frame = serde_json::to_vec(&request).map_err(WorkflowError::Json)?;
    let bytes = self.transport.round_trip(&frame)?;
    serde_json::from_slice(&bytes)
      .map_err(|e| WorkflowError::Transport(format!("decode response: {e}")))
  }
}

fn unexpected(response: &JournalResponse) -> WorkflowError {
  WorkflowError::Transport(format!("unexpected journal response: {response:?}"))
}

impl<T: JournalTransport> StepJournal for RemoteStepJournal<T> {
  fn register_workflow(&mut self, id: &str) -> Result<(), WorkflowError> {
    match self.call(JournalRequest::Register { id: id.to_string() })? {
      JournalResponse::Registered => Ok(()),
      JournalResponse::Err(e) => Err(e.into()),
      other => Err(unexpected(&other)),
    }
  }

  fn has_workflow(&self, id: &str) -> Result<bool, WorkflowError> {
    match self.call(JournalRequest::HasWorkflow { id: id.to_string() })? {
      JournalResponse::HasWorkflow(present) => Ok(present),
      JournalResponse::Err(e) => Err(e.into()),
      other => Err(unexpected(&other)),
    }
  }

  fn completed_json(&self, workflow_id: &str, seq: u32) -> Result<Option<String>, WorkflowError> {
    match self.call(JournalRequest::CompletedJson {
      workflow_id: workflow_id.to_string(),
      seq,
    })? {
      JournalResponse::CompletedJson(json) => Ok(json),
      JournalResponse::Err(e) => Err(e.into()),
      other => Err(unexpected(&other)),
    }
  }

  fn run_step_typed<T2, F>(
    &mut self,
    workflow_id: &str,
    seq: u32,
    step_name: &str,
    compute: F,
  ) -> Result<T2, WorkflowError>
  where
    T2: Serialize + DeserializeOwned,
    F: FnOnce() -> Result<T2, WorkflowError>,
  {
    let _ = step_name;
    if let Some(json) = self.completed_json(workflow_id, seq)? {
      return serde_json::from_str(&json).map_err(WorkflowError::Json);
    }
    let value = compute()?;
    let json = serde_json::to_string(&value)?;
    match self.call(JournalRequest::Append {
      workflow_id: workflow_id.to_string(),
      seq,
      json,
    })? {
      JournalResponse::Appended => Ok(value),
      JournalResponse::Err(e) => Err(e.into()),
      other => Err(unexpected(&other)),
    }
  }

  fn completed_step_count(&self, workflow_id: &str) -> Result<u32, WorkflowError> {
    match self.call(JournalRequest::CompletedStepCount {
      workflow_id: workflow_id.to_string(),
    })? {
      JournalResponse::CompletedStepCount(n) => Ok(n),
      JournalResponse::Err(e) => Err(e.into()),
      other => Err(unexpected(&other)),
    }
  }
}

impl<T: JournalTransport> DistributedStepJournal for RemoteStepJournal<T> {
  fn record_distributed_step(
    &mut self,
    workflow_id: &str,
    seq: u32,
    node_id: &str,
  ) -> Result<(), WorkflowError> {
    match self.call(JournalRequest::RecordDistributedStep {
      workflow_id: workflow_id.to_string(),
      seq,
      node_id: node_id.to_string(),
    })? {
      JournalResponse::RecordedDistributedStep => Ok(()),
      JournalResponse::Err(e) => Err(e.into()),
      other => Err(unexpected(&other)),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::sync::atomic::{AtomicU32, Ordering};

  #[test]
  fn network_stub_skips_recompute_on_resume() {
    let mut journal = NetworkJournalStub::new();
    journal.register_workflow("remote-wf").unwrap();
    let runs = AtomicU32::new(0);
    let a: i32 = journal
      .run_step_typed("remote-wf", 0, "s0", || {
        runs.fetch_add(1, Ordering::SeqCst);
        Ok(7)
      })
      .unwrap();
    assert_eq!(a, 7);
    assert_eq!(runs.load(Ordering::SeqCst), 1);
    let b: i32 = journal
      .run_step_typed("remote-wf", 0, "s0", || {
        runs.fetch_add(1, Ordering::SeqCst);
        Ok(0)
      })
      .unwrap();
    assert_eq!(b, 7);
    assert_eq!(runs.load(Ordering::SeqCst), 1);
  }

  #[test]
  fn distributed_config_default_endpoint() {
    let cfg = DistributedJournalConfig::default();
    assert!(!cfg.endpoint.is_empty());
    assert!(!cfg.require_tls);
  }

  #[test]
  fn network_stub_with_config_exposes_endpoint() {
    let cfg = DistributedJournalConfig {
      endpoint: "https://journal.test".into(),
      require_tls: true,
    };
    let stub = NetworkJournalStub::with_config(cfg);
    assert_eq!(stub.endpoint(), "https://journal.test");
  }

  #[test]
  fn register_empty_id_rejected() {
    let mut j = NetworkJournalStub::new();
    assert!(j.register_workflow("").is_err());
  }

  #[test]
  fn completed_json_unknown_workflow_errors() {
    let j = NetworkJournalStub::new();
    assert!(j.completed_json("missing", 0).is_err());
  }

  #[test]
  fn open_sqlite_journal_round_trip() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("wf.db");
    let mut log = NetworkJournalStub::open_sqlite(&path).expect("open");
    log.register_workflow("sqlite-wf").unwrap();
    let n: u32 = log
      .run_step_typed("sqlite-wf", 0, "s", || Ok(9u32))
      .unwrap();
    assert_eq!(n, 9);
  }

  #[test]
  fn network_stub_register_duplicate_fails() {
    let mut j = NetworkJournalStub::new();
    j.register_workflow("w").unwrap();
    assert!(j.register_workflow("w").is_err());
  }

  #[test]
  fn network_stub_completed_step_count() {
    let mut j = NetworkJournalStub::new();
    j.register_workflow("w").unwrap();
    j.run_step_typed("w", 0, "a", || Ok(1u32)).unwrap();
    assert_eq!(j.completed_step_count("w").unwrap(), 1);
    assert!(j.has_workflow("w").unwrap());
  }

  #[test]
  fn sqlite_log_satisfies_step_journal() {
    fn exercise<J: StepJournal>(journal: &mut J) -> Result<(), WorkflowError> {
      journal.register_workflow("trait-wf")?;
      let v: String = journal.run_step_typed("trait-wf", 0, "x", || Ok("ok".to_string()))?;
      assert_eq!(v, "ok");
      Ok(())
    }
    let mut log = DurableWorkflowLog::open_in_memory().unwrap();
    exercise(&mut log).unwrap();
  }

  #[test]
  fn completed_step_count_unknown_workflow_errors() {
    let j = NetworkJournalStub::new();
    assert!(j.completed_step_count("missing").is_err());
  }

  #[test]
  fn run_step_unknown_workflow_errors() {
    let mut j = NetworkJournalStub::new();
    let err = j.run_step_typed("missing", 0, "s", || Ok(1u32));
    assert!(err.is_err());
  }

  #[test]
  fn has_workflow_false_for_unregistered() {
    let j = NetworkJournalStub::new();
    assert!(!j.has_workflow("nope").unwrap());
  }

  #[test]
  fn remote_journal_two_endpoints_resume() {
    let server = Arc::new(JournalServer::new());
    let mut endpoint_a = RemoteStepJournal::new(LoopbackTransport::new(server.clone()));
    let mut endpoint_b = RemoteStepJournal::new(LoopbackTransport::new(server.clone()));

    endpoint_a.register_workflow("dist-wf").unwrap();

    let runs = AtomicU32::new(0);
    let first: i32 = endpoint_a
      .run_step_typed("dist-wf", 0, "s0", || {
        runs.fetch_add(1, Ordering::SeqCst);
        Ok(42)
      })
      .unwrap();
    assert_eq!(first, 42);
    assert_eq!(runs.load(Ordering::SeqCst), 1);

    // Second endpoint resumes the same workflow: cache hit, compute must NOT run.
    let resumed: i32 = endpoint_b
      .run_step_typed("dist-wf", 0, "s0", || {
        runs.fetch_add(1, Ordering::SeqCst);
        Ok(-1)
      })
      .unwrap();
    assert_eq!(resumed, 42);
    assert_eq!(runs.load(Ordering::SeqCst), 1);
    assert!(endpoint_b.has_workflow("dist-wf").unwrap());
    assert_eq!(endpoint_b.completed_step_count("dist-wf").unwrap(), 1);
  }

  #[test]
  fn remote_journal_records_distributed_step_across_endpoints() {
    let server = Arc::new(JournalServer::new());
    let mut writer = RemoteStepJournal::new(LoopbackTransport::new(server.clone()));
    let reader = RemoteStepJournal::new(LoopbackTransport::new(server.clone()));

    writer.register_workflow("placed-wf").unwrap();
    let _: u32 = writer
      .run_step_typed("placed-wf", 0, "s0", || Ok(7u32))
      .unwrap();
    writer
      .record_distributed_step("placed-wf", 0, "node-b")
      .unwrap();

    assert_eq!(
      reader.distributed_node("placed-wf", 0).unwrap(),
      Some("node-b".to_string())
    );
    assert_eq!(
      reader.completed_json("placed-wf", 0).unwrap(),
      Some("7".to_string())
    );
  }

  #[test]
  fn remote_journal_unknown_workflow_errors() {
    let server = Arc::new(JournalServer::new());
    let journal = RemoteStepJournal::new(LoopbackTransport::new(server));
    let err = journal.completed_json("missing", 0);
    assert!(matches!(err, Err(WorkflowError::UnknownWorkflow(_))));
  }

  #[test]
  fn remote_journal_duplicate_register_errors() {
    let server = Arc::new(JournalServer::new());
    let mut journal = RemoteStepJournal::new(LoopbackTransport::new(server));
    journal.register_workflow("w").unwrap();
    assert!(matches!(
      journal.register_workflow("w"),
      Err(WorkflowError::WorkflowAlreadyExists(_))
    ));
  }

  #[test]
  fn remote_journal_empty_id_rejected() {
    let server = Arc::new(JournalServer::new());
    let mut journal = RemoteStepJournal::new(LoopbackTransport::new(server));
    assert!(matches!(
      journal.register_workflow(""),
      Err(WorkflowError::InvalidWorkflowId)
    ));
  }

  #[test]
  fn remote_journal_respects_config_endpoint() {
    let cfg = DistributedJournalConfig {
      endpoint: "https://journal.cluster".into(),
      require_tls: true,
    };
    let server = Arc::new(JournalServer::new());
    let journal = RemoteStepJournal::with_config(LoopbackTransport::new(server), cfg);
    assert_eq!(journal.endpoint(), "https://journal.cluster");
  }

  #[test]
  fn remote_journal_satisfies_distributed_trait() {
    fn exercise<J: DistributedStepJournal>(journal: &mut J) -> Result<(), WorkflowError> {
      journal.register_workflow("trait-dist")?;
      let _: u32 = journal.run_step_typed("trait-dist", 0, "x", || Ok(1u32))?;
      journal.record_distributed_step("trait-dist", 0, "n0")
    }
    let server = Arc::new(JournalServer::new());
    let mut journal = RemoteStepJournal::new(LoopbackTransport::new(server));
    exercise(&mut journal).unwrap();
  }
}
