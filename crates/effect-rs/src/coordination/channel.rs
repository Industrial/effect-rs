//! Bidirectional typed pipeline — generalises one-sided [`Stream`] and
//! [`Sink`] (§25).
//!
//! Queue-backed channels map each [`Channel::write`] through `InElem → OutElem` into a shared
//! [`Queue`]. [`Channel::read`] suspends until a value is available or the queue disconnects.

use std::collections::VecDeque;
use std::marker::PhantomData;
use std::sync::{Arc, Mutex};

use core::any::Any;
use tokio::sync::Mutex as TokioMutex;

#[inline]
fn interruption_requested<R: 'static>(env: &R) -> bool {
  let any = env as &dyn Any;
  any
    .downcast_ref::<crate::runtime::CancellationToken>()
    .is_some_and(crate::runtime::CancellationToken::is_cancelled)
}

use crate::coordination::queue::{Queue, QueueError};
use crate::failure::union::Or;
use crate::kernel::{Effect, box_future, succeed};
use crate::streaming::sink::Sink;
use crate::streaming::stream::Stream;

/// Errors from [`Channel::read`] and [`Channel::to_stream`].
///
/// [`Or::Left`] is a queue disconnect (including fold channels backed by a queue).
/// [`Or::Right`] is an upstream failure when the channel is [`Channel::from_stream`].
pub type ChannelReadError<E> = Or<QueueError, E>;

type MapIn<In, Out> = Arc<dyn Fn(In) -> Out + Send + Sync>;
type PostRead<Out> = Arc<dyn Fn(Out) -> Out + Send + Sync>;
type FlatMapOut<Out> = Arc<dyn Fn(Out) -> Vec<Out> + Send + Sync>;
type FoldStep<Acc, In> = Arc<dyn Fn(Acc, In) -> Acc + Send + Sync>;
/// Phantom for [`Channel`] metadata types (`OutDone`, `OutErr`, `R`) without storing `R`.
type ChannelMeta<OutDone, OutErr, R> = PhantomData<fn() -> (OutDone, OutErr, R)>;

/// Shared state for [`ChannelState::SinkAccum`]. Split out so [`Sink`] fold drivers can capture
/// `Arc<SinkAccumInner<…>>` (always `Send`) instead of [`ChannelState`] (not `Send` when `FromStream`
/// is a variant).
pub(crate) struct SinkAccumInner<OutElem, InElem>
where
  OutElem: Send + 'static,
  InElem: Send + 'static,
{
  pub(crate) acc: Arc<Mutex<OutElem>>,
  pub(crate) step: FoldStep<OutElem, InElem>,
  pub(crate) q: Queue<OutElem>,
  pub(crate) pending: Arc<Mutex<VecDeque<OutElem>>>,
  pub(crate) flat_map_out: Option<FlatMapOut<OutElem>>,
  pub(crate) post_read: PostRead<OutElem>,
}

pub(crate) enum ChannelState<OutElem, InElem, E, R>
where
  OutElem: Send + 'static,
  InElem: Send + 'static,
  E: Send + 'static,
  R: 'static,
{
  Queue {
    q: Queue<OutElem>,
    map_in: MapIn<InElem, OutElem>,
    pending: Arc<Mutex<VecDeque<OutElem>>>,
    flat_map_out: Option<FlatMapOut<OutElem>>,
    post_read: PostRead<OutElem>,
  },
  SinkAccum {
    inner: Arc<SinkAccumInner<OutElem, InElem>>,
  },
  FromStream {
    stream: Arc<TokioMutex<Stream<OutElem, E, R>>>,
    pending: Arc<Mutex<VecDeque<OutElem>>>,
    flat_map_out: Option<FlatMapOut<OutElem>>,
    post_read: PostRead<OutElem>,
  },
}

/// Duplex pipeline: [`Channel::write`] enqueues mapped values; [`Channel::read`] dequeues them.
pub struct Channel<OutElem, InElem, OutDone, OutErr, R: 'static>
where
  OutElem: Send + 'static,
  InElem: Send + 'static,
  OutErr: Send + 'static,
{
  state: Arc<ChannelState<OutElem, InElem, OutErr, R>>,
  /// Carry `OutDone` / `OutErr` / `R` without storing `R` (only for [`Effect`] typing).
  ///
  /// Uses a function pointer phantom so [`Channel`] stays [`Send`] when its element types are
  /// [`Send`] (unlike `PhantomData<*const R>`, which would make the handle `!Send`).
  _pd: ChannelMeta<OutDone, OutErr, R>,
}

/// Drain a stream into a fold channel ([`SinkAccumInner`]) for [`Sink::fold_left`] drivers.
pub(crate) fn consume_sink_accum_stream<OutElem, InElem, OutErr, R>(
  inner: Arc<SinkAccumInner<OutElem, InElem>>,
  mut stream: Stream<InElem, OutErr, R>,
) -> Effect<OutElem, OutErr, R>
where
  OutElem: Send + Clone,
  InElem: Send + Sync + Clone,
  OutErr: Send + 'static,
  R: 'static,
{
  let st = inner.clone();
  Effect::new_async(move |env: &mut R| {
    box_future(async move {
      loop {
        if interruption_requested(env) {
          break;
        }
        match stream.poll_next_chunk(env).await? {
          None => break,
          Some(chunk) => {
            for x in chunk.into_vec() {
              let ch = Channel::<OutElem, InElem, (), OutErr, R> {
                state: Arc::new(ChannelState::SinkAccum { inner: st.clone() }),
                _pd: PhantomData,
              };
              ch.write(x)
                .run(env)
                .await
                .expect("channel write is infallible");
            }
          }
        }
      }
      let ch = Channel::<OutElem, InElem, (), OutErr, R> {
        state: Arc::new(ChannelState::SinkAccum { inner: st.clone() }),
        _pd: PhantomData,
      };
      Ok(
        ch.fold_state()
          .run(env)
          .await
          .expect("fold_state is infallible"),
      )
    })
  })
}

impl<OutElem, InElem, OutDone, OutErr, R> Clone for Channel<OutElem, InElem, OutDone, OutErr, R>
where
  OutElem: Send + 'static,
  InElem: Send + 'static,
  OutDone: 'static,
  OutErr: Send + 'static,
  R: 'static,
{
  fn clone(&self) -> Self {
    Self {
      state: self.state.clone(),
      _pd: PhantomData,
    }
  }
}

impl<OutElem, InElem, OutDone, OutErr, R> Channel<OutElem, InElem, OutDone, OutErr, R>
where
  OutElem: Send + Clone + 'static,
  InElem: Send + 'static,
  OutDone: 'static,
  OutErr: Send + 'static,
  R: 'static,
{
  /// Unbounded duplex where `InElem` and `OutElem` are the same (identity map-in).
  pub fn duplex_unbounded<T>() -> Effect<Channel<T, T, OutDone, OutErr, R>, (), ()>
  where
    T: Send + 'static,
  {
    Queue::unbounded().flat_map(|q| {
      succeed(Channel {
        state: Arc::new(ChannelState::Queue {
          q,
          map_in: Arc::new(|t: T| t),
          pending: Arc::new(Mutex::new(VecDeque::new())),
          flat_map_out: None,
          post_read: Arc::new(|t| t),
        }),
        _pd: PhantomData,
      })
    })
  }

  /// Construct from a queue and an `InElem → OutElem` map (writes map then offer).
  pub fn from_queue_and_map(
    q: Queue<OutElem>,
    map_in: impl Fn(InElem) -> OutElem + Send + Sync + 'static,
  ) -> Self {
    Channel {
      state: Arc::new(ChannelState::Queue {
        q,
        map_in: Arc::new(map_in),
        pending: Arc::new(Mutex::new(VecDeque::new())),
        flat_map_out: None,
        post_read: Arc::new(|o| o),
      }),
      _pd: PhantomData,
    }
  }

  /// Read-only view of a [`Stream`] — [`Channel::write`] with `()` is a no-op.
  pub fn from_stream(
    stream: Stream<OutElem, OutErr, R>,
  ) -> Channel<OutElem, (), OutDone, OutErr, R> {
    Channel {
      state: Arc::new(ChannelState::FromStream {
        stream: Arc::new(TokioMutex::new(stream)),
        pending: Arc::new(Mutex::new(VecDeque::new())),
        flat_map_out: None,
        post_read: Arc::new(|o| o),
      }),
      _pd: PhantomData,
    }
  }

  /// Fold-only channel: each [`Channel::write`] updates an internal accumulator (same backing as
  /// [`Sink::fold_left`] / [`Channel::from_sink`]).
  #[must_use]
  pub fn from_fold(
    init: OutElem,
    step: Arc<dyn Fn(OutElem, InElem) -> OutElem + Send + Sync>,
  ) -> Self {
    let inner = Arc::new(SinkAccumInner {
      acc: Arc::new(Mutex::new(init)),
      step,
      q: crate::runtime::run_blocking(Queue::unbounded(), ()).expect("queue"),
      pending: Arc::new(Mutex::new(VecDeque::new())),
      flat_map_out: None,
      post_read: Arc::new(|o| o),
    });
    Channel {
      state: Arc::new(ChannelState::SinkAccum { inner }),
      _pd: PhantomData,
    }
  }

  /// Fold sink as a writable channel: each [`Channel::write`] updates the accumulator and enqueues
  /// a snapshot for [`Channel::read`].
  ///
  /// Requires [`Sink::fold_left`] / [`Sink::from_fold`]. Other sinks should use [`Sink::to_queue`]
  /// plus [`Channel::from_queue_and_map`].
  pub fn from_sink(sink: Sink<OutElem, InElem, OutErr, R>) -> Self {
    let fold = sink
      .fold
      .expect("Channel::from_sink requires Sink::fold_left / Sink::from_fold");
    Self::from_fold(fold.init.clone(), fold.f.clone())
  }

  /// Current accumulator (`SinkAccum` / [`Self::from_fold`] channels only).
  pub fn fold_state(&self) -> Effect<OutElem, (), R>
  where
    OutElem: Clone,
  {
    let st = self.state.clone();
    Effect::new_async(move |_env: &mut R| {
      box_future(async move {
        match &*st {
          ChannelState::SinkAccum { inner } => {
            Ok(inner.acc.lock().expect("sink accum mutex poisoned").clone())
          }
          _ => panic!("Channel::fold_state requires a fold (SinkAccum) channel"),
        }
      })
    })
  }

  /// Drain `stream` into this channel via [`Channel::write`] and return the final accumulator
  /// (`SinkAccum` / [`Self::from_fold`] only).
  pub fn consume_stream(&self, stream: Stream<InElem, OutErr, R>) -> Effect<OutElem, OutErr, R>
  where
    OutElem: Send + Clone,
    InElem: Send + Sync + Clone,
  {
    consume_sink_accum_stream(self.sink_accum_inner(), stream)
  }

  pub(crate) fn sink_accum_inner(&self) -> Arc<SinkAccumInner<OutElem, InElem>> {
    match &*self.state {
      ChannelState::SinkAccum { inner } => inner.clone(),
      _ => panic!("sink_accum_inner requires a fold (SinkAccum) channel"),
    }
  }

  /// Read the next element, or `None` when the source has ended / disconnected.
  ///
  /// Queue- and fold-backed channels return only `Ok` / `Ok(None)` today (disconnect maps to `None`).
  /// [`Channel::from_stream`] propagates stream poll failures as [`Or::Right`] in [`ChannelReadError`].
  pub fn read(&self) -> Effect<Option<OutElem>, ChannelReadError<OutErr>, R> {
    let st = self.state.clone();
    Effect::new_async(move |env: &mut R| {
      box_future(async move {
        match &*st {
          ChannelState::Queue {
            q,
            pending,
            flat_map_out,
            post_read,
            ..
          } => {
            let q = q.clone();
            let pending = pending.clone();
            let flat = flat_map_out.clone();
            let post = post_read.clone();
            loop {
              if let Some(v) = drain_pending(&pending, &post) {
                return Ok(Some(v));
              }
              match q.take().run(&mut ()).await {
                Ok(wire) => {
                  let mut buf: Vec<OutElem> = if let Some(f) = flat.as_ref() {
                    f(wire)
                  } else {
                    vec![wire]
                  };
                  if buf.is_empty() {
                    continue;
                  }
                  let first = buf.remove(0);
                  if !buf.is_empty() {
                    let mut g = pending.lock().expect("channel pending mutex poisoned");
                    for x in buf {
                      g.push_back(x);
                    }
                  }
                  return Ok(Some(post(first)));
                }
                Err(QueueError::Disconnected) => return Ok(None),
              }
            }
          }
          ChannelState::SinkAccum { inner } => {
            let q = inner.q.clone();
            let pending = inner.pending.clone();
            let flat = inner.flat_map_out.clone();
            let post = inner.post_read.clone();
            loop {
              if let Some(v) = drain_pending(&pending, &post) {
                return Ok(Some(v));
              }
              match q.take().run(&mut ()).await {
                Ok(wire) => {
                  let mut buf: Vec<OutElem> = if let Some(f) = flat.as_ref() {
                    f(wire)
                  } else {
                    vec![wire]
                  };
                  if buf.is_empty() {
                    continue;
                  }
                  let first = buf.remove(0);
                  if !buf.is_empty() {
                    let mut g = pending.lock().expect("channel pending mutex poisoned");
                    for x in buf {
                      g.push_back(x);
                    }
                  }
                  return Ok(Some(post(first)));
                }
                Err(QueueError::Disconnected) => return Ok(None),
              }
            }
          }
          ChannelState::FromStream {
            stream,
            pending,
            flat_map_out,
            post_read,
          } => {
            let stream = stream.clone();
            let pending = pending.clone();
            let flat = flat_map_out.clone();
            let post = post_read.clone();
            loop {
              if let Some(v) = drain_pending(&pending, &post) {
                return Ok(Some(v));
              }
              let wire = {
                let mut guard = stream.lock().await;
                loop {
                  match guard.poll_next_chunk(env).await {
                    Ok(None) => break None,
                    Ok(Some(chunk)) => {
                      let mut v = chunk.into_vec();
                      if v.is_empty() {
                        continue;
                      }
                      if v.len() == 1 {
                        break Some(v.pop().expect("len checked"));
                      }
                      let first = v.remove(0);
                      let mut g = pending.lock().expect("pending mutex poisoned");
                      for x in v {
                        g.push_back(x);
                      }
                      break Some(first);
                    }
                    Err(e) => return Err(Or::Right(e)),
                  }
                }
              };
              let Some(wire) = wire else {
                return Ok(None);
              };
              let mut buf: Vec<OutElem> = if let Some(f) = flat.as_ref() {
                f(wire)
              } else {
                vec![wire]
              };
              if buf.is_empty() {
                continue;
              }
              let first = buf.remove(0);
              if !buf.is_empty() {
                let mut g = pending.lock().expect("channel pending mutex poisoned");
                for x in buf {
                  g.push_back(x);
                }
              }
              return Ok(Some(post(first)));
            }
          }
        }
      })
    })
  }

  /// Map `In2` into the channel's input type before the map-in step.
  #[must_use]
  pub fn map_in<In2: Send>(
    self,
    f: impl Fn(In2) -> InElem + Send + Sync + 'static,
  ) -> Channel<OutElem, In2, OutDone, OutErr, R> {
    let f = Arc::new(f);
    match &*self.state {
      ChannelState::Queue {
        q,
        map_in,
        pending,
        flat_map_out,
        post_read,
      } => Channel {
        state: Arc::new(ChannelState::Queue {
          q: q.clone(),
          map_in: Arc::new({
            let inner = map_in.clone();
            let f = f.clone();
            move |i: In2| inner(f(i))
          }),
          pending: pending.clone(),
          flat_map_out: flat_map_out.clone(),
          post_read: post_read.clone(),
        }),
        _pd: PhantomData,
      },
      ChannelState::SinkAccum { .. } | ChannelState::FromStream { .. } => {
        panic!("Channel::map_in is only supported on queue-backed channels")
      }
    }
  }

  /// Map each dequeued element before it is returned from [`Channel::read`].
  #[must_use]
  pub fn map_out(
    self,
    f: impl Fn(OutElem) -> OutElem + Send + Sync + 'static,
  ) -> Channel<OutElem, InElem, OutDone, OutErr, R> {
    let f = Arc::new(f);
    match &*self.state {
      ChannelState::Queue {
        q,
        map_in,
        pending,
        flat_map_out,
        post_read,
      } => Channel {
        state: Arc::new(ChannelState::Queue {
          q: q.clone(),
          map_in: map_in.clone(),
          pending: pending.clone(),
          flat_map_out: flat_map_out.clone(),
          post_read: Arc::new({
            let prev = post_read.clone();
            let f = f.clone();
            move |x| f(prev(x))
          }),
        }),
        _pd: PhantomData,
      },
      ChannelState::SinkAccum { inner } => Channel {
        state: Arc::new(ChannelState::SinkAccum {
          inner: Arc::new(SinkAccumInner {
            acc: inner.acc.clone(),
            step: inner.step.clone(),
            q: inner.q.clone(),
            pending: inner.pending.clone(),
            flat_map_out: inner.flat_map_out.clone(),
            post_read: Arc::new({
              let prev = inner.post_read.clone();
              let f = f.clone();
              move |x| f(prev(x))
            }),
          }),
        }),
        _pd: PhantomData,
      },
      ChannelState::FromStream {
        stream,
        pending,
        flat_map_out,
        post_read,
      } => Channel {
        state: Arc::new(ChannelState::FromStream {
          stream: stream.clone(),
          pending: pending.clone(),
          flat_map_out: flat_map_out.clone(),
          post_read: Arc::new({
            let prev = post_read.clone();
            let f = f.clone();
            move |x| f(prev(x))
          }),
        }),
        _pd: PhantomData,
      },
    }
  }

  /// Expand each dequeued wire element into zero or more outputs (buffered for subsequent reads).
  #[must_use]
  pub fn flat_map_out(
    self,
    f: impl Fn(OutElem) -> Vec<OutElem> + Send + Sync + 'static,
  ) -> Channel<OutElem, InElem, OutDone, OutErr, R> {
    let f = Arc::new(f);
    match &*self.state {
      ChannelState::Queue {
        q,
        map_in,
        pending,
        flat_map_out,
        post_read,
      } => Channel {
        state: Arc::new(ChannelState::Queue {
          q: q.clone(),
          map_in: map_in.clone(),
          pending: pending.clone(),
          flat_map_out: Some(Arc::new({
            let prev = flat_map_out.clone();
            let f = f.clone();
            move |x| {
              let mid = if let Some(p) = prev.as_ref() {
                p(x)
              } else {
                vec![x]
              };
              let mut out = Vec::new();
              for y in mid {
                out.extend(f(y));
              }
              out
            }
          })),
          post_read: post_read.clone(),
        }),
        _pd: PhantomData,
      },
      ChannelState::SinkAccum { inner } => Channel {
        state: Arc::new(ChannelState::SinkAccum {
          inner: Arc::new(SinkAccumInner {
            acc: inner.acc.clone(),
            step: inner.step.clone(),
            q: inner.q.clone(),
            pending: inner.pending.clone(),
            flat_map_out: Some(Arc::new({
              let prev = inner.flat_map_out.clone();
              let f = f.clone();
              move |x| {
                let mid = if let Some(p) = prev.as_ref() {
                  p(x)
                } else {
                  vec![x]
                };
                let mut out = Vec::new();
                for y in mid {
                  out.extend(f(y));
                }
                out
              }
            })),
            post_read: inner.post_read.clone(),
          }),
        }),
        _pd: PhantomData,
      },
      ChannelState::FromStream {
        stream,
        pending,
        flat_map_out,
        post_read,
      } => Channel {
        state: Arc::new(ChannelState::FromStream {
          stream: stream.clone(),
          pending: pending.clone(),
          flat_map_out: Some(Arc::new({
            let prev = flat_map_out.clone();
            let f = f.clone();
            move |x| {
              let mid = if let Some(p) = prev.as_ref() {
                p(x)
              } else {
                vec![x]
              };
              let mut out = Vec::new();
              for y in mid {
                out.extend(f(y));
              }
              out
            }
          })),
          post_read: post_read.clone(),
        }),
        _pd: PhantomData,
      },
    }
  }
}

fn drain_pending<OutElem: Send>(
  pending: &Arc<Mutex<VecDeque<OutElem>>>,
  post_read: &PostRead<OutElem>,
) -> Option<OutElem> {
  let mut g = pending.lock().expect("channel pending mutex poisoned");
  g.pop_front().map(|x| post_read(x))
}

impl<OutElem, InElem, OutDone, OutErr, R> Channel<OutElem, InElem, OutDone, OutErr, R>
where
  OutElem: Send + Clone + 'static,
  InElem: Send + 'static,
  OutDone: 'static,
  OutErr: Send + 'static,
  R: 'static,
{
  /// Sequential pipeline: downstream input type matches this channel's output element type.
  #[must_use]
  pub fn pipe<Out2, D2, E2>(
    self,
    other: Channel<Out2, OutElem, D2, E2, R>,
  ) -> Channel<Out2, InElem, D2, E2, R>
  where
    Out2: Send + Clone + 'static,
    D2: 'static,
    E2: Send + 'static,
  {
    let (map_left, post_left) = match &*self.state {
      ChannelState::Queue {
        map_in, post_read, ..
      } => (map_in.clone(), post_read.clone()),
      _ => panic!("Channel::pipe requires a queue-backed left channel"),
    };
    let ChannelState::Queue {
      q: q_r,
      map_in: map_right,
      pending: p_r,
      flat_map_out: f_r,
      post_read: post_r,
    } = &*other.state
    else {
      panic!("Channel::pipe requires a queue-backed right channel");
    };
    Channel {
      state: Arc::new(ChannelState::Queue {
        q: q_r.clone(),
        map_in: Arc::new({
          let ml = map_left.clone();
          let pl = post_left.clone();
          let mr = map_right.clone();
          move |i: InElem| mr(pl(ml(i)))
        }),
        pending: p_r.clone(),
        flat_map_out: f_r.clone(),
        post_read: post_r.clone(),
      }),
      _pd: PhantomData,
    }
  }

  /// Enqueue one element (or update a fold sink and publish a snapshot).
  pub fn write(&self, value: InElem) -> Effect<(), (), R> {
    let st = self.state.clone();
    Effect::new_async(move |_env: &mut R| {
      box_future(async move {
        match &*st {
          ChannelState::Queue { q, map_in, .. } => {
            let out = map_in(value);
            let q = q.clone();
            loop {
              match q.offer(out.clone()).run(&mut ()).await {
                Ok(true) => return Ok(()),
                Ok(false) => tokio::task::yield_now().await,
                Err(()) => unreachable!("Queue::offer is infallible"),
              }
            }
          }
          ChannelState::SinkAccum { inner } => {
            let snap = {
              let mut g = inner.acc.lock().expect("sink accum mutex poisoned");
              *g = (inner.step)((*g).clone(), value);
              (inner.post_read)(g.clone())
            };
            let q = inner.q.clone();
            loop {
              match q.offer(snap.clone()).run(&mut ()).await {
                Ok(true) => return Ok(()),
                Ok(false) => tokio::task::yield_now().await,
                Err(()) => unreachable!("Queue::offer is infallible"),
              }
            }
          }
          ChannelState::FromStream { .. } => Ok(()),
        }
      })
    })
  }

  /// Convert to a [`Stream`] that pulls until the backing source ends or [`Channel::read`] fails.
  #[must_use]
  pub fn to_stream(&self) -> Stream<OutElem, ChannelReadError<OutErr>, R> {
    Stream::from_channel(self.clone())
  }

  /// [`Sink`] that writes every stream element into this channel (queue-backed only).
  #[must_use]
  pub fn to_sink(&self) -> Sink<(), InElem, QueueError, R>
  where
    InElem: Send + Sync + Clone + 'static,
    OutElem: Send + Clone,
  {
    match &*self.state {
      ChannelState::Queue { q, map_in, .. } => {
        let q = q.clone();
        let map_in = map_in.clone();
        Sink::from_driver(Arc::new(
          move |mut stream: Stream<InElem, QueueError, R>| {
            let q = q.clone();
            let map_in = map_in.clone();
            Effect::new_async(move |env: &mut R| {
              box_future(async move {
                loop {
                  match stream.poll_next_chunk(env).await {
                    Ok(None) => break,
                    Ok(Some(chunk)) => {
                      for x in chunk.into_vec() {
                        let out = map_in(x);
                        loop {
                          match q.offer(out.clone()).run(&mut ()).await {
                            Ok(true) => break,
                            Ok(false) => tokio::task::yield_now().await,
                            Err(()) => unreachable!("Queue::offer is infallible"),
                          }
                        }
                      }
                    }
                    Err(e) => return Err(e),
                  }
                }
                Ok(())
              })
            })
          },
        ))
      }
      ChannelState::SinkAccum { .. } | ChannelState::FromStream { .. } => {
        panic!("Channel::to_sink requires a queue-backed channel (duplex or from_queue_and_map)");
      }
    }
  }
}

// ---------------------------------------------------------------------------
// Queue-only channel handle (`Send` / `Sync`): safe in Axum `Handler` futures.
// ---------------------------------------------------------------------------

/// Queue-backed channel without stream or fold variants — always [`Send`] + [`Sync`] when element
/// types are [`Send`], unlike [`Channel`] (whose `ChannelState` includes stream drivers that are not
/// `Send`).
pub struct QueueChannel<OutElem, InElem, R: 'static>
where
  OutElem: Send + Clone + 'static,
  InElem: Send + 'static,
{
  inner: Arc<QueueChannelInner<OutElem, InElem>>,
  _pd: ChannelMeta<(), (), R>,
}

impl<OutElem, InElem, R> Clone for QueueChannel<OutElem, InElem, R>
where
  OutElem: Send + Clone + 'static,
  InElem: Send + 'static,
  R: 'static,
{
  fn clone(&self) -> Self {
    Self {
      inner: self.inner.clone(),
      _pd: PhantomData,
    }
  }
}

struct QueueChannelInner<OutElem, InElem>
where
  OutElem: Send + Clone + 'static,
  InElem: Send + 'static,
{
  q: Queue<OutElem>,
  map_in: MapIn<InElem, OutElem>,
  pending: Arc<Mutex<VecDeque<OutElem>>>,
  flat_map_out: Option<FlatMapOut<OutElem>>,
  post_read: PostRead<OutElem>,
}

impl<OutElem, InElem, R> QueueChannel<OutElem, InElem, R>
where
  OutElem: Send + Clone + 'static,
  InElem: Send + 'static,
  R: 'static,
{
  /// Same wire shape as [`Channel::from_queue_and_map`], but only the queue implementation exists.
  pub fn from_queue_and_map(
    q: Queue<OutElem>,
    map_in: impl Fn(InElem) -> OutElem + Send + Sync + 'static,
  ) -> Self {
    Self {
      inner: Arc::new(QueueChannelInner {
        q,
        map_in: Arc::new(map_in),
        pending: Arc::new(Mutex::new(VecDeque::new())),
        flat_map_out: None,
        post_read: Arc::new(|o| o),
      }),
      _pd: PhantomData,
    }
  }

  /// Close the backing queue to producers; buffered values remain available to [`Self::read`].
  pub fn shutdown(&self) -> Effect<(), (), ()> {
    let q = self.inner.q.clone();
    Effect::new_async(move |_r: &mut ()| box_future(async move { q.shutdown().run(&mut ()).await }))
  }

  /// Read the next element, or `None` when the queue has ended / disconnected.
  pub fn read(&self) -> Effect<Option<OutElem>, QueueError, R> {
    let inner = self.inner.clone();
    Effect::new_async(move |_env: &mut R| {
      let q = inner.q.clone();
      let pending = inner.pending.clone();
      let flat = inner.flat_map_out.clone();
      let post = inner.post_read.clone();
      box_future(async move {
        loop {
          if let Some(v) = drain_pending(&pending, &post) {
            return Ok(Some(v));
          }
          match q.take().run(&mut ()).await {
            Ok(wire) => {
              let mut buf: Vec<OutElem> = if let Some(f) = flat.as_ref() {
                f(wire)
              } else {
                vec![wire]
              };
              if buf.is_empty() {
                continue;
              }
              let first = buf.remove(0);
              if !buf.is_empty() {
                let mut g = pending.lock().expect("channel pending mutex poisoned");
                for x in buf {
                  g.push_back(x);
                }
              }
              return Ok(Some(post(first)));
            }
            Err(QueueError::Disconnected) => return Ok(None),
          }
        }
      })
    })
  }

  /// Enqueue one mapped element.
  pub fn write(&self, value: InElem) -> Effect<(), (), R> {
    let inner = self.inner.clone();
    Effect::new_async(move |_env: &mut R| {
      let out = (inner.map_in)(value);
      let q = inner.q.clone();
      box_future(async move {
        loop {
          match q.offer(out.clone()).run(&mut ()).await {
            Ok(true) => return Ok(()),
            Ok(false) => tokio::task::yield_now().await,
            Err(()) => unreachable!("Queue::offer is infallible"),
          }
        }
      })
    })
  }
}

impl<T, R> QueueChannel<T, T, R>
where
  T: Send + Clone + 'static,
  R: 'static,
{
  /// Unbounded duplex with identity map-in (same as [`Channel::duplex_unbounded`] for the queue case).
  pub fn duplex_unbounded() -> Effect<QueueChannel<T, T, R>, (), ()> {
    Queue::unbounded().flat_map(|q| succeed(QueueChannel::from_queue_and_map(q, |t| t)))
  }

  /// Same drain semantics as [`Channel::to_stream`]: one bootstrap pull over [`QueueChannel::read`].
  ///
  /// Call [`QueueChannel::shutdown`] when no further writes are expected so [`Stream::run_collect`]
  /// can finish.
  #[must_use]
  pub fn to_stream(&self) -> Stream<T, QueueError, R> {
    Stream::from_duplex_queue_channel(self.clone())
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::runtime::run_blocking;
  use crate::{Or, box_future};

  fn block_on_effect<A, E>(eff: Effect<A, E, ()>) -> Result<A, E> {
    run_blocking(eff, ())
  }

  #[test]
  fn channel_read_returns_written_elements() {
    let ch = block_on_effect(Channel::<i32, i32, (), (), ()>::duplex_unbounded()).expect("channel");
    block_on_effect(ch.write(10)).expect("write");
    block_on_effect(ch.write(20)).expect("write");
    assert_eq!(block_on_effect(ch.read()).expect("read"), Some(10));
    assert_eq!(block_on_effect(ch.read()).expect("read"), Some(20));
  }

  #[test]
  fn channel_pipe_composes_two_channels() {
    let left = Channel::<i32, i32, (), (), ()>::from_queue_and_map(
      run_blocking(Queue::unbounded(), ()).expect("q"),
      |x: i32| x * 2,
    );
    let right = Channel::<i32, i32, (), (), ()>::from_queue_and_map(
      run_blocking(Queue::unbounded(), ()).expect("q2"),
      |x: i32| x + 1,
    );
    let both = left.pipe(right);
    block_on_effect(both.write(5)).expect("write");
    assert_eq!(block_on_effect(both.read()).expect("read"), Some(11));
  }

  #[test]
  fn channel_to_stream_bridges_correctly() {
    let inner = Stream::from_iterable(vec![1_i32, 2, 3]);
    let ch = Channel::<i32, (), (), (), ()>::from_stream(inner);
    let stream = ch.to_stream();
    let collected = block_on_effect(stream.run_collect()).expect("collect");
    assert_eq!(collected, vec![1, 2, 3]);
  }

  #[test]
  fn channel_read_from_stream_preserves_upstream_error() {
    let inner =
      Stream::<i32, &'static str, ()>::new(|_env| box_future(async move { Err("upstream") }));
    let ch = Channel::<i32, (), (), &'static str, ()>::from_stream(inner);
    assert_eq!(block_on_effect(ch.read()), Err(Or::Right("upstream")));
  }

  #[test]
  fn channel_to_stream_preserves_upstream_error() {
    let inner =
      Stream::<i32, &'static str, ()>::new(|_env| box_future(async move { Err("upstream") }));
    let ch = Channel::<i32, (), (), &'static str, ()>::from_stream(inner);
    assert_eq!(
      block_on_effect(ch.to_stream().run_collect()),
      Err(Or::Right("upstream"))
    );
  }

  #[test]
  fn queue_channel_maps_input_and_drains_after_shutdown() {
    let q = block_on_effect(Queue::<i32>::unbounded()).expect("q");
    let qc = QueueChannel::from_queue_and_map(q, |x: i32| x * 10);
    block_on_effect(qc.write(2)).expect("write");
    assert_eq!(block_on_effect(qc.read()).unwrap(), Some(20));
    block_on_effect(qc.shutdown()).expect("shutdown");
    assert_eq!(block_on_effect(qc.read()).unwrap(), None);
  }

  #[test]
  fn queue_channel_duplex_to_stream_collects() {
    let qc = block_on_effect(QueueChannel::<i32, i32, ()>::duplex_unbounded()).expect("qc");
    block_on_effect(qc.write(1)).expect("w");
    block_on_effect(qc.write(2)).expect("w");
    block_on_effect(qc.shutdown()).expect("shutdown");
    let stream = qc.to_stream();
    let got = block_on_effect(stream.run_collect()).expect("collect");
    assert_eq!(got, vec![1, 2]);
  }
}
