//! BehaviorRegistry for remote spawn (FLAME model).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use id_effect::{Deferred, Queue, run_blocking};
use id_effect_process::{CallError, ExitReason, Info, Pid, ProcMsg, Process, ProcessNode};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::envelope::{ExitReasonWire, RemoteFault};

/// Spawn fn for a registered behavior.
pub type SpawnFn =
  Arc<dyn Fn(&ProcessNode, &[u8], Option<&Pid>) -> Result<Pid, RemoteFault> + Send + Sync>;

/// Wire bytes into a mailbox.
pub type WireFn = Arc<dyn Fn(&[u8]) -> Result<(), RemoteFault> + Send + Sync>;

/// Call with deferred byte reply.
pub type CallWireFn = Arc<
  dyn Fn(&[u8], Deferred<Vec<u8>, RemoteFault>) -> Result<(), RemoteFault> + Send + Sync,
>;

#[derive(Clone)]
struct Route {
  cast: WireFn,
  info: WireFn,
  call: CallWireFn,
}

/// Named spawn factory.
#[derive(Clone)]
pub struct BehaviorEntry {
  /// Stable cluster-wide behavior name.
  pub name: String,
  /// Decode args, spawn, install route.
  pub spawn: SpawnFn,
}

/// Behavior name map + live pid routes.
#[derive(Clone, Default)]
pub struct BehaviorRegistry {
  entries: HashMap<String, BehaviorEntry>,
  routes: Arc<Mutex<HashMap<Pid, Route>>>,
}

impl BehaviorRegistry {
  /// Empty registry.
  pub fn new() -> Self {
    Self::default()
  }

  /// Look up spawn entry.
  pub fn get(&self, name: &str) -> Option<&BehaviorEntry> {
    self.entries.get(name)
  }

  /// True if pid has a wire route.
  pub fn has_route(&self, pid: &Pid) -> bool {
    self.routes.lock().expect("routes").contains_key(pid)
  }

  /// Spawn named behavior.
  pub fn spawn(
    &self,
    node: &ProcessNode,
    behavior: &str,
    args: &[u8],
    link_to: Option<&Pid>,
  ) -> Result<Pid, RemoteFault> {
    let entry = self
      .get(behavior)
      .ok_or_else(|| RemoteFault::UnknownBehavior(behavior.to_string()))?;
    (entry.spawn)(node, args, link_to)
  }

  /// Deliver cast bytes.
  pub fn deliver_cast(&self, pid: &Pid, bytes: &[u8]) -> Result<(), RemoteFault> {
    let route = self
      .routes
      .lock()
      .expect("routes")
      .get(pid)
      .cloned()
      .ok_or(RemoteFault::NoProc)?;
    (route.cast)(bytes)
  }

  /// Deliver info bytes.
  pub fn deliver_info(&self, pid: &Pid, bytes: &[u8]) -> Result<(), RemoteFault> {
    let route = self
      .routes
      .lock()
      .expect("routes")
      .get(pid)
      .cloned()
      .ok_or(RemoteFault::NoProc)?;
    (route.info)(bytes)
  }

  /// Deliver call bytes.
  pub fn deliver_call(
    &self,
    pid: &Pid,
    bytes: &[u8],
    reply: Deferred<Vec<u8>, RemoteFault>,
  ) -> Result<(), RemoteFault> {
    let route = self
      .routes
      .lock()
      .expect("routes")
      .get(pid)
      .cloned()
      .ok_or(RemoteFault::NoProc)?;
    (route.call)(bytes, reply)
  }

  /// Forget dead pid.
  pub fn forget(&self, pid: &Pid) {
    self.routes.lock().expect("routes").remove(pid);
  }

  /// Register behavior P under name.
  pub fn register_counter_like<P>(&mut self, name: impl Into<String>)
  where
    P: Process,
    P::Args: Serialize + DeserializeOwned + Send + 'static,
    P::Env: Default + Send + 'static,
    P::Call: Serialize + DeserializeOwned + Send + 'static,
    P::Reply: Serialize + DeserializeOwned + Clone + Send + Sync + 'static,
    P::Cast: Serialize + DeserializeOwned + Send + 'static,
    P::Info: Serialize + DeserializeOwned + Send + 'static,
  {
    self.register::<P>(name);
  }

  /// Register behavior P under name.
  pub fn register<P>(&mut self, name: impl Into<String>)
  where
    P: Process,
    P::Args: Serialize + DeserializeOwned + Send + 'static,
    P::Env: Default + Send + 'static,
    P::Call: Serialize + DeserializeOwned + Send + 'static,
    P::Reply: Serialize + DeserializeOwned + Clone + Send + Sync + 'static,
    P::Cast: Serialize + DeserializeOwned + Send + 'static,
    P::Info: Serialize + DeserializeOwned + Send + 'static,
  {
    let name = name.into();
    let routes = Arc::clone(&self.routes);
    let spawn: SpawnFn = Arc::new(move |node, args_bytes, link_to| {
      let args: P::Args = postcard::from_bytes(args_bytes)
        .map_err(|e| RemoteFault::Decode(e.to_string()))?;
      let addr = match link_to {
        Some(parent) => node.spawn_linked_to::<P>(parent, args, P::Env::default()),
        None => node.spawn::<P>(args, P::Env::default()),
      };
      install_route::<P>(&routes, addr.pid().clone(), addr.clone_mailbox());
      Ok(addr.pid().clone())
    });
    self.entries.insert(name.clone(), BehaviorEntry { name, spawn });
  }

  /// Adopt local process for remote delivery.
  pub fn adopt_local<P>(&self, name: &str, pid: &Pid, mailbox: Queue<ProcMsg<P>>)
  where
    P: Process,
    P::Call: Serialize + DeserializeOwned + Send + 'static,
    P::Reply: Serialize + DeserializeOwned + Clone + Send + Sync + 'static,
    P::Cast: Serialize + DeserializeOwned + Send + 'static,
    P::Info: Serialize + DeserializeOwned + Send + 'static,
  {
    if self.entries.contains_key(name) {
      install_route::<P>(&self.routes, pid.clone(), mailbox);
    }
  }
}

fn install_route<P>(
  routes: &Arc<Mutex<HashMap<Pid, Route>>>,
  pid: Pid,
  mailbox: Queue<ProcMsg<P>>,
) where
  P: Process,
  P::Call: Serialize + DeserializeOwned + Send + 'static,
  P::Reply: Serialize + DeserializeOwned + Clone + Send + Sync + 'static,
  P::Cast: Serialize + DeserializeOwned + Send + 'static,
  P::Info: Serialize + DeserializeOwned + Send + 'static,
{
  let cast: WireFn = {
    let mailbox = mailbox.clone();
    Arc::new(move |bytes| {
      let msg: P::Cast =
        postcard::from_bytes(bytes).map_err(|e| RemoteFault::Decode(e.to_string()))?;
      if run_blocking(mailbox.offer(ProcMsg::Cast(msg)), ()).unwrap_or(false) {
        Ok(())
      } else {
        Err(RemoteFault::NoProc)
      }
    })
  };
  let info: WireFn = {
    let mailbox = mailbox.clone();
    Arc::new(move |bytes| {
      let msg: P::Info =
        postcard::from_bytes(bytes).map_err(|e| RemoteFault::Decode(e.to_string()))?;
      if run_blocking(mailbox.offer(ProcMsg::Info(Info::User(msg))), ()).unwrap_or(false) {
        Ok(())
      } else {
        Err(RemoteFault::NoProc)
      }
    })
  };
  let call: CallWireFn = {
    let mailbox = mailbox.clone();
    Arc::new(move |bytes, reply| {
      let msg: P::Call =
        postcard::from_bytes(bytes).map_err(|e| RemoteFault::Decode(e.to_string()))?;
      let typed: Deferred<P::Reply, CallError> = run_blocking(Deferred::make(), ())
        .unwrap_or_else(|_| unreachable!("Deferred::make is infallible"));
      let bridge = typed.clone();
      if !run_blocking(mailbox.offer(ProcMsg::Call(msg, typed)), ()).unwrap_or(false) {
        return Err(RemoteFault::NoProc);
      }
      std::thread::spawn(move || match run_blocking(bridge.wait(), ()) {
        Ok(rep) => match postcard::to_allocvec(&rep) {
          Ok(encoded) => {
            let _ = reply.try_succeed(encoded);
          }
          Err(e) => {
            let _ = reply.try_fail_cause(id_effect::Cause::fail(RemoteFault::Decode(
              e.to_string(),
            )));
          }
        },
        Err(id_effect::Cause::Fail(CallError::Down(reason))) => {
          let wire: ExitReasonWire = (&reason).into();
          let _ = reply.try_fail_cause(id_effect::Cause::fail(RemoteFault::Down(wire)));
        }
        Err(id_effect::Cause::Fail(CallError::Timeout)) => {
          let _ = reply.try_fail_cause(id_effect::Cause::fail(RemoteFault::Timeout));
        }
        Err(id_effect::Cause::Fail(CallError::MailboxClosed)) => {
          let _ = reply.try_fail_cause(id_effect::Cause::fail(RemoteFault::NoProc));
        }
        Err(_) => {
          let wire: ExitReasonWire = (&ExitReason::Interrupted).into();
          let _ = reply.try_fail_cause(id_effect::Cause::fail(RemoteFault::Down(wire)));
        }
      });
      Ok(())
    })
  };
  routes.lock().expect("routes").insert(pid, Route { cast, info, call });
}
