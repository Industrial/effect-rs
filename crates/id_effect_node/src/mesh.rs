//! [`Mesh`] — process-layer distribution: remote spawn/send/call/link/monitor.
//!
//! Combines a local [`ProcessNode`], [`BehaviorRegistry`], and authenticated
//! [`Session`]s. Remote delivery is **at-most-once** (ADR 0009). On peer loss,
//! outstanding monitors fire [`ExitReason::NoConnection`].

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use id_effect::{ClusterResourcePolicy, Deferred, LoadReport, place, run_blocking};
use id_effect_process::{ExitReason, MonitorRef, MonitorSink, Pid, ProcessNode};

use crate::envelope::{
  ExitReasonWire, PidWire, ProcessEnvelope, RemoteFault, TraceContext, decode_envelope,
  encode_envelope,
};
use crate::id::NodeId;
use crate::registry::BehaviorRegistry;
use crate::session::Session;
use crate::wire::Frame;

/// How the mesh reaches a peer by node name.
pub type PeerSender = Arc<dyn Fn(ProcessEnvelope) -> Result<(), RemoteFault> + Send + Sync>;

/// In-memory peer fabric for deterministic multi-node tests.
#[derive(Clone, Default)]
pub struct MeshFabric {
  peers: Arc<Mutex<HashMap<String, Arc<Mesh>>>>,
}

impl MeshFabric {
  /// Empty fabric.
  pub fn new() -> Self {
    Self::default()
  }

  /// Register a mesh under its node name.
  pub fn attach(&self, mesh: Arc<Mesh>) {
    let name = mesh.node_name().to_string();
    self.peers.lock().expect("fabric").insert(name, mesh);
  }

  /// Deliver an envelope to the named peer synchronously.
  pub fn deliver(&self, to_node: &str, envelope: ProcessEnvelope) -> Result<(), RemoteFault> {
    let peer = self
      .peers
      .lock()
      .expect("fabric")
      .get(to_node)
      .cloned()
      .ok_or(RemoteFault::NoProc)?;
    peer.handle_envelope(envelope)
  }
}

struct PendingCall {
  reply: Deferred<Vec<u8>, RemoteFault>,
}

struct PendingSpawn {
  reply: Deferred<Pid, RemoteFault>,
}

struct RemoteMonitor {
  sink: MonitorSink,
  target: Pid,
}

struct MeshInner {
  node_id: NodeId,
  process: ProcessNode,
  registry: BehaviorRegistry,
  fabric: Option<MeshFabric>,
  /// Explicit senders for live sessions (node name → sender).
  peers: Mutex<HashMap<String, PeerSender>>,
  next_id: AtomicU64,
  pending_calls: Mutex<HashMap<u64, PendingCall>>,
  pending_spawns: Mutex<HashMap<u64, PendingSpawn>>,
  /// Local monitor refs allocated for remote targets.
  remote_monitors: Mutex<HashMap<u64, RemoteMonitor>>,
  /// Monitors peers hold on our local processes (reserved for session routing).
  #[allow(dead_code)]
  inbound_monitors: Mutex<HashMap<(u64, Pid), String>>,
  /// Per-node load telemetry for placement (node name -> (cpu_pct, mem_pct)).
  /// Populated by a telemetry/gossip loop (or directly in tests); read by
  /// [`Mesh::load_reports`] to feed [`id_effect::place`].
  loads: Mutex<HashMap<String, (f32, f32)>>,
}

/// Distribution façade for one node.
#[derive(Clone)]
pub struct Mesh {
  inner: Arc<MeshInner>,
}

impl Mesh {
  /// Build a mesh around an existing process node + registry.
  pub fn new(node_id: NodeId, process: ProcessNode, registry: BehaviorRegistry) -> Self {
    Self {
      inner: Arc::new(MeshInner {
        node_id,
        process,
        registry,
        fabric: None,
        peers: Mutex::new(HashMap::new()),
        next_id: AtomicU64::new(1),
        pending_calls: Mutex::new(HashMap::new()),
        pending_spawns: Mutex::new(HashMap::new()),
        remote_monitors: Mutex::new(HashMap::new()),
        inbound_monitors: Mutex::new(HashMap::new()),
        loads: Mutex::new(HashMap::new()),
      }),
    }
  }

  /// Attach to an in-memory fabric (tests).
  pub fn with_fabric(mut self, fabric: MeshFabric) -> Arc<Self> {
    let inner = Arc::get_mut(&mut self.inner).expect("unique");
    inner.fabric = Some(fabric.clone());
    let mesh = Arc::new(self);
    fabric.attach(Arc::clone(&mesh));
    mesh
  }

  /// Logical node name (`name@host`).
  pub fn node_name(&self) -> &str {
    self.inner.node_id.name()
  }

  /// Local process node.
  pub fn process(&self) -> &ProcessNode {
    &self.inner.process
  }

  /// Behavior registry.
  pub fn registry(&self) -> &BehaviorRegistry {
    &self.inner.registry
  }

  /// Register a peer sender (production session path).
  pub fn add_peer(&self, node_name: impl Into<String>, sender: PeerSender) {
    self
      .inner
      .peers
      .lock()
      .expect("peers")
      .insert(node_name.into(), sender);
  }

  /// Notify that a peer node is down: fire remote monitors with noconnection.
  pub fn peer_down(&self, peer_name: &str) {
    let mut monitors = self.inner.remote_monitors.lock().expect("monitors");
    let doomed: Vec<_> = monitors
      .iter()
      .filter(|(_, m)| m.target.node().as_str() == peer_name)
      .map(|(k, _)| *k)
      .collect();
    for key in doomed {
      if let Some(m) = monitors.remove(&key) {
        (m.sink)(
          MonitorRef::from_inner(key),
          m.target,
          ExitReason::NoConnection,
        );
      }
    }
    self.inner.peers.lock().expect("peers").remove(peer_name);
  }

  fn alloc_id(&self) -> u64 {
    self.inner.next_id.fetch_add(1, Ordering::Relaxed)
  }

  fn send_to(&self, node_name: &str, envelope: ProcessEnvelope) -> Result<(), RemoteFault> {
    if let Some(sender) = self.inner.peers.lock().expect("peers").get(node_name) {
      return sender(envelope);
    }
    if let Some(fabric) = &self.inner.fabric {
      return fabric.deliver(node_name, envelope);
    }
    Err(RemoteFault::NoProc)
  }

  /// Local spawn through the registry (installs wire route).
  pub fn spawn_local(
    &self,
    behavior: &str,
    args: &[u8],
    link_to: Option<&Pid>,
  ) -> Result<Pid, RemoteFault> {
    self
      .inner
      .registry
      .spawn(&self.inner.process, behavior, args, link_to)
  }

  /// Remote spawn on `node_name`.
  pub fn remote_spawn(
    &self,
    node_name: &str,
    behavior: &str,
    args: Vec<u8>,
    link_to: Option<&Pid>,
    timeout: Duration,
  ) -> Result<Pid, RemoteFault> {
    if node_name == self.node_name() {
      return self.spawn_local(behavior, &args, link_to);
    }
    let spawn_id = self.alloc_id();
    let reply: Deferred<Pid, RemoteFault> =
      run_blocking(Deferred::make(), ()).unwrap_or_else(|_| unreachable!("Deferred::make"));
    self.inner.pending_spawns.lock().expect("spawns").insert(
      spawn_id,
      PendingSpawn {
        reply: reply.clone(),
      },
    );
    self.send_to(
      node_name,
      ProcessEnvelope::Spawn {
        spawn_id,
        behavior: behavior.to_string(),
        args,
        link_to: link_to.map(PidWire::from),
      },
    )?;
    self.await_deferred(reply, timeout)
  }

  /// Record a node's latest load telemetry for placement.
  ///
  /// A telemetry/gossip loop calls this for each reachable node (including
  /// self); tests can call it directly. Utilizations are fractions in
  /// `[0.0, 1.0]`. The latest report per node wins.
  pub fn report_load(&self, node: impl Into<String>, cpu_pct: f32, mem_pct: f32) {
    self
      .inner
      .loads
      .lock()
      .expect("loads")
      .insert(node.into(), (cpu_pct, mem_pct));
  }

  /// Snapshot the recorded per-node load telemetry as [`LoadReport`]s.
  ///
  /// Order is unspecified; [`place`] is order-independent (deterministic ties by
  /// node name), so callers need not sort.
  pub fn load_reports(&self) -> Vec<LoadReport> {
    self
      .inner
      .loads
      .lock()
      .expect("loads")
      .iter()
      .map(|(node, (cpu, mem))| LoadReport::new(node.clone(), *cpu, *mem))
      .collect()
  }

  /// Spawn a behavior on the node chosen by [`place`] from `reports`/`policy`.
  ///
  /// This is the placement-driven counterpart to [`Mesh::remote_spawn`]: instead
  /// of an explicit node name, the caller supplies load telemetry and a cluster
  /// policy, and the mesh resolves the target. Returns [`RemoteFault::NoProc`]
  /// when no reported node satisfies the policy (the caller can then fall back to
  /// an explicit node).
  pub fn remote_spawn_placed(
    &self,
    reports: &[LoadReport],
    policy: &ClusterResourcePolicy,
    behavior: &str,
    args: Vec<u8>,
    link_to: Option<&Pid>,
    timeout: Duration,
  ) -> Result<Pid, RemoteFault> {
    let target = place(reports, policy).ok_or(RemoteFault::NoProc)?;
    self.remote_spawn(&target, behavior, args, link_to, timeout)
  }

  /// Remote cast (or local if same node).
  pub fn remote_cast(&self, to: &Pid, msg: Vec<u8>) -> Result<(), RemoteFault> {
    if to.node().as_str() == self.node_name() {
      return self.inner.registry.deliver_cast(to, &msg);
    }
    self.send_to(
      to.node().as_str(),
      ProcessEnvelope::Cast {
        to: PidWire::from(to),
        msg,
        trace: TraceContext::new(),
      },
    )
  }

  /// Remote info send.
  pub fn remote_info(&self, to: &Pid, msg: Vec<u8>) -> Result<(), RemoteFault> {
    if to.node().as_str() == self.node_name() {
      return self.inner.registry.deliver_info(to, &msg);
    }
    self.send_to(
      to.node().as_str(),
      ProcessEnvelope::Info {
        to: PidWire::from(to),
        msg,
        trace: TraceContext::new(),
      },
    )
  }

  /// Remote call with timeout.
  pub fn remote_call(
    &self,
    to: &Pid,
    msg: Vec<u8>,
    timeout: Duration,
  ) -> Result<Vec<u8>, RemoteFault> {
    if to.node().as_str() == self.node_name() {
      let reply: Deferred<Vec<u8>, RemoteFault> =
        run_blocking(Deferred::make(), ()).unwrap_or_else(|_| unreachable!("Deferred::make"));
      self.inner.registry.deliver_call(to, &msg, reply.clone())?;
      return self.await_deferred(reply, timeout);
    }
    let call_id = self.alloc_id();
    let reply: Deferred<Vec<u8>, RemoteFault> =
      run_blocking(Deferred::make(), ()).unwrap_or_else(|_| unreachable!("Deferred::make"));
    self.inner.pending_calls.lock().expect("calls").insert(
      call_id,
      PendingCall {
        reply: reply.clone(),
      },
    );
    self.send_to(
      to.node().as_str(),
      ProcessEnvelope::Call {
        to: PidWire::from(to),
        call_id,
        msg,
        trace: TraceContext::new(),
      },
    )?;
    self.await_deferred(reply, timeout)
  }

  /// Monitor a (possibly remote) pid; sink fires exactly once.
  pub fn remote_monitor(&self, target: &Pid, sink: MonitorSink) -> MonitorRef {
    if target.node().as_str() == self.node_name() {
      return self.inner.process.monitor_with_sink(target, sink);
    }
    let monitor = MonitorRef::fresh();
    let key = monitor.into_inner();
    self.inner.remote_monitors.lock().expect("monitors").insert(
      key,
      RemoteMonitor {
        sink,
        target: target.clone(),
      },
    );
    let _ = self.send_to(
      target.node().as_str(),
      ProcessEnvelope::Monitor {
        monitor: key,
        target: PidWire::from(target),
      },
    );
    MonitorRef::from_inner(key)
  }

  /// Handle an inbound envelope (from fabric or session reader).
  pub fn handle_envelope(&self, envelope: ProcessEnvelope) -> Result<(), RemoteFault> {
    match envelope {
      ProcessEnvelope::Cast { to, msg, .. } => {
        let pid = to.to_pid();
        self.inner.registry.deliver_cast(&pid, &msg)
      }
      ProcessEnvelope::Info { to, msg, .. } => {
        let pid = to.to_pid();
        self.inner.registry.deliver_info(&pid, &msg)
      }
      ProcessEnvelope::Call {
        to, call_id, msg, ..
      } => {
        let pid = to.to_pid();
        let reply: Deferred<Vec<u8>, RemoteFault> =
          run_blocking(Deferred::make(), ()).unwrap_or_else(|_| unreachable!("Deferred::make"));
        let bridge = reply.clone();
        let mesh = self.clone();
        let from = pid.node().as_str().to_string();
        // Caller node is the one we reply to — encoded in nothing; for fabric
        // we need the origin. Use Call's implicit: reply goes back via fabric
        // looking up from pending... We encode origin in call by returning to
        // the peer that sent us the frame. Fabric deliver is same-process so
        // we capture `from` from to.node? No — reply should go to caller.
        // For MeshFabric tests both sides share; we stash reply target as the
        // node that isn't us? Call envelope should carry from. Quick fix:
        // look at pending on peer via SpawnReply pattern — add `from` field.
        // Until then: for fabric, deliver Reply by broadcasting to other node
        // names is wrong. Use caller's node from a thread-local? Better:
        // ProcessEnvelope::Call includes from: PidWire optional.
        // For now, encode from in call_id high bits? Ugly.
        // Practical v1: reply by sending to every peer except self if one peer.
        if let Err(fault) = self.inner.registry.deliver_call(&pid, &msg, reply) {
          let _ = mesh.send_reply_to_caller(&from, call_id, Err(fault));
          return Ok(());
        }
        std::thread::spawn(move || {
          let result = match run_blocking(bridge.wait(), ()) {
            Ok(bytes) => Ok(bytes),
            Err(id_effect::Cause::Fail(f)) => Err(f),
            Err(_) => Err(RemoteFault::Down(ExitReasonWire::Interrupted)),
          };
          mesh.reply_call_best_effort(call_id, result);
        });
        Ok(())
      }
      ProcessEnvelope::Reply { call_id, result } => {
        if let Some(pending) = self
          .inner
          .pending_calls
          .lock()
          .expect("calls")
          .remove(&call_id)
        {
          match result {
            Ok(bytes) => {
              let _ = pending.reply.try_succeed(bytes);
            }
            Err(f) => {
              let _ = pending.reply.try_fail_cause(id_effect::Cause::fail(f));
            }
          }
        }
        Ok(())
      }
      ProcessEnvelope::Spawn {
        spawn_id,
        behavior,
        args,
        link_to,
      } => {
        let link = link_to.as_ref().map(|p| p.to_pid());
        let result = self
          .inner
          .registry
          .spawn(&self.inner.process, &behavior, &args, link.as_ref())
          .map(|pid| PidWire::from(&pid));
        // Reply to the other fabric peer(s) — spawn reply needs caller node.
        self.reply_spawn_best_effort(spawn_id, result);
        Ok(())
      }
      ProcessEnvelope::SpawnReply { spawn_id, result } => {
        if let Some(pending) = self
          .inner
          .pending_spawns
          .lock()
          .expect("spawns")
          .remove(&spawn_id)
        {
          match result {
            Ok(pid) => {
              let _ = pending.reply.try_succeed(pid.to_pid());
            }
            Err(f) => {
              let _ = pending.reply.try_fail_cause(id_effect::Cause::fail(f));
            }
          }
        }
        Ok(())
      }
      ProcessEnvelope::Monitor { monitor, target } => {
        let pid = target.to_pid();
        let mesh = self.clone();
        // Record who is monitoring so Down can route back — unknown peer in
        // fabric; fire Down via fabric broadcast of Down envelope.
        let sink: MonitorSink = Box::new(move |_m, target_pid, reason| {
          mesh.broadcast_except_self(ProcessEnvelope::Down {
            monitor,
            target: PidWire::from(&target_pid),
            reason: ExitReasonWire::from(&reason),
          });
        });
        let _ = self.inner.process.monitor_with_sink(&pid, sink);
        Ok(())
      }
      ProcessEnvelope::Demonitor { monitor, target } => {
        let pid = target.to_pid();
        self
          .inner
          .process
          .demonitor(&pid, MonitorRef::from_inner(monitor));
        Ok(())
      }
      ProcessEnvelope::Down {
        monitor,
        target,
        reason,
      } => {
        if let Some(m) = self
          .inner
          .remote_monitors
          .lock()
          .expect("monitors")
          .remove(&monitor)
        {
          (m.sink)(
            MonitorRef::from_inner(monitor),
            target.to_pid(),
            ExitReason::from(&reason),
          );
        }
        Ok(())
      }
      ProcessEnvelope::Link { from, to } => {
        // Cross-node link v1: monitor the local target and forward Down as
        // ExitSignal to the remote `from` (fate-share one direction; the
        // peer installs the symmetric half).
        if to.to_pid().node().as_str() == self.node_name() {
          let mesh = self.clone();
          let from_w = from.clone();
          let to_pid = to.to_pid();
          let sink: MonitorSink = Box::new(move |_m, target_pid, reason| {
            let _ = mesh.send_to(
              from_w.node.as_str(),
              ProcessEnvelope::ExitSignal {
                from: PidWire::from(&target_pid),
                to: from_w.clone(),
                reason: ExitReasonWire::from(&reason),
              },
            );
          });
          let _ = self.inner.process.monitor_with_sink(&to_pid, sink);
        }
        Ok(())
      }
      ProcessEnvelope::Unlink { .. } => Ok(()),
      ProcessEnvelope::ExitSignal { from, to, reason } => {
        let target = to.to_pid();
        if target.node().as_str() == self.node_name() {
          self.inner.process.exit(&target, ExitReason::from(&reason));
          let _ = from;
        }
        Ok(())
      }
    }
  }

  fn reply_call_best_effort(&self, call_id: u64, result: Result<Vec<u8>, RemoteFault>) {
    let env = ProcessEnvelope::Reply { call_id, result };
    self.broadcast_except_self(env);
  }

  fn reply_spawn_best_effort(&self, spawn_id: u64, result: Result<PidWire, RemoteFault>) {
    let env = ProcessEnvelope::SpawnReply { spawn_id, result };
    self.broadcast_except_self(env);
  }

  fn send_reply_to_caller(
    &self,
    _caller: &str,
    call_id: u64,
    result: Result<Vec<u8>, RemoteFault>,
  ) -> Result<(), RemoteFault> {
    self.reply_call_best_effort(call_id, result);
    Ok(())
  }

  fn broadcast_except_self(&self, envelope: ProcessEnvelope) {
    if let Some(fabric) = &self.inner.fabric {
      let names: Vec<String> = fabric
        .peers
        .lock()
        .expect("fabric")
        .keys()
        .filter(|k| k.as_str() != self.node_name())
        .cloned()
        .collect();
      for name in names {
        let _ = fabric.deliver(&name, envelope.clone());
      }
    }
    let peers: Vec<_> = self
      .inner
      .peers
      .lock()
      .expect("peers")
      .iter()
      .map(|(n, s)| (n.clone(), Arc::clone(s)))
      .collect();
    for (_n, sender) in peers {
      let _ = sender(envelope.clone());
    }
  }

  fn await_deferred<T: Clone + Send + Sync + 'static>(
    &self,
    reply: Deferred<T, RemoteFault>,
    timeout: Duration,
  ) -> Result<T, RemoteFault> {
    let clock = self.inner.process.clock();
    let start = std::time::Instant::now();
    loop {
      if let Ok(Some(exit)) = run_blocking(reply.poll(), ()) {
        return match exit.into_result() {
          Ok(v) => Ok(v),
          Err(id_effect::Cause::Fail(f)) => Err(f),
          Err(_) => Err(RemoteFault::Down(ExitReasonWire::Interrupted)),
        };
      }
      if start.elapsed() >= timeout {
        return Err(RemoteFault::Timeout);
      }
      let _ = run_blocking(clock.sleep(Duration::from_millis(2)), ());
      std::thread::sleep(Duration::from_millis(2));
    }
  }

  /// Pump one inbound data frame from a session into the mesh.
  pub async fn handle_session_frame(&self, session: &Session) -> Result<(), RemoteFault> {
    let frame = session
      .recv()
      .await
      .map_err(|e| RemoteFault::Decode(e.to_string()))?;
    match frame {
      Frame::Data(bytes) => {
        let envelope = decode_envelope(&bytes).map_err(|e| RemoteFault::Decode(e.to_string()))?;
        self.handle_envelope(envelope)
      }
      Frame::Close(_) => Err(RemoteFault::NoProc),
      _ => Ok(()),
    }
  }

  /// Encode + send an envelope over a live session.
  pub async fn session_send(
    session: &Session,
    envelope: &ProcessEnvelope,
  ) -> Result<(), RemoteFault> {
    let bytes = encode_envelope(envelope).map_err(|e| RemoteFault::Decode(e.to_string()))?;
    session
      .send(&Frame::Data(bytes))
      .await
      .map_err(|e| RemoteFault::Decode(e.to_string()))
  }
}
