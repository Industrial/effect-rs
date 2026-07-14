//! [`Addr`] — typed send/cast/call handle to a process mailbox.

use std::future::poll_fn;
use std::pin::pin;
use std::task::Poll;
use std::time::Duration;

use id_effect::{Cause, Deferred, Effect, Queue, box_future, run_blocking};

use crate::exit::ExitReason;
use crate::msg::{CallError, Info, ProcMsg, SendError};
use crate::node::ProcessNode;
use crate::pid::Pid;
use crate::process::Process;

/// Typed handle to a process implementing behavior `P`.
///
/// Cloneable; sending to a dead process fails with [`SendError::Rejected`]
/// (send) or [`CallError`] (call) — never panics.
pub struct Addr<P: Process> {
  pid: Pid,
  mailbox: Queue<ProcMsg<P>>,
  node: ProcessNode,
}

impl<P: Process> Clone for Addr<P> {
  fn clone(&self) -> Self {
    Self {
      pid: self.pid.clone(),
      mailbox: self.mailbox.clone(),
      node: self.node.clone(),
    }
  }
}

impl<P: Process> Addr<P> {
  pub(crate) fn new(pid: Pid, mailbox: Queue<ProcMsg<P>>, node: ProcessNode) -> Self {
    Self { pid, mailbox, node }
  }

  /// The pid this address points at.
  pub fn pid(&self) -> &Pid {
    &self.pid
  }

  /// Clone the typed mailbox (distribution layer installs wire gateways).
  pub fn clone_mailbox(&self) -> Queue<ProcMsg<P>> {
    self.mailbox.clone()
  }

  /// Whether the process is still alive on its node.
  pub fn is_alive(&self) -> bool {
    self.node.table().lookup(&self.pid).is_some()
  }

  /// Fire-and-forget request (`GenServer.cast`).
  pub fn cast(&self, msg: P::Cast) -> Effect<(), SendError, ()> {
    self.offer(ProcMsg::Cast(msg))
  }

  /// Plain message delivered to `handle_info` (BEAM's bare `send`).
  pub fn send(&self, msg: P::Info) -> Effect<(), SendError, ()> {
    self.offer(ProcMsg::Info(Info::User(msg)))
  }

  /// Non-effect variant of [`Addr::send`] for use from plain code
  /// (timers, transports). Returns `false` if the mailbox is closed.
  pub fn send_now(&self, msg: P::Info) -> bool {
    run_blocking(self.mailbox.offer(ProcMsg::Info(Info::User(msg))), ()).unwrap_or(false)
  }

  fn offer(&self, msg: ProcMsg<P>) -> Effect<(), SendError, ()> {
    let mailbox = self.mailbox.clone();
    Effect::new_async(move |_r| {
      box_future(async move {
        match mailbox.offer(msg).run(&mut ()).await {
          Ok(true) => Ok(()),
          _ => Err(SendError::Rejected),
        }
      })
    })
  }

  /// Synchronous request with a deadline (`GenServer.call`).
  ///
  /// Implemented exactly as BEAM's convention: monitor + one-shot reply +
  /// timeout. If the callee exits before replying the call fails with
  /// [`CallError::Down`]; if the deadline passes first it fails with
  /// [`CallError::Timeout`] (the request may still execute — at-most-once).
  pub fn call(&self, msg: P::Call, timeout: Duration) -> Effect<P::Reply, CallError, ()> {
    let mailbox = self.mailbox.clone();
    let node = self.node.clone();
    let pid = self.pid.clone();
    Effect::new_async(move |_r| {
      box_future(async move {
        let reply: Deferred<P::Reply, CallError> = Deferred::make()
          .run(&mut ())
          .await
          .unwrap_or_else(|_| unreachable!("Deferred::make is infallible"));

        // Monitor completes the deferred with Down if the callee dies
        // before replying (first completion wins).
        let down_reply = reply.clone();
        let monitor = node.monitor_with_sink(
          &pid,
          Box::new(move |_m, _p, reason: ExitReason| {
            down_reply.try_fail_cause(Cause::fail(CallError::Down(reason)));
          }),
        );

        let offered = mailbox
          .offer(ProcMsg::Call(msg, reply.clone()))
          .run(&mut ())
          .await
          .unwrap_or(false);
        if !offered {
          node.demonitor(&pid, monitor);
          return Err(CallError::MailboxClosed);
        }

        let clock = node.clock();
        let mut sleep_env = ();
        let outcome = {
          let mut reply_fut = pin!(reply.wait_future());
          let mut timeout_fut = pin!(clock.sleep(timeout).run(&mut sleep_env));
          poll_fn(move |cx| {
            if let Poll::Ready(r) = reply_fut.as_mut().poll(cx) {
              return Poll::Ready(Some(r));
            }
            if timeout_fut.as_mut().poll(cx).is_ready() {
              return Poll::Ready(None);
            }
            Poll::Pending
          })
          .await
        };

        node.demonitor(&pid, monitor);
        match outcome {
          Some(Ok(rep)) => Ok(rep),
          Some(Err(Cause::Fail(err))) => Err(err),
          Some(Err(_)) => Err(CallError::Down(ExitReason::Interrupted)),
          None => Err(CallError::Timeout),
        }
      })
    })
  }
}
