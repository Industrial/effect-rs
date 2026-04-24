//! Effect.ts-inspired `Stream` API.
//!
//! Stream v2 adds a pull contract (`poll_next_chunk`) while preserving the existing facade.
//! Wave 7: the **output** side of any [`crate::coordination::channel::Channel`]`<A, …, …, E, R>` is
//! consumed as a stream of `A` with upstream/read failures [`crate::ChannelReadError`] via
//! [`Stream::from_channel`] / [`crate::coordination::channel::Channel::to_stream`].
//! Duplex [`crate::coordination::channel::QueueChannel`]`<A, A, _>` uses [`Stream::from_duplex_queue_channel`] /
//! [`crate::coordination::channel::QueueChannel::to_stream`] with [`QueueError`] (call
//! [`crate::coordination::channel::QueueChannel::shutdown`] to end a drain). A dedicated
//! incremental `StreamState` arm is deferred while `Stream` stays generic over `E` without a
//! `QueueError` error channel.

use crate::collections::sorted_map::EffectSortedMap;
use crate::coordination::pubsub::PubSub;
use crate::coordination::queue::{Queue, QueueError};
use crate::coordination::semaphore::Semaphore;
use crate::observability::metric::Metric;
use crate::resource::scope::Scope;
use crate::runtime::CancellationToken;
use crate::{Chunk, Effect, Or, Predicate};
use core::any::Any;
use core::fmt;
use futures::stream::{self, StreamExt};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Ordered window-start → aggregation state, for time-keyed stream operators (tumbling / sliding).
pub type TimeBucketMap<A> = EffectSortedMap<Instant, A>;

/// Update the bucket at `window_start` with `f` (`None` if the bucket did not exist yet).
#[inline]
pub fn merge_time_bucket<A: Clone>(
  map: TimeBucketMap<A>,
  window_start: Instant,
  f: impl FnOnce(Option<A>) -> A,
) -> TimeBucketMap<A> {
  crate::collections::sorted_map::modify(map, window_start, |old| Some(f(old)))
}

#[allow(clippy::type_complexity)]
enum StreamState<A, E, R>
where
  A: Send + 'static,
  E: Send + 'static,
  R: 'static,
{
  Pending(Option<Box<dyn FnOnce(&mut R) -> crate::kernel::BoxFuture<'_, Result<Vec<A>, E>>>>),
  Channel {
    queue: Queue<ChannelMessage<A, E>>,
    buffered: VecDeque<A>,
    closed: bool,
  },
  /// Items from a [`Queue<A>`] (e.g. [`PubSub::subscribe`]); optional upstream error in `shared_fail`.
  DirectQueue {
    queue: Queue<A>,
    buffered: VecDeque<A>,
    closed: bool,
    scope: Option<Scope>,
    shared_fail: Arc<Mutex<Option<E>>>,
  },
  Buffered(VecDeque<A>),
  /// [`Stream::chain`]: drain `first` to completion, then drain `second` incrementally.
  Chaining {
    first: Option<Stream<A, E, R>>,
    second: Stream<A, E, R>,
  },
  Exhausted,
}

#[derive(Clone)]
enum ChannelMessage<A, E> {
  Chunk(Chunk<A>),
  End,
  Fail(E),
}

/// How [`stream_from_channel_with_policy`] behaves when the internal queue is full.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackpressurePolicy {
  /// Block the producer until space is available.
  BoundedBlock,
  /// Drop the newly offered item when full.
  DropNewest,
  /// Evict the oldest queued item to make room.
  DropOldest,
  /// Surface failure to the producer instead of blocking or dropping.
  Fail,
}

/// Concrete action chosen for one enqueue attempt given policy and fill level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackpressureDecision {
  /// Store the item in the queue.
  Enqueue,
  /// Producer should wait and retry.
  Block,
  /// Discard the new item.
  DropNewest,
  /// Remove the oldest item then accept the new one.
  DropOldest,
  /// Propagate full-queue failure to the caller.
  Fail,
}

/// Maps `(policy, queue_len, capacity)` to a [`BackpressureDecision`] (capacity `< 1` treated as `1`).
#[inline]
pub fn backpressure_decision(
  policy: BackpressurePolicy,
  queue_len: usize,
  capacity: usize,
) -> BackpressureDecision {
  let bounded_capacity = capacity.max(1);
  if queue_len < bounded_capacity {
    return BackpressureDecision::Enqueue;
  }
  match policy {
    BackpressurePolicy::BoundedBlock => BackpressureDecision::Block,
    BackpressurePolicy::DropNewest => BackpressureDecision::DropNewest,
    BackpressurePolicy::DropOldest => BackpressureDecision::DropOldest,
    BackpressurePolicy::Fail => BackpressureDecision::Fail,
  }
}

/// Error when [`send_chunk`] cannot enqueue because the channel is full and
/// [`BackpressurePolicy::Fail`] is in effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamChannelFull;

impl fmt::Display for StreamChannelFull {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "stream channel is full (backpressure policy Fail)")
  }
}

impl std::error::Error for StreamChannelFull {}

/// Producer handle for a [`Stream`] built with [`stream_from_channel`] / [`stream_from_channel_with_policy`].
#[derive(Clone)]
pub struct StreamSender<A, E>
where
  A: Send + 'static,
  E: Send + 'static,
{
  queue: Queue<ChannelMessage<A, E>>,
  policy: BackpressurePolicy,
}

impl<A, E> StreamSender<A, E>
where
  A: Send + Clone + 'static,
  E: Send + Clone + 'static,
{
  /// Enqueues a terminal failure for consumers; returns `false` if the queue cannot accept it.
  pub fn fail(&self, error: E) -> bool {
    let msg = ChannelMessage::Fail(error);
    loop {
      match crate::runtime::run_blocking(self.queue.offer(msg.clone()), ()) {
        Ok(true) => return true,
        Ok(false) => std::thread::yield_now(),
        Err(()) => return false,
      }
    }
  }
}

/// Pull-based async stream of `A` with error type `E` and environment `R`.
pub struct Stream<A, E = (), R = ()>
where
  A: Send + 'static,
  E: Send + 'static,
  R: 'static,
{
  state: Arc<Mutex<StreamState<A, E, R>>>,
  /// When set, [`Stream::poll_next_chunk`] increments this counter by the number of items in each yielded chunk (for throughput / element-rate observability).
  throughput: Option<Metric<u64, ()>>,
}

/// Compatibility facade preserving the v1 public stream type name.
pub type StreamV1<A, E = (), R = ()> = Stream<A, E, R>;

/// `(consumer streams, pump effect)` from [`Stream::broadcast`].
pub type StreamBroadcastFanout<A, E, R> = (Vec<Stream<A, E, R>>, Effect<(), E, R>);

impl<A, E, R> Stream<A, E, R>
where
  A: Send + 'static,
  E: Send + 'static,
  R: 'static,
{
  /// Stream that resolves its first pull from `f` once, buffers the remainder, then exhausts.
  #[inline]
  pub fn new<F>(f: F) -> Self
  where
    F: FnOnce(&mut R) -> crate::kernel::BoxFuture<'_, Result<Vec<A>, E>> + 'static,
  {
    Self {
      state: Arc::new(Mutex::new(StreamState::Pending(Some(Box::new(f))))),
      throughput: None,
    }
  }

  /// Attach a counter incremented by the item count of every chunk successfully returned from [`Stream::poll_next_chunk`].
  ///
  /// Combine with wall-clock samples to derive items-per-second. Does not count failed polls.
  #[must_use]
  pub fn with_throughput_metric(mut self, metric: Metric<u64, ()>) -> Self {
    self.throughput = Some(metric);
    self
  }

  /// Pull the next chunk from the stream. `Ok(None)` means the stream is exhausted.
  pub fn poll_next_chunk<'a>(
    &mut self,
    env: &'a mut R,
  ) -> crate::kernel::BoxFuture<'a, Result<Option<Chunk<A>>, E>> {
    self.poll_next_chunk_with_size(env, 64)
  }

  fn poll_next_chunk_with_size<'a>(
    &mut self,
    env: &'a mut R,
    chunk_size: usize,
  ) -> crate::kernel::BoxFuture<'a, Result<Option<Chunk<A>>, E>> {
    let state = self.state.clone();
    let throughput = self.throughput.clone();
    // `guard` is dropped before every `.await` (see Channel recv + Pending arms); clippy's
    // CFG does not always prove that across the `match`.
    #[allow(clippy::await_holding_lock)]
    Box::pin(async move {
      if chunk_size == 0 {
        return Ok(None);
      }

      loop {
        let mut guard = state.lock().expect("stream state mutex poisoned");
        match &mut *guard {
          StreamState::Channel {
            queue,
            buffered,
            closed,
          } => {
            if buffered.is_empty() && !*closed {
              let q = queue.clone();
              drop(guard);
              let message = q.take().run(&mut ()).await;
              let mut guard = state.lock().expect("stream state mutex poisoned");
              match &mut *guard {
                StreamState::Channel {
                  buffered, closed, ..
                } => match message {
                  Ok(ChannelMessage::Chunk(chunk)) => buffered.extend(chunk.into_vec()),
                  Ok(ChannelMessage::End) => *closed = true,
                  Ok(ChannelMessage::Fail(error)) => return Err(error),
                  Err(QueueError::Disconnected) => *closed = true,
                },
                _ => return Ok(None),
              }
              continue;
            }

            if buffered.is_empty() && *closed {
              *guard = StreamState::Exhausted;
              return Ok(None);
            }

            let count = buffered.len().min(chunk_size);
            let mut out = Vec::with_capacity(count);
            for _ in 0..count {
              if let Some(v) = buffered.pop_front() {
                out.push(v);
              }
            }
            let n = out.len() as u64;
            if let Some(m) = &throughput {
              match m.apply(n).run(&mut ()).await {
                Ok(()) => {}
                Err(never) => match never {},
              }
            }
            return Ok(Some(Chunk::from_vec(out)));
          }
          StreamState::DirectQueue {
            queue,
            buffered,
            closed,
            scope,
            shared_fail,
          } => {
            if let Some(e) = shared_fail
              .lock()
              .expect("shared_fail mutex poisoned")
              .take()
            {
              return Err(e);
            }
            if buffered.is_empty() && !*closed {
              let q = queue.clone();
              drop(guard);
              let recv = q.take().run(&mut ()).await;
              let mut guard = state.lock().expect("stream state mutex poisoned");
              match &mut *guard {
                StreamState::DirectQueue {
                  buffered, closed, ..
                } => match recv {
                  Ok(a) => buffered.push_back(a),
                  Err(QueueError::Disconnected) => *closed = true,
                },
                _ => return Ok(None),
              }
              continue;
            }

            if buffered.is_empty() && *closed {
              if let Some(s) = scope.take() {
                let _ = s.close();
              }
              *guard = StreamState::Exhausted;
              return Ok(None);
            }

            let count = buffered.len().min(chunk_size);
            let mut out = Vec::with_capacity(count);
            for _ in 0..count {
              if let Some(v) = buffered.pop_front() {
                out.push(v);
              }
            }
            let n = out.len() as u64;
            if let Some(m) = &throughput {
              match m.apply(n).run(&mut ()).await {
                Ok(()) => {}
                Err(never) => match never {},
              }
            }
            return Ok(Some(Chunk::from_vec(out)));
          }
          StreamState::Chaining { first, second } => {
            if let Some(mut fs) = first.take() {
              drop(guard);
              match fs.poll_next_chunk_with_size(env, chunk_size).await {
                Ok(Some(chunk)) => {
                  let mut g = state.lock().expect("stream state mutex poisoned");
                  if let StreamState::Chaining { first, .. } = &mut *g {
                    *first = Some(fs);
                  }
                  drop(g);
                  let n = chunk.len() as u64;
                  if let Some(m) = &throughput {
                    match m.apply(n).run(&mut ()).await {
                      Ok(()) => {}
                      Err(never) => match never {},
                    }
                  }
                  return Ok(Some(chunk));
                }
                Err(e) => {
                  let mut g = state.lock().expect("stream state mutex poisoned");
                  if let StreamState::Chaining { first, .. } = &mut *g {
                    *first = Some(fs);
                  }
                  return Err(e);
                }
                Ok(None) => {
                  continue;
                }
              }
            }
            let out = second.poll_next_chunk_with_size(env, chunk_size).await;
            match &out {
              Ok(Some(chunk)) => {
                let n = chunk.len() as u64;
                if let Some(m) = &throughput {
                  match m.apply(n).run(&mut ()).await {
                    Ok(()) => {}
                    Err(never) => match never {},
                  }
                }
              }
              Ok(None) => {
                *guard = StreamState::Exhausted;
              }
              Err(_) => {}
            }
            return out;
          }
          StreamState::Buffered(items) => {
            if items.is_empty() {
              *guard = StreamState::Exhausted;
              return Ok(None);
            }
            let count = items.len().min(chunk_size);
            let mut out = Vec::with_capacity(count);
            for _ in 0..count {
              if let Some(v) = items.pop_front() {
                out.push(v);
              }
            }
            if items.is_empty() {
              *guard = StreamState::Exhausted;
            }
            let n = out.len() as u64;
            if let Some(m) = &throughput {
              match m.apply(n).run(&mut ()).await {
                Ok(()) => {}
                Err(never) => match never {},
              }
            }
            return Ok(Some(Chunk::from_vec(out)));
          }
          StreamState::Exhausted => return Ok(None),
          StreamState::Pending(make) => {
            let make = make.take();
            drop(guard);
            let Some(make) = make else {
              *state.lock().expect("stream state mutex poisoned") = StreamState::Exhausted;
              return Ok(None);
            };

            let items = make(env).await?;
            if items.is_empty() {
              *state.lock().expect("stream state mutex poisoned") = StreamState::Exhausted;
              return Ok(None);
            }

            let mut queue = VecDeque::from(items);
            let count = queue.len().min(chunk_size);
            let mut out = Vec::with_capacity(count);
            for _ in 0..count {
              if let Some(v) = queue.pop_front() {
                out.push(v);
              }
            }
            if queue.is_empty() {
              *state.lock().expect("stream state mutex poisoned") = StreamState::Exhausted;
            } else {
              *state.lock().expect("stream state mutex poisoned") = StreamState::Buffered(queue);
            }
            let n = out.len() as u64;
            if let Some(m) = &throughput {
              match m.apply(n).run(&mut ()).await {
                Ok(()) => {}
                Err(never) => match never {},
              }
            }
            return Ok(Some(Chunk::from_vec(out)));
          }
        }
      }
    })
  }

  /// Eagerly runs `effect` to obtain all elements (same shape as [`Stream::new`]).
  #[inline]
  pub fn from_effect(effect: Effect<Vec<A>, E, R>) -> Self {
    Self::new(move |r| effect.run(r))
  }

  /// Concatenate: yields all chunks from `self`, then all chunks from `second` (incremental; supports
  /// a long or unbounded second stream).
  #[must_use]
  pub fn chain(self, second: Self) -> Self {
    Self {
      state: Arc::new(Mutex::new(StreamState::Chaining {
        first: Some(self),
        second,
      })),
      throughput: None,
    }
  }

  /// Any [`Channel`](crate::coordination::channel::Channel) as an output [`Stream`] (Wave 7).
  ///
  /// Drains via [`Channel::read`](crate::coordination::channel::Channel::read). [`Channel::to_stream`](crate::coordination::channel::Channel::to_stream) forwards to this constructor.
  #[inline]
  #[must_use]
  pub fn from_channel<InElem, OutDone>(
    ch: crate::coordination::channel::Channel<A, InElem, OutDone, E, R>,
  ) -> Stream<A, crate::ChannelReadError<E>, R>
  where
    A: Clone,
    InElem: Send + 'static,
    OutDone: 'static,
  {
    let ch = ch.clone();
    Stream::new(move |env: &mut R| {
      crate::box_future(async move {
        let mut out = Vec::new();
        loop {
          match ch.read().run(env).await {
            Ok(None) => break,
            Ok(Some(x)) => out.push(x),
            Err(e) => return Err(e),
          }
        }
        Ok(out)
      })
    })
  }

  /// Generates elements by repeatedly running `f` on state until it returns `None`.
  #[inline]
  pub fn unfold_effect<S, F>(init: S, mut f: F) -> Stream<A, E, R>
  where
    S: 'static,
    F: FnMut(S) -> Effect<Option<(A, S)>, E, R> + 'static,
  {
    Self::new(move |r: &mut R| {
      Box::pin(async move {
        let mut state = Some(init);
        let mut out = Vec::new();
        while let Some(s) = state.take() {
          match f(s).run(r).await? {
            Some((a, s2)) => {
              out.push(a);
              state = Some(s2);
            }
            None => break,
          }
        }
        Ok(out)
      })
    })
  }

  /// Drains the stream into a single vector (stops early if `R` embeds a cancelled [`CancellationToken`]).
  #[inline]
  pub fn run_collect(self) -> Effect<Vec<A>, E, R> {
    let mut stream = self;
    Effect::new_async(move |r: &mut R| {
      Box::pin(async move {
        let mut out = Vec::new();
        loop {
          if interruption_requested(r) {
            break;
          }
          match stream.poll_next_chunk(r).await? {
            Some(chunk) => out.extend(chunk.into_vec()),
            None => break,
          }
        }
        Ok(out)
      })
    })
  }

  /// Runs `f` for each element (effectful side channel).
  #[inline]
  pub fn run_for_each_effect<F>(self, mut f: F) -> Effect<(), E, R>
  where
    F: FnMut(A) -> Effect<(), E, R> + 'static,
  {
    let mut stream = self;
    Effect::new_async(move |r: &mut R| {
      Box::pin(async move {
        loop {
          if interruption_requested(r) {
            break;
          }
          let Some(chunk) = stream.poll_next_chunk(r).await? else {
            break;
          };
          for item in chunk.into_vec() {
            f(item).run(r).await?;
          }
        }
        Ok(())
      })
    })
  }

  /// Alias of [`Stream::run_for_each_effect`].
  #[inline]
  pub fn run_for_each<F>(self, f: F) -> Effect<(), E, R>
  where
    F: FnMut(A) -> Effect<(), E, R> + 'static,
  {
    self.run_for_each_effect(f)
  }

  /// Left fold over all elements with a pure step function.
  #[inline]
  pub fn run_fold<B, F>(self, init: B, mut f: F) -> Effect<B, E, R>
  where
    B: 'static,
    F: FnMut(B, A) -> B + 'static,
  {
    let mut stream = self;
    Effect::new_async(move |r: &mut R| {
      Box::pin(async move {
        let mut acc = init;
        loop {
          if interruption_requested(r) {
            break;
          }
          let Some(chunk) = stream.poll_next_chunk(r).await? else {
            break;
          };
          for item in chunk.into_vec() {
            acc = f(acc, item);
          }
        }
        Ok(acc)
      })
    })
  }

  /// Left fold where each step is an [`Effect`].
  #[inline]
  pub fn run_fold_effect<B, F>(self, init: B, mut f: F) -> Effect<B, E, R>
  where
    B: 'static,
    F: FnMut(B, A) -> Effect<B, E, R> + 'static,
  {
    let mut stream = self;
    Effect::new_async(move |r: &mut R| {
      Box::pin(async move {
        let mut acc = init;
        loop {
          if interruption_requested(r) {
            break;
          }
          let Some(chunk) = stream.poll_next_chunk(r).await? else {
            break;
          };
          for item in chunk.into_vec() {
            acc = f(acc, item).run(r).await?;
          }
        }
        Ok(acc)
      })
    })
  }

  /// Maps each element (pulls upstream to completion, then emits mapped chunks).
  #[inline]
  pub fn map<B, F>(self, mut f: F) -> Stream<B, E, R>
  where
    B: Send + 'static,
    F: FnMut(A) -> B + 'static,
  {
    Stream::new(move |r: &mut R| {
      Box::pin(async move {
        let mut upstream = self;
        let mut out = Vec::new();
        loop {
          let Some(chunk) = upstream.poll_next_chunk(r).await? else {
            break;
          };
          out.extend(chunk.map(&mut f).into_vec());
        }
        Ok(out)
      })
    })
  }

  /// Retains elements satisfying `p`.
  #[inline]
  pub fn filter(self, p: Predicate<A>) -> Stream<A, E, R> {
    Stream::new(move |r: &mut R| {
      Box::pin(async move {
        let mut upstream = self;
        let mut out = Vec::new();
        loop {
          let Some(chunk) = upstream.poll_next_chunk(r).await? else {
            break;
          };
          out.extend(chunk.into_vec().into_iter().filter(|item| p(item)));
        }
        Ok(out)
      })
    })
  }

  /// Yields elements from the start of the stream while `p` holds; ends before the first failure.
  #[inline]
  pub fn take_while(self, p: Predicate<A>) -> Stream<A, E, R> {
    Stream::new(move |r: &mut R| {
      Box::pin(async move {
        let mut upstream = self;
        let mut out = Vec::new();
        'drain: loop {
          let Some(chunk) = upstream.poll_next_chunk(r).await? else {
            break;
          };
          for item in chunk.into_vec() {
            if !p(&item) {
              break 'drain;
            }
            out.push(item);
          }
        }
        Ok(out)
      })
    })
  }

  /// Drops elements from the start while `p` holds, then yields the remainder.
  #[inline]
  pub fn drop_while(self, p: Predicate<A>) -> Stream<A, E, R> {
    Stream::new(move |r: &mut R| {
      Box::pin(async move {
        let mut upstream = self;
        let mut out = Vec::new();
        let mut skipping = true;
        loop {
          let Some(chunk) = upstream.poll_next_chunk(r).await? else {
            break;
          };
          for item in chunk.into_vec() {
            if skipping {
              if p(&item) {
                continue;
              }
              skipping = false;
              out.push(item);
            } else {
              out.push(item);
            }
          }
        }
        Ok(out)
      })
    })
  }

  /// At most `n` elements from the start of the stream.
  #[inline]
  pub fn take(self, n: usize) -> Stream<A, E, R> {
    Stream::new(move |r: &mut R| {
      Box::pin(async move {
        let mut upstream = self;
        let mut remaining = n;
        let mut out = Vec::new();
        while remaining > 0 {
          let Some(chunk) = upstream.poll_next_chunk(r).await? else {
            break;
          };
          let mut items = chunk.into_vec();
          if items.len() > remaining {
            items.truncate(remaining);
          }
          remaining = remaining.saturating_sub(items.len());
          out.extend(items);
        }
        Ok(out)
      })
    })
  }

  /// Chunks into vectors of length `size` (last chunk may be shorter; `size == 0` yields empty output).
  #[inline]
  pub fn grouped(self, size: usize) -> Stream<Vec<A>, E, R> {
    Stream::new(move |r: &mut R| {
      Box::pin(async move {
        if size == 0 {
          return Ok(Vec::new());
        }
        let mut upstream = self;
        let mut out: Vec<Vec<A>> = Vec::new();
        let mut cur: Vec<A> = Vec::with_capacity(size);
        loop {
          let Some(chunk) = upstream.poll_next_chunk(r).await? else {
            break;
          };
          for item in chunk.into_vec() {
            cur.push(item);
            if cur.len() == size {
              out.push(core::mem::take(&mut cur));
            }
          }
        }
        if !cur.is_empty() {
          out.push(cur);
        }
        Ok(out)
      })
    })
  }

  /// Fan this stream out to `branches` independent consumers via [`PubSub`] (sliding ring of size `n`).
  ///
  /// Returns `(streams, pump)`. Run `pump` concurrently with pulls on the returned streams (same
  /// environment `R` as the pump); the pump forwards upstream chunks into the hub and shuts it down
  /// when the source ends or fails.
  ///
  /// If the upstream fails, the error is stored for branches; the first branch that polls after the
  /// failure [`take`](Option::take)s it from the shared slot (others may end without seeing `Err`).
  #[inline]
  pub fn broadcast(self, n: usize, branches: usize) -> Effect<StreamBroadcastFanout<A, E, R>, E, R>
  where
    A: Clone,
  {
    Effect::new_async(move |_r: &mut R| {
      Box::pin(async move {
        if branches == 0 {
          let upstream = self;
          let pump = Effect::new_async(move |r2: &mut R| {
            Box::pin(async move {
              let mut upstream = upstream;
              while upstream.poll_next_chunk(r2).await?.is_some() {}
              Ok(())
            })
          });
          return Ok((Vec::new(), pump));
        }

        let cap = n.max(1);
        let ps = crate::runtime::run_blocking(PubSub::sliding(cap), ()).expect("pubsub sliding");
        let hub = Scope::make();
        let shared_fail: Arc<Mutex<Option<E>>> = Arc::new(Mutex::new(None));
        let mut outs = Vec::with_capacity(branches);
        for _ in 0..branches {
          let child = hub.fork();
          let q = match crate::runtime::run_async(ps.clone().subscribe(), child.clone()).await {
            Ok(q) => q,
            Err(e) => match e {},
          };
          outs.push(Stream {
            state: Arc::new(Mutex::new(StreamState::DirectQueue {
              queue: q,
              buffered: VecDeque::new(),
              closed: false,
              scope: Some(child),
              shared_fail: Arc::clone(&shared_fail),
            })),
            throughput: None,
          });
        }

        let upstream = self;
        let ps_pump = ps.clone();
        let shared_pump = Arc::clone(&shared_fail);
        let hub_pump = hub.clone();
        let pump = Effect::new_async(move |r2: &mut R| {
          let hub_pump = hub_pump.clone();
          let ps_pump = ps_pump.clone();
          let shared_pump = Arc::clone(&shared_pump);
          Box::pin(async move {
            let mut upstream = upstream;
            loop {
              match upstream.poll_next_chunk(r2).await {
                Ok(Some(chunk)) => {
                  for a in chunk.into_vec() {
                    let _ = crate::runtime::run_async(ps_pump.publish(a), ()).await;
                    // `Effect` steps often complete in one poll; without yielding, `tokio::join!`
                    // on the current-thread runtime may run the pump to `hub.close()` before
                    // consumer branches are polled.
                    tokio::task::yield_now().await;
                  }
                }
                Ok(None) => {
                  let _ = crate::runtime::run_async(ps_pump.shutdown(), ()).await;
                  let _ = hub_pump.close();
                  break;
                }
                Err(e) => {
                  *shared_pump.lock().expect("shared_fail mutex poisoned") = Some(e);
                  let _ = crate::runtime::run_async(ps_pump.shutdown(), ()).await;
                  let _ = hub_pump.close();
                  break;
                }
              }
            }
            Ok(())
          })
        });

        Ok((outs, pump))
      })
    })
  }

  /// Maps with an effect per element; errors widen to [`Or<E, E2>`].
  #[inline]
  pub fn map_effect<B, E2, F>(self, mut f: F) -> Stream<B, Or<E, E2>, R>
  where
    B: Send + 'static,
    E2: Send + 'static,
    F: FnMut(A) -> Effect<B, E2, R> + 'static,
  {
    Stream::new(move |r: &mut R| {
      Box::pin(async move {
        let mut upstream = self;
        let mut out = Vec::new();
        loop {
          let Some(chunk) = upstream.poll_next_chunk(r).await.map_err(Or::Left)? else {
            break;
          };
          for item in chunk.into_vec() {
            out.push(f(item).run(r).await.map_err(|e2| Or::Right(e2))?);
          }
        }
        Ok(out)
      })
    })
  }

  /// Map each element with `f` with at most `n` concurrent effect runs (via [`Semaphore`]).
  ///
  /// Drains the upstream stream first, then runs mappers in parallel with a permit cap of
  /// `max(n, 1)`. Output order matches the original stream order. Each mapper receives a **clone**
  /// of the environment `R`, so `R` must be [`Clone`], [`Send`], and [`Sync`].
  #[inline]
  pub fn map_par_n<B, F>(self, n: usize, f: F) -> Stream<B, E, R>
  where
    A: Send,
    B: Send + 'static,
    E: Send,
    R: Clone + Send + Sync,
    F: Fn(A) -> Effect<B, E, R> + Send + Sync + 'static,
  {
    let f = Arc::new(f);
    Stream::new(move |r: &mut R| {
      let f = Arc::clone(&f);
      let r_env = r.clone();
      Box::pin(async move {
        let permits = n.max(1);
        let sem = Arc::new(
          crate::runtime::run_async(Semaphore::make(permits), ())
            .await
            .expect("semaphore make"),
        );

        let mut upstream = self;
        let mut all_items = Vec::new();
        loop {
          let Some(chunk) = upstream.poll_next_chunk(r).await? else {
            break;
          };
          all_items.extend(chunk.into_vec());
        }

        let len = all_items.len();
        let results: Vec<Result<(usize, B), E>> =
          stream::iter(all_items.into_iter().enumerate().map(move |(idx, item)| {
            let sem = Arc::clone(&sem);
            let f = Arc::clone(&f);
            let mut env = r_env.clone();
            async move {
              let _permit = crate::runtime::run_async(sem.acquire_owned(), ())
                .await
                .unwrap_or_else(|e| match e {});
              f(item).run(&mut env).await.map(|b| (idx, b))
            }
          }))
          .buffer_unordered(permits)
          .collect()
          .await;

        let mut pairs = Vec::with_capacity(len);
        for res in results {
          match res {
            Ok(pair) => pairs.push(pair),
            Err(e) => return Err(e),
          }
        }
        pairs.sort_by_key(|(i, _)| *i);
        let out: Vec<B> = pairs.into_iter().map(|(_, b)| b).collect();
        debug_assert_eq!(out.len(), len);
        Ok(out)
      })
    })
  }

  /// Optional binary reduction over the stream (`None` if empty).
  #[inline]
  pub fn run_reduce<F>(self, mut f: F) -> Effect<Option<A>, E, R>
  where
    F: FnMut(A, A) -> A + 'static,
  {
    let mut stream = self;
    Effect::new_async(move |r: &mut R| {
      Box::pin(async move {
        let mut acc: Option<A> = None;
        loop {
          if interruption_requested(r) {
            break;
          }
          let Some(chunk) = stream.poll_next_chunk(r).await? else {
            break;
          };
          for item in chunk.into_vec() {
            acc = Some(match acc.take() {
              Some(current) => f(current, item),
              None => item,
            });
          }
        }
        Ok(acc)
      })
    })
  }

  /// Stateful map: emits `f(&mut state, item)` for each element.
  #[inline]
  pub fn scan<S, B, F>(self, state: S, mut f: F) -> Stream<B, E, R>
  where
    S: 'static,
    B: Send + 'static,
    F: FnMut(&mut S, A) -> B + 'static,
  {
    Stream::new(move |r: &mut R| {
      Box::pin(async move {
        let mut upstream = self;
        let mut s = state;
        let mut out = Vec::new();
        loop {
          let Some(chunk) = upstream.poll_next_chunk(r).await? else {
            break;
          };
          for item in chunk.into_vec() {
            out.push(f(&mut s, item));
          }
        }
        Ok(out)
      })
    })
  }
}

fn interruption_requested<R: 'static>(env: &R) -> bool {
  let any = env as &dyn Any;
  any
    .downcast_ref::<CancellationToken>()
    .is_some_and(CancellationToken::is_cancelled)
}

impl Stream<i64, (), ()> {
  /// Half-open range `[start, end_exclusive)` as a one-shot buffered stream.
  #[inline]
  pub fn range(start: i64, end_exclusive: i64) -> Self {
    Stream::new(move |_r: &mut ()| Box::pin(async move { Ok((start..end_exclusive).collect()) }))
  }
}

/// Builds a channel-backed stream and sender with explicit [`BackpressurePolicy`].
pub fn stream_from_channel_with_policy<A, E, R>(
  capacity: usize,
  policy: BackpressurePolicy,
) -> (Stream<A, E, R>, StreamSender<A, E>)
where
  A: Send + Clone + 'static,
  E: Send + Clone + 'static,
  R: 'static,
{
  let cap = capacity.max(1);
  let queue_effect = match policy {
    BackpressurePolicy::BoundedBlock | BackpressurePolicy::Fail => Queue::bounded(cap),
    BackpressurePolicy::DropNewest => Queue::dropping(cap),
    BackpressurePolicy::DropOldest => Queue::sliding(cap),
  };
  let queue =
    crate::runtime::run_blocking(queue_effect, ()).expect("stream queue construction must succeed");
  let sender = StreamSender {
    queue: queue.clone(),
    policy,
  };
  let stream = Stream {
    state: Arc::new(Mutex::new(StreamState::Channel {
      queue,
      buffered: VecDeque::new(),
      closed: false,
    })),
    throughput: None,
  };
  (stream, sender)
}

/// [`stream_from_channel_with_policy`] with [`BackpressurePolicy::BoundedBlock`].
pub fn stream_from_channel<A, E, R>(capacity: usize) -> (Stream<A, E, R>, StreamSender<A, E>)
where
  A: Send + Clone + 'static,
  E: Send + Clone + 'static,
  R: 'static,
{
  stream_from_channel_with_policy(capacity, BackpressurePolicy::BoundedBlock)
}

/// Enqueues a chunk for consumers; may block, drop, or fail per the sender’s policy.
pub fn send_chunk<A, E>(
  sender: &StreamSender<A, E>,
  chunk: Chunk<A>,
) -> Effect<(), StreamChannelFull, ()>
where
  A: Send + Clone + 'static,
  E: Send + Clone + 'static,
{
  let queue = sender.queue.clone();
  let policy = sender.policy;
  let msg = ChannelMessage::Chunk(chunk);
  Effect::new(move |_env: &mut ()| match policy {
    BackpressurePolicy::BoundedBlock => loop {
      match crate::runtime::run_blocking(queue.offer(msg.clone()), ()) {
        Ok(true) => return Ok(()),
        Ok(false) => std::thread::yield_now(),
        Err(()) => unreachable!("Queue::offer is infallible"),
      }
    },
    BackpressurePolicy::Fail => match crate::runtime::run_blocking(queue.offer(msg), ()) {
      Ok(true) => Ok(()),
      Ok(false) | Err(()) => Err(StreamChannelFull),
    },
    BackpressurePolicy::DropNewest | BackpressurePolicy::DropOldest => {
      match crate::runtime::run_blocking(queue.offer(msg), ()) {
        Ok(_) => Ok(()),
        Err(()) => unreachable!("Queue::offer is infallible"),
      }
    }
  })
}

/// Signals end-of-stream to consumers (blocks until accepted when using a blocking queue).
pub fn end_stream<A, E>(sender: StreamSender<A, E>) -> Effect<(), (), ()>
where
  A: Send + Clone + 'static,
  E: Send + Clone + 'static,
{
  let queue = sender.queue.clone();
  Effect::new(move |_env: &mut ()| {
    let msg = ChannelMessage::End;
    loop {
      match crate::runtime::run_blocking(queue.offer(msg.clone()), ()) {
        Ok(true) => return Ok(()),
        Ok(false) => std::thread::yield_now(),
        Err(()) => unreachable!("Queue::offer is infallible"),
      }
    }
  })
}

impl<A, E, R> Drop for Stream<A, E, R>
where
  A: Send + 'static,
  E: Send + 'static,
  R: 'static,
{
  fn drop(&mut self) {
    if let Ok(mut g) = self.state.lock()
      && let StreamState::DirectQueue { scope, .. } = &mut *g
      && let Some(s) = scope.take()
    {
      let _ = s.close();
    }
  }
}

impl<A, R> Stream<A, QueueError, R>
where
  A: Send + Clone + 'static,
  R: 'static,
{
  /// [`crate::coordination::channel::QueueChannel`] with the same input and output element type (duplex / identity map-in).
  ///
  /// Drains via [`crate::coordination::channel::QueueChannel::read`]; same bootstrap semantics
  /// as [`Stream::from_channel`]. Use [`crate::coordination::channel::QueueChannel::shutdown`]
  /// so the final [`crate::coordination::channel::QueueChannel::read`] returns `None` and collection can complete.
  #[inline]
  #[must_use]
  pub fn from_duplex_queue_channel(
    ch: crate::coordination::channel::QueueChannel<A, A, R>,
  ) -> Self {
    let ch = ch.clone();
    Stream::new(move |env: &mut R| {
      crate::box_future(async move {
        let mut out = Vec::new();
        loop {
          match ch.read().run(env).await {
            Ok(None) => break,
            Ok(Some(x)) => out.push(x),
            Err(e) => return Err(e),
          }
        }
        Ok(out)
      })
    })
  }
}

impl<A: Send + 'static> Stream<A, (), ()> {
  /// Finite stream from any iterator (materialized once).
  #[inline]
  pub fn from_iterable<I>(iter: I) -> Self
  where
    I: IntoIterator<Item = A> + 'static,
  {
    let v: Vec<A> = iter.into_iter().collect();
    Stream::new(move |_r: &mut ()| Box::pin(async move { Ok(v) }))
  }

  /// Pure [`Stream::unfold_effect`]: expands state with `f` until `None`.
  #[inline]
  pub fn unfold<S, F>(init: S, mut f: F) -> Self
  where
    S: 'static,
    F: FnMut(S) -> Option<(A, S)> + 'static,
  {
    Stream::new(move |_r: &mut ()| {
      Box::pin(async move {
        let mut state = Some(init);
        let mut out: Vec<A> = Vec::new();
        while let Some(s) = state.take() {
          match f(s) {
            Some((a, s2)) => {
              out.push(a);
              state = Some(s2);
            }
            None => break,
          }
        }
        Ok(out)
      })
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{fail, succeed};
  use core::future::Future;
  use core::task::{Context, Poll, Waker};
  use rstest::rstest;
  use std::sync::Arc;
  use std::task::Wake;
  use std::thread;

  struct ThreadUnpark(thread::Thread);
  impl Wake for ThreadUnpark {
    fn wake(self: Arc<Self>) {
      self.0.unpark();
    }
  }

  fn block_on<F: Future>(fut: F) -> F::Output {
    let mut fut = Box::pin(fut);
    let waker = Waker::from(Arc::new(ThreadUnpark(thread::current())));
    let mut cx = Context::from_waker(&waker);
    loop {
      match fut.as_mut().poll(&mut cx) {
        Poll::Ready(v) => return v,
        Poll::Pending => thread::park(),
      }
    }
  }

  mod time_window_buckets {
    use super::*;
    use crate::collections::sorted_map;
    use std::time::Duration;

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct AggState {
      sum: i32,
      cnt: usize,
    }

    #[test]
    fn merge_time_bucket_orders_keys_for_ordered_iteration() {
      let t0 = Instant::now();
      let t1 = t0 + Duration::from_secs(1);
      let t2 = t0 + Duration::from_secs(2);
      let m = sorted_map::empty();
      let m = merge_time_bucket(m, t2, |_| AggState { sum: 2, cnt: 1 });
      let m = merge_time_bucket(m, t0, |_| AggState { sum: 0, cnt: 1 });
      let m = merge_time_bucket(m, t1, |_| AggState { sum: 1, cnt: 1 });
      let keys: Vec<Instant> = m.iter().map(|(k, _)| *k).collect();
      assert_eq!(keys, vec![t0, t1, t2]);
    }

    #[test]
    fn merge_time_bucket_accumulates_same_window() {
      let t0 = Instant::now();
      let m = sorted_map::empty();
      let m = merge_time_bucket(m, t0, |_| AggState { sum: 3, cnt: 1 });
      let m = merge_time_bucket(m, t0, |o| {
        let a = o.expect("prior");
        AggState {
          sum: a.sum + 5,
          cnt: a.cnt + 1,
        }
      });
      assert_eq!(m.get(&t0), Some(&AggState { sum: 8, cnt: 2 }));
    }
  }

  mod constructors {
    use super::*;

    #[test]
    fn from_iterable_with_values_collects_values_in_original_order() {
      let stream = Stream::from_iterable([1, 2, 3]);
      let out = block_on(stream.run_collect().run(&mut ()));
      assert_eq!(out, Ok(vec![1, 2, 3]));
    }

    #[test]
    fn poll_next_chunk_with_chunk_size_returns_incremental_slices_then_none() {
      let mut stream = Stream::from_iterable([1, 2, 3, 4, 5]);
      let mut env = ();
      assert_eq!(
        block_on(stream.poll_next_chunk_with_size(&mut env, 2)),
        Ok(Some(Chunk::from_vec(vec![1, 2])))
      );
      assert_eq!(
        block_on(stream.poll_next_chunk_with_size(&mut env, 2)),
        Ok(Some(Chunk::from_vec(vec![3, 4])))
      );
      assert_eq!(
        block_on(stream.poll_next_chunk_with_size(&mut env, 2)),
        Ok(Some(Chunk::from_vec(vec![5])))
      );
      assert_eq!(
        block_on(stream.poll_next_chunk_with_size(&mut env, 2)),
        Ok(None)
      );
    }

    #[test]
    fn poll_next_chunk_with_zero_chunk_size_returns_none_without_consuming_stream() {
      let mut stream = Stream::from_iterable([1, 2, 3]);
      let mut env = ();
      assert_eq!(
        block_on(stream.poll_next_chunk_with_size(&mut env, 0)),
        Ok(None)
      );
      assert_eq!(
        block_on(stream.poll_next_chunk_with_size(&mut env, 3)),
        Ok(Some(Chunk::from_vec(vec![1, 2, 3])))
      );
    }

    #[test]
    fn range_with_start_and_end_builds_expected_half_open_interval() {
      let stream = Stream::range(3, 7);
      let out = block_on(stream.run_collect().run(&mut ()));
      assert_eq!(out, Ok(vec![3, 4, 5, 6]));
    }

    #[test]
    fn unfold_with_generator_stops_when_generator_returns_none() {
      let stream = Stream::unfold(0, |s| if s < 3 { Some((s, s + 1)) } else { None });
      let out = block_on(stream.run_collect().run(&mut ()));
      assert_eq!(out, Ok(vec![0, 1, 2]));
    }

    #[test]
    fn stream_from_channel_with_chunks_collects_chunks_in_order() {
      let (stream, sender) = stream_from_channel::<i32, &'static str, ()>(3);
      assert_eq!(
        block_on(send_chunk(&sender, Chunk::from_vec(vec![1, 2])).run(&mut ())),
        Ok(())
      );
      assert_eq!(
        block_on(send_chunk(&sender, Chunk::from_vec(vec![3, 4])).run(&mut ())),
        Ok(())
      );
      assert_eq!(block_on(end_stream(sender).run(&mut ())), Ok(()));

      let out = block_on(stream.run_collect().run(&mut ()));
      assert_eq!(out, Ok(vec![1, 2, 3, 4]));
    }

    #[test]
    fn stream_from_channel_with_producer_failure_propagates_failure() {
      let (stream, sender) = stream_from_channel::<i32, &'static str, ()>(1);
      assert!(sender.fail("producer-failure"));
      let out = block_on(stream.run_collect().run(&mut ()));
      assert_eq!(out, Err("producer-failure"));
    }
  }

  mod backpressure {
    use super::*;

    #[rstest]
    #[case::bounded_block(BackpressurePolicy::BoundedBlock)]
    #[case::drop_newest(BackpressurePolicy::DropNewest)]
    #[case::drop_oldest(BackpressurePolicy::DropOldest)]
    #[case::fail(BackpressurePolicy::Fail)]
    fn backpressure_decision_with_non_full_queue_always_enqueues(
      #[case] policy: BackpressurePolicy,
    ) {
      assert_eq!(
        backpressure_decision(policy, 0, 4),
        BackpressureDecision::Enqueue
      );
    }

    #[rstest]
    #[case::bounded_block(BackpressurePolicy::BoundedBlock, BackpressureDecision::Block)]
    #[case::drop_newest(BackpressurePolicy::DropNewest, BackpressureDecision::DropNewest)]
    #[case::drop_oldest(BackpressurePolicy::DropOldest, BackpressureDecision::DropOldest)]
    #[case::fail(BackpressurePolicy::Fail, BackpressureDecision::Fail)]
    fn backpressure_decision_with_full_queue_matches_policy_contract(
      #[case] policy: BackpressurePolicy,
      #[case] expected: BackpressureDecision,
    ) {
      assert_eq!(backpressure_decision(policy, 4, 4), expected);
    }

    #[test]
    fn backpressure_decision_with_zero_capacity_treats_capacity_as_one() {
      assert_eq!(
        backpressure_decision(BackpressurePolicy::BoundedBlock, 0, 0),
        BackpressureDecision::Enqueue
      );
      assert_eq!(
        backpressure_decision(BackpressurePolicy::Fail, 1, 0),
        BackpressureDecision::Fail
      );
    }

    #[test]
    fn stream_from_channel_backpressure_bounded_blocks() {
      let (stream, sender) =
        stream_from_channel_with_policy::<i32, (), ()>(1, BackpressurePolicy::BoundedBlock);
      let sender_clone = sender.clone();
      let producer = thread::spawn(move || {
        assert_eq!(
          block_on(send_chunk(&sender_clone, Chunk::from_vec(vec![1])).run(&mut ())),
          Ok(())
        );
        assert_eq!(
          block_on(send_chunk(&sender_clone, Chunk::from_vec(vec![2])).run(&mut ())),
          Ok(())
        );
        assert_eq!(block_on(end_stream(sender_clone).run(&mut ())), Ok(()));
      });
      let out = block_on(stream.run_collect().run(&mut ()));
      producer.join().expect("producer thread");
      assert_eq!(out, Ok(vec![1, 2]));
    }

    #[test]
    fn stream_from_channel_dropping_discards_newest() {
      let (gate_tx, gate_rx) = std::sync::mpsc::channel::<()>();
      let (stream, sender) =
        stream_from_channel_with_policy::<i32, (), ()>(2, BackpressurePolicy::DropNewest);
      let s2 = sender.clone();
      let producer = thread::spawn(move || {
        assert_eq!(
          block_on(send_chunk(&s2, Chunk::from_vec(vec![1])).run(&mut ())),
          Ok(())
        );
        assert_eq!(
          block_on(send_chunk(&s2, Chunk::from_vec(vec![2])).run(&mut ())),
          Ok(())
        );
        assert_eq!(
          block_on(send_chunk(&s2, Chunk::from_vec(vec![3])).run(&mut ())),
          Ok(())
        );
        gate_tx.send(()).expect("open gate for consumer");
        assert_eq!(block_on(end_stream(s2).run(&mut ())), Ok(()));
      });
      gate_rx.recv().expect("producer queued three chunks");
      let out = block_on(stream.run_collect().run(&mut ()));
      producer.join().expect("producer thread");
      assert_eq!(out, Ok(vec![1, 2]));
    }

    #[test]
    fn stream_sender_end_stream_closes_queue() {
      let (mut stream, sender) = stream_from_channel::<i32, (), ()>(2);
      assert_eq!(
        block_on(send_chunk(&sender, Chunk::from_vec(vec![42])).run(&mut ())),
        Ok(())
      );
      assert_eq!(block_on(end_stream(sender).run(&mut ())), Ok(()));
      assert_eq!(
        block_on(stream.poll_next_chunk(&mut ())),
        Ok(Some(Chunk::from_vec(vec![42])))
      );
      assert_eq!(block_on(stream.poll_next_chunk(&mut ())), Ok(None));
    }
  }

  mod transformations {
    use super::*;
    use crate::foundation::predicate::predicate;
    use crate::observability::metric::Metric;

    #[test]
    fn stream_throughput_metric_counts_elements() {
      let m = Metric::counter("stream_throughput_elems", []);
      let stream = Stream::range(0, 5).with_throughput_metric(m.clone());
      let out = block_on(stream.run_collect().run(&mut ()));
      assert_eq!(out, Ok(vec![0, 1, 2, 3, 4]));
      assert_eq!(m.snapshot_count(), 5);
    }

    #[test]
    fn map_filter_take_chain_with_values_produces_expected_output() {
      let stream = Stream::from_iterable(1..=10)
        .filter(Box::new(|n: &i32| *n % 2 == 0))
        .map(|n| n * 10)
        .take(3);
      let out = block_on(stream.run_collect().run(&mut ()));
      assert_eq!(out, Ok(vec![20, 40, 60]));
    }

    #[test]
    fn grouped_with_size_groups_stream_into_fixed_size_batches() {
      let stream = Stream::from_iterable([1, 2, 3, 4, 5]).grouped(2);
      let out = block_on(stream.run_collect().run(&mut ()));
      assert_eq!(out, Ok(vec![vec![1, 2], vec![3, 4], vec![5]]));
    }

    #[test]
    fn grouped_with_zero_size_returns_empty_output() {
      let stream = Stream::from_iterable([1, 2, 3]).grouped(0);
      let out = block_on(stream.run_collect().run(&mut ()));
      assert_eq!(out, Ok(Vec::<Vec<i32>>::new()));
    }

    #[test]
    fn map_effect_with_mapping_function_lifts_async_effectful_mapping() {
      let stream =
        Stream::from_iterable([1, 2, 3]).map_effect(|n| succeed::<i32, &'static str, ()>(n * 2));
      let out = block_on(stream.run_collect().run(&mut ()));
      assert_eq!(out, Ok(vec![2, 4, 6]));
    }

    #[test]
    fn map_effect_with_failures_preserves_upstream_and_mapper_error_channels() {
      let upstream_fail = Stream::<i32, &'static str, ()>::from_effect(fail("upstream"))
        .map_effect(|n| succeed::<i32, &'static str, ()>(n * 2));
      let upstream_out = block_on(upstream_fail.run_collect().run(&mut ()));
      assert_eq!(upstream_out, Err(Or::Left("upstream")));

      let mapper_fail =
        Stream::from_iterable([1, 2, 3]).map_effect(|_n| fail::<i32, &'static str, ()>("mapper"));
      let mapper_out = block_on(mapper_fail.run_collect().run(&mut ()));
      assert_eq!(mapper_out, Err(Or::Right("mapper")));
    }

    #[test]
    fn pure_transforms_with_incremental_polling_drive_output_from_chunks() {
      let mut stream = Stream::from_iterable([1, 2, 3, 4, 5, 6])
        .map(|n| n * 2)
        .filter(Box::new(|n: &i32| *n % 4 == 0))
        .take(2);
      let mut env = ();
      assert_eq!(
        block_on(stream.poll_next_chunk_with_size(&mut env, 1)),
        Ok(Some(Chunk::from_vec(vec![4])))
      );
      assert_eq!(
        block_on(stream.poll_next_chunk_with_size(&mut env, 1)),
        Ok(Some(Chunk::from_vec(vec![8])))
      );
      assert_eq!(
        block_on(stream.poll_next_chunk_with_size(&mut env, 1)),
        Ok(None)
      );
    }

    #[rstest]
    #[case(vec![1, 2, 3, 4, 5, 6], vec![2, 4, 6])]
    #[case(vec![2, 4, 8], vec![2, 4, 8])]
    fn filter_keeps_matching_elements(#[case] input: Vec<i32>, #[case] expected: Vec<i32>) {
      let stream = Stream::from_iterable(input).filter(Box::new(|n: &i32| *n % 2 == 0));
      let out = block_on(stream.run_collect().run(&mut ()));
      assert_eq!(out, Ok(expected));
    }

    #[test]
    fn filter_empty_stream_returns_empty() {
      let stream = Stream::<i32, (), ()>::from_iterable(core::iter::empty())
        .filter(Box::new(|n: &i32| *n % 2 == 0));
      let out = block_on(stream.run_collect().run(&mut ()));
      assert_eq!(out, Ok(Vec::new()));
    }

    #[test]
    fn take_while_stops_at_first_false() {
      let stream =
        Stream::from_iterable([2_i32, 4, 5, 6, 8]).take_while(Box::new(|n: &i32| *n % 2 == 0));
      let out = block_on(stream.run_collect().run(&mut ()));
      assert_eq!(out, Ok(vec![2, 4]));
    }

    #[test]
    fn drop_while_skips_initial_run() {
      let stream =
        Stream::from_iterable([0_i32, 2, 4, 1, 2, 3]).drop_while(Box::new(|n: &i32| *n % 2 == 0));
      let out = block_on(stream.run_collect().run(&mut ()));
      assert_eq!(out, Ok(vec![1, 2, 3]));
    }

    #[rstest]
    #[case(
      vec![-4_i32, -2, 0, 1, 2, 3, 4, 5],
      vec![2_i32, 4]
    )]
    #[case(vec![1_i32, 3, 5], vec![])]
    #[case(vec![2_i32, 4, 6], vec![2_i32, 4, 6])]
    fn filter_with_predicate_and_combinator_matches_only_even_positive(
      #[case] input: Vec<i32>,
      #[case] expected: Vec<i32>,
    ) {
      let p = predicate::and(Box::new(|n: &i32| *n % 2 == 0), Box::new(|n: &i32| *n > 0));
      let stream = Stream::from_iterable(input).filter(p);
      let out = block_on(stream.run_collect().run(&mut ()));
      assert_eq!(out, Ok(expected));
    }
  }

  mod consumers {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[test]
    fn run_fold_with_values_accumulates_all_values() {
      let stream = Stream::from_iterable([1, 2, 3, 4]);
      let out = block_on(stream.run_fold(0, |acc, x| acc + x).run(&mut ()));
      assert_eq!(out, Ok(10));
    }

    #[test]
    fn run_collect_with_pre_cancelled_token_returns_empty_output() {
      let stream = Stream::<i32, (), CancellationToken>::from_effect(succeed(vec![1, 2, 3]));
      let mut token = CancellationToken::new();
      token.cancel();
      let out = block_on(stream.run_collect().run(&mut token));
      assert_eq!(out, Ok(Vec::<i32>::new()));
    }

    #[test]
    fn run_reduce_with_empty_stream_returns_none() {
      let stream = Stream::<i32, (), ()>::from_iterable(core::iter::empty());
      let out = block_on(stream.run_reduce(|a, b| a + b).run(&mut ()));
      assert_eq!(out, Ok(None));
    }

    #[rstest]
    #[case::sum(vec![1, 2, 3], 6)]
    #[case::single(vec![5], 5)]
    fn run_reduce_with_values_combines_values_when_present(
      #[case] input: Vec<i32>,
      #[case] expected: i32,
    ) {
      let stream = Stream::from_iterable(input);
      let out = block_on(stream.run_reduce(|a, b| a + b).run(&mut ()));
      assert_eq!(out, Ok(Some(expected)));
    }

    #[test]
    fn run_for_each_with_callback_executes_sync_effect_for_each_element() {
      let seen: Rc<RefCell<Vec<i32>>> = Rc::new(RefCell::new(Vec::new()));
      let seen_ref = Rc::clone(&seen);
      let stream = Stream::from_iterable([2, 4, 6]);
      let out = block_on(
        stream
          .run_for_each(move |n| {
            let seen_ref = Rc::clone(&seen_ref);
            Effect::new(move |_r: &mut ()| {
              seen_ref.borrow_mut().push(n);
              Ok(())
            })
          })
          .run(&mut ()),
      );
      assert_eq!(out, Ok(()));
      assert_eq!(*seen.borrow(), vec![2, 4, 6]);
    }
  }

  mod unfold_effect {
    use super::*;

    #[test]
    fn unfold_effect_with_generator_pulls_until_none() {
      let stream = Stream::unfold_effect(0, |s| {
        succeed::<Option<(i32, i32)>, (), ()>(if s < 3 { Some((s, s + 1)) } else { None })
      });
      let out = block_on(stream.run_collect().run(&mut ()));
      assert_eq!(out, Ok(vec![0, 1, 2]));
    }
  }

  mod map_par_n {
    use super::*;
    use crate::run_async;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn map_par_n_preserves_order() {
      let stream = Stream::from_iterable([1, 2, 3, 4, 5]);
      let out = run_async(
        stream
          .map_par_n(2, |x: i32| {
            Effect::new_async(move |_r: &mut ()| {
              crate::box_future(async move {
                tokio::time::sleep(std::time::Duration::from_millis((6 - x).max(1) as u64)).await;
                Ok(x * 10)
              })
            })
          })
          .run_collect(),
        (),
      )
      .await
      .expect("collect");
      assert_eq!(out, vec![10, 20, 30, 40, 50]);
    }

    #[tokio::test]
    async fn map_par_n_limits_concurrency() {
      let current = Arc::new(AtomicUsize::new(0));
      let max_seen = Arc::new(AtomicUsize::new(0));
      let stream = Stream::from_iterable(0..12usize);
      let c = Arc::clone(&current);
      let m = Arc::clone(&max_seen);
      let collected = run_async(
        stream
          .map_par_n(3, move |i: usize| {
            let c = Arc::clone(&c);
            let m = Arc::clone(&m);
            Effect::new_async(move |_r: &mut ()| {
              crate::box_future(async move {
                let active = c.fetch_add(1, Ordering::SeqCst) + 1;
                let mut prev = m.load(Ordering::SeqCst);
                while active > prev {
                  match m.compare_exchange_weak(prev, active, Ordering::SeqCst, Ordering::SeqCst) {
                    Ok(_) => break,
                    Err(p) => prev = p,
                  }
                }
                tokio::time::sleep(std::time::Duration::from_millis(8)).await;
                c.fetch_sub(1, Ordering::SeqCst);
                Ok(i)
              })
            })
          })
          .run_collect(),
        (),
      )
      .await
      .expect("collect");
      assert_eq!(collected.len(), 12);
      assert!(
        max_seen.load(Ordering::SeqCst) <= 3,
        "observed max concurrency {}",
        max_seen.load(Ordering::SeqCst)
      );
    }

    #[tokio::test]
    async fn map_par_n_propagates_error_from_inner() {
      let stream = Stream::from_iterable([1, 2, 3]);
      let res = run_async(
        stream
          .map_par_n(2, |x: i32| {
            Effect::new(move |_r: &mut ()| if x == 2 { Err(()) } else { Ok(x) })
          })
          .run_collect(),
        (),
      )
      .await;
      assert_eq!(res, Err(()));
    }
  }

  mod channel_backed {
    use super::*;
    use crate::coordination::channel::{Channel, QueueChannel};

    #[test]
    fn stream_map_via_channel_preserves_elements() {
      let ch = Channel::<i32, (), (), (), ()>::from_stream(Stream::from_iterable([1_i32, 2, 3]));
      let stream = Stream::from_channel(ch).map(|n| n * 2);
      let out = block_on(stream.run_collect().run(&mut ()));
      assert_eq!(out, Ok(vec![2, 4, 6]));
    }

    #[test]
    fn stream_filter_via_channel_drops_unmatched() {
      let ch = Channel::<i32, (), (), (), ()>::from_stream(Stream::from_iterable([1_i32, 2, 3, 4]));
      let stream = Stream::from_channel(ch).filter(Box::new(|n: &i32| *n % 2 == 0));
      let out = block_on(stream.run_collect().run(&mut ()));
      assert_eq!(out, Ok(vec![2, 4]));
    }

    #[test]
    fn stream_collect_via_channel_gathers_all() {
      let ch = Channel::<i32, (), (), (), ()>::from_stream(Stream::from_iterable([10_i32, 20, 30]));
      let stream = Stream::from_channel(ch);
      let out = block_on(stream.run_collect().run(&mut ()));
      assert_eq!(out, Ok(vec![10, 20, 30]));
    }

    #[test]
    fn duplex_queue_channel_to_stream_drains_after_shutdown() {
      use crate::runtime::run_blocking;
      let ch = run_blocking(QueueChannel::<i32, i32, ()>::duplex_unbounded(), ()).expect("channel");
      run_blocking(ch.write(1), ()).unwrap();
      run_blocking(ch.write(2), ()).unwrap();
      run_blocking(ch.shutdown(), ()).unwrap();
      let out = block_on(ch.to_stream().map(|x| x * 10).run_collect().run(&mut ()));
      assert_eq!(out, Ok(vec![10, 20]));
    }

    #[test]
    fn stream_from_duplex_queue_channel_matches_queue_channel_to_stream() {
      use crate::runtime::run_blocking;
      let mk = || {
        let ch =
          run_blocking(QueueChannel::<i32, i32, ()>::duplex_unbounded(), ()).expect("channel");
        run_blocking(ch.write(7), ()).unwrap();
        run_blocking(ch.shutdown(), ()).unwrap();
        ch
      };
      let ch_a = mk();
      let ch_b = mk();
      let a = block_on(
        Stream::from_duplex_queue_channel(ch_a)
          .run_collect()
          .run(&mut ()),
      );
      let b = block_on(ch_b.to_stream().run_collect().run(&mut ()));
      assert_eq!(a, Ok(vec![7]));
      assert_eq!(a, b);
    }
  }

  mod broadcast_tests {
    use super::*;
    use crate::run_async;
    use std::time::Duration;

    #[tokio::test]
    async fn broadcast_all_consumers_receive_every_element() {
      let src = Stream::from_iterable(vec![1_i32, 2, 3]);
      let (streams, pump) = run_async(src.broadcast(8, 2), ()).await.expect("broadcast");
      assert_eq!(streams.len(), 2);
      let mut streams = streams;
      let s1 = streams.pop().expect("s1");
      let s0 = streams.pop().expect("s0");
      let (pr, a, b) = tokio::join!(
        run_async(pump, ()),
        run_async(s0.run_collect(), ()),
        run_async(s1.run_collect(), ()),
      );
      pr.expect("pump");
      assert_eq!(a.expect("collect 0"), vec![1, 2, 3]);
      assert_eq!(b.expect("collect 1"), vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn broadcast_slow_consumer_does_not_block_fast() {
      let (src, sender) = stream_from_channel::<i32, (), ()>(32);
      let (streams, pump) = run_async(src.broadcast(64, 2), ())
        .await
        .expect("broadcast");
      let mut streams = streams;
      let s_slow = streams.pop().expect("slow stream");
      let s_fast = streams.pop().expect("fast stream");

      let (pr, _, fast_v, slow_v) = tokio::join!(
        run_async(pump, ()),
        async {
          for i in 0..30i32 {
            run_async(send_chunk(&sender, Chunk::from_vec(vec![i])), ())
              .await
              .expect("send");
          }
          run_async(end_stream(sender), ()).await.expect("end");
        },
        run_async(s_fast.run_collect(), ()),
        async move {
          let mut out = Vec::new();
          let mut s = s_slow;
          let mut env = ();
          loop {
            tokio::time::sleep(Duration::from_millis(8)).await;
            match s.poll_next_chunk(&mut env).await.expect("poll") {
              Some(c) => out.extend(c.into_vec()),
              None => break,
            }
          }
          out
        },
      );
      pr.expect("pump");
      assert_eq!(fast_v.expect("fast collect"), (0..30).collect::<Vec<_>>());
      assert_eq!(slow_v, (0..30).collect::<Vec<_>>());
    }
  }

  // ── from_effect ──────────────────────────────────────────────────────────

  mod from_effect {
    use super::*;

    #[test]
    fn from_effect_wraps_vec_producing_effect() {
      let eff = succeed::<Vec<i32>, (), ()>(vec![10, 20, 30]);
      let stream = Stream::from_effect(eff);
      let out = block_on(stream.run_collect().run(&mut ()));
      assert_eq!(out, Ok(vec![10, 20, 30]));
    }

    #[test]
    fn from_effect_propagates_failure() {
      let eff = fail::<Vec<i32>, &'static str, ()>("boom");
      let stream = Stream::from_effect(eff);
      let out = block_on(stream.run_collect().run(&mut ()));
      assert_eq!(out, Err("boom"));
    }
  }

  // ── run_fold_effect ──────────────────────────────────────────────────────

  mod run_fold_effect {
    use super::*;

    #[test]
    fn run_fold_effect_accumulates_with_effectful_step() {
      let stream = Stream::from_iterable(vec![1_i32, 2, 3]);
      let out = block_on(
        stream
          .run_fold_effect(0, |acc, x| succeed::<i32, (), ()>(acc + x))
          .run(&mut ()),
      );
      assert_eq!(out, Ok(6));
    }

    #[test]
    fn run_fold_effect_propagates_step_failure() {
      let stream =
        Stream::<i32, &'static str, ()>::from_effect(succeed::<Vec<i32>, &'static str, ()>(vec![
          1, 2, 3,
        ]));
      let out = block_on(
        stream
          .run_fold_effect(0_i32, |_acc, x| {
            if x == 2 {
              fail::<i32, &'static str, ()>("step fail")
            } else {
              succeed::<i32, &'static str, ()>(0)
            }
          })
          .run(&mut ()),
      );
      assert_eq!(out, Err("step fail"));
    }
  }

  // ── scan ─────────────────────────────────────────────────────────────────

  mod scan {
    use super::*;

    #[test]
    fn scan_emits_running_state_per_element() {
      let stream = Stream::from_iterable(vec![1_i32, 2, 3, 4]);
      let out = block_on(
        stream
          .scan(0_i32, |acc, x| {
            *acc += x;
            *acc
          })
          .run_collect()
          .run(&mut ()),
      );
      assert_eq!(out, Ok(vec![1, 3, 6, 10]));
    }

    #[test]
    fn scan_with_empty_stream_produces_empty_output() {
      let stream = Stream::<i32, (), ()>::from_iterable(vec![]);
      let out = block_on(
        stream
          .scan(0_i32, |acc, x| {
            *acc += x;
            *acc
          })
          .run_collect()
          .run(&mut ()),
      );
      assert_eq!(out, Ok(vec![]));
    }
  }

  mod run_for_each_effect_direct {
    use super::*;

    #[test]
    fn run_for_each_effect_collects_side_effects() {
      use std::sync::{Arc, Mutex};
      let collected = Arc::new(Mutex::new(vec![]));
      let c = Arc::clone(&collected);
      let stream = Stream::from_iterable(vec![1_i32, 2, 3]);
      let result = block_on(
        stream
          .run_for_each_effect(move |x| {
            c.lock().unwrap().push(x);
            succeed::<(), (), ()>(())
          })
          .run(&mut ()),
      );
      assert_eq!(result, Ok(()));
      assert_eq!(*collected.lock().unwrap(), vec![1, 2, 3]);
    }
  }

  mod stream_channel_full_error {
    use super::*;

    #[test]
    fn stream_channel_full_display() {
      let e = StreamChannelFull;
      let s = format!("{e}");
      assert!(s.contains("full"), "display: {s}");
    }

    #[test]
    fn stream_channel_full_debug() {
      let _ = format!("{:?}", StreamChannelFull);
    }

    #[test]
    fn stream_channel_full_is_error_trait() {
      let e: &dyn std::error::Error = &StreamChannelFull;
      let _ = format!("{e}");
    }

    #[test]
    fn send_chunk_fail_policy_returns_err_when_full() {
      // Use a 2-item queue: fill it, then verify Fail policy rejects immediately.
      // Drop sender/stream directly — no end_stream (which would loop on full queue).
      let (_stream, sender) =
        stream_from_channel_with_policy::<i32, (), ()>(2, BackpressurePolicy::Fail);
      let r1 = block_on(send_chunk(&sender, Chunk::from_vec(vec![1])).run(&mut ()));
      assert_eq!(r1, Ok(()));
      let r2 = block_on(send_chunk(&sender, Chunk::from_vec(vec![2])).run(&mut ()));
      assert_eq!(r2, Ok(()));
      // Queue full — Fail policy returns Err immediately
      let r3 = block_on(send_chunk(&sender, Chunk::from_vec(vec![3])).run(&mut ()));
      assert_eq!(r3, Err(StreamChannelFull));
    }
  }

  mod range_tests {
    use super::*;

    #[test]
    fn range_empty_when_start_equals_end() {
      let out = block_on(
        Stream::<i64, (), ()>::range(5, 5)
          .run_collect()
          .run(&mut ()),
      );
      assert_eq!(out, Ok(vec![]));
    }

    #[test]
    fn range_empty_when_start_greater_than_end() {
      let out = block_on(
        Stream::<i64, (), ()>::range(10, 5)
          .run_collect()
          .run(&mut ()),
      );
      assert_eq!(out, Ok(vec![]));
    }
  }

  mod stream_error_channel {
    use super::*;

    #[test]
    fn stream_error_propagates_via_fail_message() {
      // Inject a ChannelMessage::Fail directly via the sender's queue
      let (stream, sender) = stream_from_channel_with_policy::<i32, &'static str, ()>(
        8,
        BackpressurePolicy::BoundedBlock,
      );
      let s = sender.clone();
      let _ = block_on(send_chunk(&s, Chunk::from_vec(vec![1, 2])).run(&mut ()));
      let msg = ChannelMessage::Fail("stream error");
      let _ = crate::runtime::run_blocking(sender.queue.offer(msg), ());
      let out = block_on(stream.run_collect().run(&mut ()));
      assert_eq!(out, Err("stream error"));
    }
  }
}
