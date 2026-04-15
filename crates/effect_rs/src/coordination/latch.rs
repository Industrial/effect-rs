//! One-shot open gate — waiters block until [`Latch::open`] runs (idempotent).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::Notify;

use crate::kernel::{Effect, box_future};
use crate::runtime::Never;

// ── Latch ───────────────────────────────────────────────────────────────────

struct LatchInner {
  open: AtomicBool,
  notify: Notify,
}

/// A latch that starts closed; [`Latch::open`] wakes all waiters. [`Latch::open`] is idempotent.
#[derive(Clone)]
pub struct Latch {
  inner: Arc<LatchInner>,
}

impl Latch {
  /// Create a new closed latch.
  pub fn make() -> Effect<Self> {
    Effect::new(|_| {
      Ok(Latch {
        inner: Arc::new(LatchInner {
          open: AtomicBool::new(false),
          notify: Notify::new(),
        }),
      })
    })
  }

  /// Open the latch and wake every current waiter. No-op if already open.
  pub fn open(&self) -> Effect<(), Never, ()> {
    let inner = Arc::clone(&self.inner);
    Effect::new_async(move |_| {
      box_future(async move {
        if inner.open.swap(true, Ordering::SeqCst) {
          return Ok(());
        }
        inner.notify.notify_waiters();
        Ok(())
      })
    })
  }

  /// Wait until the latch is open (async).
  pub fn wait(&self) -> Effect<(), Never, ()> {
    let inner = Arc::clone(&self.inner);
    Effect::new_async(move |_| {
      box_future(async move {
        while !inner.open.load(Ordering::Acquire) {
          inner.notify.notified().await;
        }
        Ok(())
      })
    })
  }

  /// Whether [`Latch::open`] has been called successfully at least once.
  pub fn is_open(&self) -> Effect<bool, Never, ()> {
    let v = self.inner.open.load(Ordering::Acquire);
    Effect::new(move |_| Ok(v))
  }
}

/// `Latch.make()`
pub fn make() -> Effect<Latch> {
  Latch::make()
}

/// `Latch.open(l)`
pub fn open(l: &Latch) -> Effect<(), Never, ()> {
  l.open()
}

/// `Latch.wait(l)`
pub fn wait(l: &Latch) -> Effect<(), Never, ()> {
  l.wait()
}

/// `Latch.is_open(l)`
pub fn is_open(l: &Latch) -> Effect<bool, Never, ()> {
  l.is_open()
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::runtime::run_async;
  use tokio::task::LocalSet;

  #[tokio::test]
  async fn latch_wait_returns_after_open() {
    let local = LocalSet::new();
    local
      .run_until(async {
        let l = crate::runtime::run_blocking(Latch::make(), ()).unwrap();
        let l2 = l.clone();
        let w = tokio::task::spawn_local(async move { run_async(l2.wait(), ()).await.unwrap() });
        let o = tokio::task::spawn_local(async move {
          tokio::task::yield_now().await;
          run_async(l.open(), ()).await.unwrap();
        });
        let _ = tokio::join!(w, o);
      })
      .await;
  }

  #[tokio::test]
  async fn latch_open_is_idempotent() {
    let l = crate::runtime::run_blocking(Latch::make(), ()).unwrap();
    run_async(l.open(), ()).await.unwrap();
    run_async(l.open(), ()).await.unwrap();
    assert!(run_async(l.is_open(), ()).await.unwrap());
    run_async(l.wait(), ()).await.unwrap();
  }

  #[tokio::test]
  async fn free_functions_work_same_as_methods() {
    let l = crate::runtime::run_blocking(make(), ()).unwrap();
    assert!(!run_async(is_open(&l), ()).await.unwrap());
    run_async(open(&l), ()).await.unwrap();
    assert!(run_async(is_open(&l), ()).await.unwrap());
    run_async(wait(&l), ()).await.unwrap(); // already open, returns immediately
  }
}
