//! MPMC queue primitives — bounded, unbounded, dropping, and sliding backpressure.
//!
//! Flume backs bounded/unbounded/dropping; sliding uses an internal `VecDeque` with
//! fixed capacity (oldest dropped on overflow).

use std::collections::VecDeque;
use std::fmt;
use std::sync::Arc;

use tokio::sync::watch;
use tokio::sync::{Mutex, Notify};

use crate::Chunk;
use crate::kernel::{Effect, box_future, succeed};

/// Error returned when receiving from a [`Queue`] that has no senders and no buffered values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueError {
  /// No senders remain and the buffer is empty.
  Disconnected,
}

impl fmt::Display for QueueError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      QueueError::Disconnected => write!(f, "queue is disconnected"),
    }
  }
}

impl std::error::Error for QueueError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FlumeKind {
  Bounded,
  Unbounded,
  Dropping,
}

struct FlumeShared<A: Send> {
  tx: Mutex<Option<flume::Sender<A>>>,
  rx: Mutex<Option<flume::Receiver<A>>>,
  kind: FlumeKind,
  shutdown: watch::Sender<bool>,
}

impl<A: Send + 'static> FlumeShared<A> {
  fn new(
    kind: FlumeKind,
    tx: flume::Sender<A>,
    rx: flume::Receiver<A>,
    shutdown: watch::Sender<bool>,
  ) -> Self {
    Self {
      tx: Mutex::new(Some(tx)),
      rx: Mutex::new(Some(rx)),
      kind,
      shutdown,
    }
  }

  async fn offer(&self, value: A) -> bool {
    let mut guard = self.tx.lock().await;
    let Some(tx) = guard.as_mut() else {
      return false;
    };
    match self.kind {
      FlumeKind::Bounded => match tx.try_send(value) {
        Ok(()) => true,
        Err(flume::TrySendError::Full(_)) => false,
        Err(flume::TrySendError::Disconnected(_)) => false,
      },
      FlumeKind::Dropping => match tx.try_send(value) {
        Ok(()) => true,
        Err(flume::TrySendError::Full(v)) => {
          drop(v);
          false
        }
        Err(flume::TrySendError::Disconnected(_)) => false,
      },
      FlumeKind::Unbounded => match tx.try_send(value) {
        Ok(()) => true,
        Err(flume::TrySendError::Full(v)) => {
          drop(v);
          false
        }
        Err(flume::TrySendError::Disconnected(_)) => false,
      },
    }
  }

  async fn recv(&self) -> Result<A, QueueError> {
    let rx = {
      let guard = self.rx.lock().await;
      guard.as_ref().map(flume::Receiver::clone)
    };
    let Some(rx) = rx else {
      return Err(QueueError::Disconnected);
    };
    match rx.recv_async().await {
      Ok(v) => Ok(v),
      Err(_) => Err(QueueError::Disconnected),
    }
  }

  fn try_recv(&self) -> Result<Option<A>, QueueError> {
    let guard = self.rx.blocking_lock();
    let Some(rx) = guard.as_ref() else {
      return Err(QueueError::Disconnected);
    };
    match rx.try_recv() {
      Ok(v) => Ok(Some(v)),
      Err(flume::TryRecvError::Empty) => Ok(None),
      Err(flume::TryRecvError::Disconnected) => Err(QueueError::Disconnected),
    }
  }

  /// Like [`FlumeShared::offer`], but returns `Err(value)` when a **bounded** queue is full so
  /// callers (e.g. [`Queue::offer_all`]) can retain unstored elements.
  async fn offer_or_retain(&self, value: A) -> Result<(), A> {
    let mut guard = self.tx.lock().await;
    let Some(tx) = guard.as_mut() else {
      return Ok(());
    };
    match self.kind {
      FlumeKind::Bounded => match tx.try_send(value) {
        Ok(()) => Ok(()),
        Err(flume::TrySendError::Full(v)) => Err(v),
        Err(flume::TrySendError::Disconnected(v)) => {
          drop(v);
          Ok(())
        }
      },
      FlumeKind::Unbounded | FlumeKind::Dropping => match tx.try_send(value) {
        Ok(()) => Ok(()),
        Err(flume::TrySendError::Full(v)) => {
          drop(v);
          Ok(())
        }
        Err(flume::TrySendError::Disconnected(v)) => {
          drop(v);
          Ok(())
        }
      },
    }
  }

  fn len(&self) -> usize {
    let guard = self.rx.blocking_lock();
    guard.as_ref().map(flume::Receiver::len).unwrap_or(0)
  }

  fn is_empty(&self) -> bool {
    self.len() == 0
  }

  fn is_full(&self) -> bool {
    let tx_guard = self.tx.blocking_lock();
    let Some(tx) = tx_guard.as_ref() else {
      return true;
    };
    tx.is_full()
  }

  async fn shutdown(&self) {
    let mut guard = self.tx.lock().await;
    guard.take();
    let _ = self.shutdown.send(true);
  }

  fn is_shutdown(&self) -> bool {
    *self.shutdown.borrow()
  }

  async fn await_shutdown(&self) {
    if *self.shutdown.borrow() {
      return;
    }
    let mut rx = self.shutdown.subscribe();
    let _ = rx.changed().await;
  }
}

struct SlidingState<A> {
  deque: VecDeque<A>,
  open: bool,
}

struct SlidingShared<A: Send> {
  state: Mutex<SlidingState<A>>,
  capacity: usize,
  not_empty: Notify,
  shutdown: watch::Sender<bool>,
}

impl<A: Send + 'static> SlidingShared<A> {
  fn new(capacity: usize) -> Self {
    Self {
      state: Mutex::new(SlidingState {
        deque: VecDeque::new(),
        open: true,
      }),
      capacity: capacity.max(1),
      not_empty: Notify::new(),
      shutdown: watch::channel(false).0,
    }
  }

  async fn offer(&self, value: A) -> bool {
    self.offer_or_retain(value).await.is_ok()
  }

  /// Same as [`SlidingShared::offer`], but returns `Err(value)` when the queue is closed so
  /// [`Queue::offer_all`] can retain unstored elements.
  async fn offer_or_retain(&self, value: A) -> Result<(), A> {
    let mut g = self.state.lock().await;
    if !g.open {
      return Err(value);
    }
    g.deque.push_back(value);
    while g.deque.len() > self.capacity {
      g.deque.pop_front();
    }
    drop(g);
    self.not_empty.notify_waiters();
    Ok(())
  }

  async fn recv(&self) -> Result<A, QueueError> {
    loop {
      let maybe = {
        let mut g = self.state.lock().await;
        if let Some(v) = g.deque.pop_front() {
          Some(v)
        } else if !g.open {
          return Err(QueueError::Disconnected);
        } else {
          None
        }
      };
      if let Some(v) = maybe {
        return Ok(v);
      }
      self.not_empty.notified().await;
    }
  }

  fn try_recv(&self) -> Result<Option<A>, QueueError> {
    let mut g = self.state.blocking_lock();
    if let Some(v) = g.deque.pop_front() {
      return Ok(Some(v));
    }
    if !g.open {
      return Err(QueueError::Disconnected);
    }
    Ok(None)
  }

  async fn len(&self) -> usize {
    self.state.lock().await.deque.len()
  }

  async fn is_empty(&self) -> bool {
    self.state.lock().await.deque.is_empty()
  }

  async fn is_full(&self) -> bool {
    self.state.lock().await.deque.len() >= self.capacity
  }

  async fn shutdown(&self) {
    let mut g = self.state.lock().await;
    g.open = false;
    drop(g);
    let _ = self.shutdown.send(true);
    self.not_empty.notify_waiters();
  }

  fn is_shutdown(&self) -> bool {
    *self.shutdown.borrow()
  }

  async fn await_shutdown(&self) {
    if *self.shutdown.borrow() {
      return;
    }
    let mut rx = self.shutdown.subscribe();
    let _ = rx.changed().await;
  }
}

enum QueueRepr<A: Send + 'static> {
  Flume(Arc<FlumeShared<A>>),
  Sliding(Arc<SlidingShared<A>>),
}

/// Cloneable multi-producer / multi-consumer queue with several backpressure modes.
pub struct Queue<A: Send + 'static> {
  repr: Arc<QueueRepr<A>>,
}

impl<A: Send + 'static> Clone for Queue<A> {
  fn clone(&self) -> Self {
    Self {
      repr: Arc::clone(&self.repr),
    }
  }
}

impl<A: Send + 'static> Queue<A> {
  fn from_flume(kind: FlumeKind, tx: flume::Sender<A>, rx: flume::Receiver<A>) -> Self {
    let shutdown = watch::channel(false).0;
    Self {
      repr: Arc::new(QueueRepr::Flume(Arc::new(FlumeShared::new(
        kind, tx, rx, shutdown,
      )))),
    }
  }

  fn from_sliding(inner: Arc<SlidingShared<A>>) -> Self {
    Self {
      repr: Arc::new(QueueRepr::Sliding(inner)),
    }
  }

  /// Bounded queue: [`Queue::offer`] returns `false` when full (no blocking).
  pub fn bounded(capacity: usize) -> Effect<Queue<A>, (), ()> {
    let cap = capacity.max(1);
    let (tx, rx) = flume::bounded(cap);
    succeed(Self::from_flume(FlumeKind::Bounded, tx, rx))
  }

  /// Unbounded queue.
  pub fn unbounded() -> Effect<Queue<A>, (), ()> {
    let (tx, rx) = flume::unbounded();
    succeed(Self::from_flume(FlumeKind::Unbounded, tx, rx))
  }

  /// Bounded queue that drops the **incoming** element when full.
  pub fn dropping(capacity: usize) -> Effect<Queue<A>, (), ()> {
    let cap = capacity.max(1);
    let (tx, rx) = flume::bounded(cap);
    succeed(Self::from_flume(FlumeKind::Dropping, tx, rx))
  }

  /// Fixed-capacity queue that drops the **oldest** element when full.
  pub fn sliding(capacity: usize) -> Effect<Queue<A>, (), ()> {
    let inner = Arc::new(SlidingShared::new(capacity));
    succeed(Self::from_sliding(inner))
  }

  /// Try to enqueue one value. `false` means the value was not stored (full, dropping, or shut down).
  pub fn offer(&self, value: A) -> Effect<bool, (), ()> {
    let repr = Arc::clone(&self.repr);
    Effect::new_async(move |_r| {
      box_future(async move {
        match &*repr {
          QueueRepr::Flume(f) => Ok(f.offer(value).await),
          QueueRepr::Sliding(s) => Ok(s.offer(value).await),
        }
      })
    })
  }

  /// Enqueue as many values as possible; returns those that could not be stored (in order).
  pub fn offer_all<I>(&self, iter: I) -> Effect<Vec<A>, (), ()>
  where
    I: IntoIterator<Item = A> + 'static,
    I::IntoIter: Send + 'static,
  {
    let repr = Arc::clone(&self.repr);
    Effect::new_async(move |_r| {
      box_future(async move {
        let mut left = Vec::new();
        for v in iter {
          match &*repr {
            QueueRepr::Flume(f) => match f.offer_or_retain(v).await {
              Ok(()) => {}
              Err(v) => left.push(v),
            },
            QueueRepr::Sliding(s) => match s.offer_or_retain(v).await {
              Ok(()) => {}
              Err(v) => left.push(v),
            },
          }
        }
        Ok(left)
      })
    })
  }

  /// Block until a value is available or the queue disconnects.
  pub fn take(&self) -> Effect<A, QueueError, ()> {
    let repr = Arc::clone(&self.repr);
    Effect::new_async(move |_r| {
      box_future(async move {
        match &*repr {
          QueueRepr::Flume(f) => f.recv().await,
          QueueRepr::Sliding(s) => s.recv().await,
        }
      })
    })
  }

  /// Wait for at least one element, then drain all currently available elements.
  pub fn take_all(&self) -> Effect<Chunk<A>, QueueError, ()> {
    let repr = Arc::clone(&self.repr);
    Effect::new_async(move |_r| {
      box_future(async move {
        let first = match &*repr {
          QueueRepr::Flume(f) => f.recv().await?,
          QueueRepr::Sliding(s) => s.recv().await?,
        };
        let mut out = vec![first];
        loop {
          match match &*repr {
            QueueRepr::Flume(f) => f.try_recv(),
            QueueRepr::Sliding(s) => s.try_recv(),
          } {
            Ok(None) => break,
            Ok(Some(v)) => out.push(v),
            Err(e) => return Err(e),
          }
        }
        Ok(Chunk::from_vec(out))
      })
    })
  }

  /// After the first element arrives, take at most `n` elements total (including the first).
  pub fn take_up_to(&self, n: usize) -> Effect<Chunk<A>, QueueError, ()> {
    let repr = Arc::clone(&self.repr);
    Effect::new_async(move |_r| {
      box_future(async move {
        if n == 0 {
          return Ok(Chunk::empty());
        }
        let first = match &*repr {
          QueueRepr::Flume(f) => f.recv().await?,
          QueueRepr::Sliding(s) => s.recv().await?,
        };
        let mut out = vec![first];
        while out.len() < n {
          match match &*repr {
            QueueRepr::Flume(f) => f.try_recv(),
            QueueRepr::Sliding(s) => s.try_recv(),
          } {
            Ok(None) => break,
            Ok(Some(v)) => out.push(v),
            Err(e) => return Err(e),
          }
        }
        Ok(Chunk::from_vec(out))
      })
    })
  }

  /// Block until `n` separate receives complete (or disconnect).
  pub fn take_n(&self, n: usize) -> Effect<Chunk<A>, QueueError, ()> {
    let repr = Arc::clone(&self.repr);
    Effect::new_async(move |_r| {
      box_future(async move {
        if n == 0 {
          return Ok(Chunk::empty());
        }
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
          let v = match &*repr {
            QueueRepr::Flume(f) => f.recv().await?,
            QueueRepr::Sliding(s) => s.recv().await?,
          };
          out.push(v);
        }
        Ok(Chunk::from_vec(out))
      })
    })
  }

  /// Take between `min` and `max` elements inclusive (waits for `min` values unless disconnected).
  pub fn take_between(&self, min: usize, max: usize) -> Effect<Chunk<A>, QueueError, ()> {
    let repr = Arc::clone(&self.repr);
    Effect::new_async(move |_r| {
      box_future(async move {
        if min > max {
          return Ok(Chunk::empty());
        }
        if min == 0 && max == 0 {
          return Ok(Chunk::empty());
        }
        let mut out = Vec::new();
        for _ in 0..min {
          let v = match &*repr {
            QueueRepr::Flume(f) => f.recv().await?,
            QueueRepr::Sliding(s) => s.recv().await?,
          };
          out.push(v);
        }
        while out.len() < max {
          match match &*repr {
            QueueRepr::Flume(f) => f.try_recv(),
            QueueRepr::Sliding(s) => s.try_recv(),
          } {
            Ok(None) => break,
            Ok(Some(v)) => out.push(v),
            Err(e) => return Err(e),
          }
        }
        Ok(Chunk::from_vec(out))
      })
    })
  }

  /// Non-blocking receive.
  pub fn poll(&self) -> Effect<Option<A>, QueueError, ()> {
    let repr = Arc::clone(&self.repr);
    Effect::new_async(move |_r| {
      box_future(async move {
        tokio::task::yield_now().await;
        match &*repr {
          QueueRepr::Flume(f) => f.try_recv(),
          QueueRepr::Sliding(s) => s.try_recv(),
        }
      })
    })
  }

  /// Queued element count (approximate for flume).
  pub fn size(&self) -> Effect<usize, (), ()> {
    let repr = Arc::clone(&self.repr);
    Effect::new_async(move |_r| {
      box_future(async move {
        match &*repr {
          QueueRepr::Flume(f) => Ok(f.len()),
          QueueRepr::Sliding(s) => Ok(s.len().await),
        }
      })
    })
  }

  /// `true` when there are no queued elements.
  pub fn is_empty(&self) -> Effect<bool, (), ()> {
    let repr = Arc::clone(&self.repr);
    Effect::new_async(move |_r| {
      box_future(async move {
        match &*repr {
          QueueRepr::Flume(f) => Ok(f.is_empty()),
          QueueRepr::Sliding(s) => Ok(s.is_empty().await),
        }
      })
    })
  }

  /// `true` when at capacity (bounded / sliding) or shut down (flume side).
  pub fn is_full(&self) -> Effect<bool, (), ()> {
    let repr = Arc::clone(&self.repr);
    Effect::new_async(move |_r| {
      box_future(async move {
        match &*repr {
          QueueRepr::Flume(f) => Ok(f.is_full()),
          QueueRepr::Sliding(s) => Ok(s.is_full().await),
        }
      })
    })
  }

  /// Close the queue to producers; buffered values remain available to receivers.
  pub fn shutdown(&self) -> Effect<(), (), ()> {
    let repr = Arc::clone(&self.repr);
    Effect::new_async(move |_r| {
      box_future(async move {
        match &*repr {
          QueueRepr::Flume(f) => f.shutdown().await,
          QueueRepr::Sliding(s) => s.shutdown().await,
        }
        Ok(())
      })
    })
  }

  /// `true` after [`Self::shutdown`] has completed.
  pub fn is_shutdown(&self) -> Effect<bool, (), ()> {
    let repr = Arc::clone(&self.repr);
    Effect::new_async(move |_r| {
      box_future(async move {
        match &*repr {
          QueueRepr::Flume(f) => Ok(f.is_shutdown()),
          QueueRepr::Sliding(s) => Ok(s.is_shutdown()),
        }
      })
    })
  }

  /// Completes after [`Self::shutdown`] has been observed.
  pub fn await_shutdown(&self) -> Effect<(), (), ()> {
    let repr = Arc::clone(&self.repr);
    Effect::new_async(move |_r| {
      box_future(async move {
        match &*repr {
          QueueRepr::Flume(f) => f.await_shutdown().await,
          QueueRepr::Sliding(s) => s.await_shutdown().await,
        }
        Ok(())
      })
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::runtime::run_async;

  fn drive<A: 'static, E: 'static, R: 'static>(eff: Effect<A, E, R>, env: R) -> Result<A, E> {
    pollster::block_on(run_async(eff, env))
  }

  #[test]
  fn queue_take_suspends_until_offer() {
    let q = drive(Queue::<u32>::bounded(4), ()).unwrap();
    let q2 = q.clone();
    let h = std::thread::spawn(move || {
      std::thread::sleep(std::time::Duration::from_millis(20));
      drive(q2.offer(7u32), ()).unwrap();
    });
    let v = drive(q.take(), ()).unwrap();
    h.join().unwrap();
    assert_eq!(v, 7);
  }

  #[test]
  fn queue_bounded_offer_returns_false_when_full() {
    let q = drive(Queue::<u32>::bounded(1), ()).unwrap();
    assert!(drive(q.offer(1u32), ()).unwrap());
    assert!(!drive(q.offer(2u32), ()).unwrap());
    assert_eq!(drive(q.take(), ()).unwrap(), 1);
    assert!(drive(q.offer(3u32), ()).unwrap());
  }

  #[test]
  fn queue_dropping_drops_newest() {
    let q = drive(Queue::<u32>::dropping(1), ()).unwrap();
    assert!(drive(q.offer(1u32), ()).unwrap());
    assert!(!drive(q.offer(2u32), ()).unwrap());
    assert_eq!(drive(q.size(), ()).unwrap(), 1);
    assert_eq!(drive(q.take(), ()).unwrap(), 1);
  }

  #[test]
  fn queue_sliding_drops_oldest() {
    let q = drive(Queue::<u32>::sliding(2), ()).unwrap();
    assert!(drive(q.offer(1u32), ()).unwrap());
    assert!(drive(q.offer(2u32), ()).unwrap());
    assert!(drive(q.offer(3u32), ()).unwrap());
    assert_eq!(drive(q.take(), ()).unwrap(), 2);
    assert_eq!(drive(q.take(), ()).unwrap(), 3);
  }

  #[test]
  fn queue_await_shutdown_returns_after_shutdown() {
    let q = drive(Queue::<u32>::unbounded(), ()).unwrap();
    let q2 = q.clone();
    let h = std::thread::spawn(move || {
      std::thread::sleep(std::time::Duration::from_millis(15));
      drive(q2.shutdown(), ()).unwrap();
    });
    drive(q.await_shutdown(), ()).unwrap();
    h.join().unwrap();
    assert!(drive(q.is_shutdown(), ()).unwrap());
  }

  #[test]
  fn queue_offer_all_retains_overflow_bounded() {
    let q = drive(Queue::<u32>::bounded(2), ()).unwrap();
    let left = drive(q.offer_all([1u32, 2, 3, 4]), ()).unwrap();
    assert_eq!(left, vec![3, 4]);
    let chunk = drive(q.take_all(), ()).unwrap();
    assert_eq!(chunk.into_vec(), vec![1, 2]);
  }

  #[test]
  fn queue_take_up_to_and_take_n() {
    let q = drive(Queue::<u32>::bounded(10), ()).unwrap();
    drive(q.offer_all([1u32, 2, 3]), ()).unwrap();
    let c = drive(q.take_up_to(2), ()).unwrap();
    assert_eq!(c.into_vec(), vec![1, 2]);
    drive(q.offer_all([4u32, 5]), ()).unwrap();
    let c2 = drive(q.take_n(2), ()).unwrap();
    assert_eq!(c2.into_vec(), vec![3, 4]);
  }

  #[test]
  fn queue_take_between_min_max_and_edges() {
    let q = drive(Queue::<u32>::unbounded(), ()).unwrap();
    assert_eq!(drive(q.take_between(2, 1), ()).unwrap().len(), 0);
    assert_eq!(drive(q.take_between(0, 0), ()).unwrap().len(), 0);
    drive(q.offer_all([10u32, 11, 12]), ()).unwrap();
    let c = drive(q.take_between(2, 3), ()).unwrap();
    assert_eq!(c.len(), 3);
  }

  #[test]
  fn queue_poll_and_is_empty_is_full() {
    let q = drive(Queue::<u32>::bounded(1), ()).unwrap();
    assert_eq!(drive(q.poll(), ()).unwrap(), None);
    assert!(drive(q.is_empty(), ()).unwrap());
    drive(q.offer(7u32), ()).unwrap();
    assert!(drive(q.is_full(), ()).unwrap());
    assert_eq!(drive(q.poll(), ()).unwrap(), Some(7));
  }

  #[test]
  fn queue_sliding_is_full_after_capacity() {
    let q = drive(Queue::<u32>::sliding(2), ()).unwrap();
    drive(q.offer_all([1u32, 2, 3]), ()).unwrap();
    assert!(drive(q.is_full(), ()).unwrap());
  }
}
