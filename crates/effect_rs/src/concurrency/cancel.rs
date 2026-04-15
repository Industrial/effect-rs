//! [`CancellationToken`] — cooperative interruption signal (§ 7.4).
//!
//! Composes from Stratum 6 ([`crate::runtime`] execution primitives) and the `AsyncNotify`
//! internal primitive. [`check_interrupt`] lifts a token snapshot into a pure effect.

use core::sync::atomic;
use std::sync::{Arc, Mutex, Weak};

use crate::concurrency::async_notify::AsyncNotify;
use crate::kernel::{Effect, box_future};

use crate::runtime::Never;

/// Cooperative cancellation signal; propagates to [`Self::child_token`] descendants.
#[derive(Debug, Clone)]
pub struct CancellationToken {
  inner: Arc<CancellationInner>,
}

#[derive(Debug)]
struct CancellationInner {
  cancelled: atomic::AtomicBool,
  children: Mutex<Vec<Weak<CancellationInner>>>,
  notify: AsyncNotify,
}

impl CancellationToken {
  /// New token in the non-cancelled state.
  pub fn new() -> Self {
    Self {
      inner: Arc::new(CancellationInner {
        cancelled: atomic::AtomicBool::new(false),
        children: Mutex::new(Vec::new()),
        notify: AsyncNotify::new(),
      }),
    }
  }

  /// Set cancelled and notify waiters; cancels registered child tokens. Returns `false` if already cancelled.
  pub fn cancel(&self) -> bool {
    if self.inner.cancelled.swap(true, atomic::Ordering::SeqCst) {
      return false;
    }
    self.inner.notify.notify_waiters();
    let children = self
      .inner
      .children
      .lock()
      .expect("cancellation children mutex poisoned")
      .iter()
      .filter_map(Weak::upgrade)
      .map(|inner| CancellationToken { inner })
      .collect::<Vec<_>>();
    for child in children {
      child.cancel();
    }
    true
  }

  /// Current cancellation flag.
  #[inline]
  pub fn is_cancelled(&self) -> bool {
    self.inner.cancelled.load(atomic::Ordering::SeqCst)
  }

  /// Child token cancelled when this token is cancelled (or immediately if already cancelled).
  pub fn child_token(&self) -> Self {
    let child = CancellationToken::new();
    self
      .inner
      .children
      .lock()
      .expect("cancellation children mutex poisoned")
      .push(Arc::downgrade(&child.inner));
    if self.is_cancelled() {
      child.cancel();
    }
    child
  }

  /// Yields until [`Self::is_cancelled`] becomes true.
  pub async fn wait_cancelled(&self) {
    while !self.is_cancelled() {
      self.inner.notify.notified().await;
    }
  }

  /// Effect that completes when this token is cancelled.
  pub fn cancelled(&self) -> Effect<(), Never, ()> {
    let token = self.clone();
    Effect::new_async(move |_env| {
      box_future(async move {
        token.wait_cancelled().await;
        Ok::<(), Never>(())
      })
    })
  }
}

impl Default for CancellationToken {
  fn default() -> Self {
    Self::new()
  }
}

/// Snapshot whether `token` is cancelled (pure effect, no waiting).
#[inline]
pub fn check_interrupt(token: &CancellationToken) -> Effect<bool, Never, ()> {
  let cancelled = token.is_cancelled();
  Effect::new(move |_env| Ok::<bool, Never>(cancelled))
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::time::Duration;

  mod cancellation_token {
    use super::*;

    #[test]
    fn cancel_when_parent_is_cancelled_propagates_to_descendants() {
      let parent = CancellationToken::new();
      let child = parent.child_token();
      let grandchild = child.child_token();

      assert!(!parent.is_cancelled());
      assert!(!child.is_cancelled());
      assert!(!grandchild.is_cancelled());

      assert!(parent.cancel());
      assert!(parent.is_cancelled());
      assert!(child.is_cancelled());
      assert!(grandchild.is_cancelled());
    }

    #[test]
    fn cancel_when_child_is_cancelled_does_not_cancel_parent() {
      let parent = CancellationToken::new();
      let child = parent.child_token();

      assert!(child.cancel());
      assert!(child.is_cancelled());
      assert!(!parent.is_cancelled());
    }

    #[test]
    fn cancel_when_called_twice_is_idempotent_and_returns_false_on_second_call() {
      let token = CancellationToken::new();
      assert!(token.cancel());
      assert!(!token.cancel());
      assert!(token.is_cancelled());
    }

    #[test]
    fn default_creates_non_cancelled_token() {
      let token = CancellationToken::default();
      assert!(!token.is_cancelled());
      assert!(token.cancel());
      assert!(token.is_cancelled());
    }

    #[test]
    fn child_token_when_parent_already_cancelled_is_immediately_cancelled() {
      let parent = CancellationToken::new();
      parent.cancel();
      let child = parent.child_token();
      assert!(child.is_cancelled());
    }

    #[test]
    fn cancelled_effect_when_cancel_signal_arrives_completes_successfully() {
      let token = CancellationToken::new();
      let producer = token.clone();
      std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(1));
        producer.cancel();
      });
      let out = pollster::block_on(token.cancelled().run(&mut ()));
      assert_eq!(out, Ok(()));
    }
  }

  mod check_interrupt {
    use super::*;

    #[test]
    fn when_token_changes_reflects_latest_cancellation_state() {
      let token = CancellationToken::new();
      let before = pollster::block_on(super::check_interrupt(&token).run(&mut ()));
      token.cancel();
      let after = pollster::block_on(super::check_interrupt(&token).run(&mut ()));

      assert_eq!(before, Ok(false));
      assert_eq!(after, Ok(true));
    }
  }
}
