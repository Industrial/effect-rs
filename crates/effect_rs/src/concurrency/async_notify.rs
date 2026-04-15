//! Executor-agnostic wakeups (std `Waker` list). Used instead of `tokio::sync::Notify` so the core
//! `effect` crate stays free of Tokio.
//!
//! **Deprecated for new fiber completion paths:** prefer [`crate::coordination::deferred::Deferred`]. This module
//! remains for [`crate::runtime::CancellationToken`] until that type migrates off `AsyncNotify`.

use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;
use std::task::{Context, Poll, Waker};

#[derive(Debug)]
struct NotifyState {
  waiters: Vec<Waker>,
}

/// Wake cooperating futures registered via [`Self::notified`].
#[derive(Debug)]
pub struct AsyncNotify {
  state: Mutex<NotifyState>,
}

impl AsyncNotify {
  #[inline]
  pub fn new() -> Self {
    Self {
      state: Mutex::new(NotifyState {
        waiters: Vec::new(),
      }),
    }
  }

  /// Wake every waiter currently registered with [`Self::notified`].
  pub fn notify_waiters(&self) {
    let mut guard = self.state.lock().expect("async notify mutex poisoned");
    for waker in guard.waiters.drain(..) {
      waker.wake();
    }
  }

  /// Wait until the next [`Self::notify_waiters`] after this future starts waiting.
  #[inline]
  pub fn notified(&self) -> Notified<'_> {
    Notified {
      notify: self,
      armed: false,
    }
  }
}

impl Default for AsyncNotify {
  fn default() -> Self {
    Self::new()
  }
}

/// Future returned by [`AsyncNotify::notified`].
pub struct Notified<'a> {
  notify: &'a AsyncNotify,
  armed: bool,
}

impl Future for Notified<'_> {
  type Output = ();

  fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
    let this = self.as_mut().get_mut();
    if this.armed {
      this.armed = false;
      return Poll::Ready(());
    }
    let mut guard = this
      .notify
      .state
      .lock()
      .expect("async notify mutex poisoned");
    guard.waiters.push(cx.waker().clone());
    this.armed = true;
    Poll::Pending
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn default_creates_empty_notify() {
    let n = AsyncNotify::default();
    // notify_waiters on empty list is a no-op
    n.notify_waiters();
  }

  #[tokio::test]
  async fn notified_completes_after_notify_waiters() {
    let n = AsyncNotify::new();
    // Spawn a task that waits on notified()
    let n2 = std::sync::Arc::new(n);
    let n3 = n2.clone();
    let handle = tokio::spawn(async move {
      n3.notified().await;
    });
    // Give the task a chance to register
    tokio::task::yield_now().await;
    n2.notify_waiters();
    handle.await.expect("notified task should complete");
  }
}
