//! [`tower::Service`] implementations that run [`effect_rs::Effect`] programs using
//! [`effect_tokio::run_async`], so handlers stay in the Effect interpreter while composing with
//! Tower middleware stacks.
//!
//! This crate depends on **`effect_rs_tokio`** for the async runtime bridge; it does not re-export
//! [`effect_tokio::TokioRuntime`].
//!
//! ## Concurrency
//!
//! [`EffectService::with_max_in_flight`] adds a [`tokio::sync::Semaphore`] gate so
//! [`Service::poll_ready`] acquires a permit before each
//! [`Service::call`]. In-flight work is also tracked in a
//! [`effect_rs::SynchronizedRef`] so effectful updates stay serialized when you compose with other
//! `Effect` code.
//!
//! [`EffectService::with_request_metrics`] wraps each call with [`effect_rs::Metric::track_duration`]
//! (typically a [`Metric::timer`](effect_rs::Metric::timer)) and increments an error counter when the
//! handler effect fails.
//!
//! ## Examples
//!
//! See `examples/` (e.g. `cargo run -p effect_rs_tower --example 001_effect_service`) or
//! `moon run effect-tower:examples`.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use effect_rs::duration::Duration;
use effect_rs::{Effect, Metric, QueueError, SynchronizedRef, box_future};
use effect_tokio::run_async as run_effect_async;
use tokio::sync::{AcquireError, OwnedSemaphorePermit, Semaphore};
use tower::Service;

type AcquireFuture =
  Pin<Box<dyn Future<Output = Result<OwnedSemaphorePermit, AcquireError>> + Send>>;

/// Optional per-request latency ([`Metric::timer`] / histogram) and error-rate counter.
#[derive(Clone)]
struct RequestMetrics {
  latency: Metric<Duration, ()>,
  errors: Metric<u64, ()>,
}

/// Shared limiter for [`EffectService::with_max_in_flight`].
struct ServiceLimit {
  sem: Arc<Semaphore>,
  in_flight: SynchronizedRef<u64>,
}

impl ServiceLimit {
  fn new(max_in_flight: usize) -> Self {
    assert!(max_in_flight > 0, "max_in_flight must be positive");
    let in_flight = SynchronizedRef::new(0_u64);
    Self {
      sem: Arc::new(Semaphore::new(max_in_flight)),
      in_flight,
    }
  }
}

/// [`Service`] that clones `S`, runs `f(&mut env, req)` to an effect, then drives it with
/// [`run_effect_async`].
///
/// `S` is typically application state (`Clone`, often `Arc`-backed), matching Axum’s pattern.
pub struct EffectService<S, F, Req> {
  state: S,
  f: F,
  limit: Option<Arc<ServiceLimit>>,
  request_metrics: Option<RequestMetrics>,
  /// Permit reserved by [`Service::poll_ready`](tower::Service::poll_ready) for the next
  /// [`Service::call`](tower::Service::call) when a concurrency limit is set.
  ready_permit: Option<OwnedSemaphorePermit>,
  pending_acquire: Option<AcquireFuture>,
  _pd: PhantomData<fn(Req) -> ()>,
}

impl<S, F, Req> EffectService<S, F, Req> {
  /// Build an unlimited-concurrency service with handler closure `f` and shared state `state`.
  #[inline]
  pub fn new(state: S, f: F) -> Self {
    Self {
      state,
      f,
      limit: None,
      request_metrics: None,
      ready_permit: None,
      pending_acquire: None,
      _pd: PhantomData,
    }
  }

  /// Record each request duration with `latency` ([`Metric::track_duration`]) and increment `errors`
  /// when the inner effect fails.
  #[must_use]
  #[inline]
  pub fn with_request_metrics(
    mut self,
    latency: Metric<Duration, ()>,
    errors: Metric<u64, ()>,
  ) -> Self {
    self.request_metrics = Some(RequestMetrics { latency, errors });
    self
  }

  /// Same as [`Self::new`], but at most `max_in_flight` requests run concurrently.
  ///
  /// [`Service::poll_ready`] acquires a semaphore permit (possibly
  /// after pending) before reporting readiness; [`Service::call`] consumes
  /// that permit for the duration of the handler effect.
  ///
  /// # Panics
  ///
  /// Panics if `max_in_flight == 0`.
  #[inline]
  pub fn with_max_in_flight(state: S, max_in_flight: usize, f: F) -> Self {
    Self {
      state,
      f,
      limit: Some(Arc::new(ServiceLimit::new(max_in_flight))),
      request_metrics: None,
      ready_permit: None,
      pending_acquire: None,
      _pd: PhantomData,
    }
  }

  /// Current in-flight count when this service was created with [`Self::with_max_in_flight`].
  ///
  /// Returns `None` if there is no concurrency limit (unlimited [`Self::new`]).
  pub fn in_flight_counter(&self) -> Option<SynchronizedRef<u64>> {
    self.limit.as_ref().map(|l| l.in_flight.clone())
  }

  /// Immutable reference to the service’s shared state `S`.
  #[inline]
  pub fn state(&self) -> &S {
    &self.state
  }

  /// Mutable reference to the service’s shared state `S`.
  #[inline]
  pub fn state_mut(&mut self) -> &mut S {
    &mut self.state
  }

  fn poll_ready_limited(
    &mut self,
    cx: &mut Context<'_>,
    lim: &Arc<ServiceLimit>,
  ) -> Poll<Result<(), AcquireError>> {
    if self.ready_permit.is_some() {
      return Poll::Ready(Ok(()));
    }

    match lim.sem.clone().try_acquire_owned() {
      Ok(permit) => {
        self.pending_acquire = None;
        self.ready_permit = Some(permit);
        Poll::Ready(Ok(()))
      }
      Err(_) => {
        if self.pending_acquire.is_none() {
          self.pending_acquire = Some(Box::pin(lim.sem.clone().acquire_owned()));
        }
        let fut = self
          .pending_acquire
          .as_mut()
          .expect("just set pending_acquire");
        match fut.as_mut().poll(cx) {
          Poll::Ready(Ok(permit)) => {
            self.pending_acquire = None;
            self.ready_permit = Some(permit);
            Poll::Ready(Ok(()))
          }
          Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
          Poll::Pending => Poll::Pending,
        }
      }
    }
  }
}

impl<S, F, Req> Clone for EffectService<S, F, Req>
where
  S: Clone,
  F: Clone,
{
  fn clone(&self) -> Self {
    Self {
      state: self.state.clone(),
      f: self.f.clone(),
      limit: self.limit.clone(),
      request_metrics: self.request_metrics.clone(),
      ready_permit: None,
      pending_acquire: None,
      _pd: PhantomData,
    }
  }
}

impl<S, F, Req, Res, E> Service<Req> for EffectService<S, F, Req>
where
  S: Clone + Send + 'static,
  Req: Send + 'static,
  Res: Send + 'static,
  E: Send + 'static,
  F: Fn(&mut S, Req) -> Effect<Res, E, S> + Clone + Send + 'static,
{
  type Response = Res;
  type Error = E;
  /// Not `Send`: [`effect_rs::Effect`] uses non-`Send` [`effect_rs::BoxFuture`]
  /// so it can support single-threaded / `!Send` environments. Use a single-thread Tokio runtime
  /// (`current_thread`) or `spawn_local` if you need `!Send` futures on Tokio.
  type Future = Pin<Box<dyn Future<Output = Result<Res, E>> + 'static>>;

  fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
    let lim = self.limit.clone();
    match lim {
      None => Poll::Ready(Ok(())),
      Some(lim) => match self.poll_ready_limited(cx, &lim) {
        Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
        Poll::Ready(Err(e)) => {
          // Semaphore closed — treat as ready with no permit; call will panic on take (extremely rare).
          let _ = e;
          Poll::Ready(Ok(()))
        }
        Poll::Pending => Poll::Pending,
      },
    }
  }

  fn call(&mut self, req: Req) -> Self::Future {
    let mut env = self.state.clone();
    let f = self.f.clone();
    let metrics = self.request_metrics.clone();
    match &self.limit {
      None => Box::pin(async move {
        let eff = f(&mut env, req);
        let eff = match &metrics {
          Some(m) => m.latency.track_duration(eff),
          None => eff,
        };
        let result = run_effect_async(eff, env).await;
        if result.is_err()
          && let Some(m) = metrics
        {
          let _ = run_effect_async(m.errors.apply(1), ()).await;
        }
        result
      }),
      Some(lim) => {
        let permit = self
          .ready_permit
          .take()
          .expect("EffectService::call without successful poll_ready");
        let in_flight = lim.in_flight.clone();
        Box::pin(async move {
          run_effect_async(in_flight.update(|n| n.saturating_add(1)), ())
            .await
            .expect("in_flight increment");
          let eff = f(&mut env, req);
          let eff = match &metrics {
            Some(m) => m.latency.track_duration(eff),
            None => eff,
          };
          let result = run_effect_async(eff, env.clone()).await;
          if result.is_err()
            && let Some(m) = metrics
          {
            let _ = run_effect_async(m.errors.apply(1), ()).await;
          }
          run_effect_async(in_flight.update(|n| n.saturating_sub(1)), ())
            .await
            .expect("in_flight decrement");
          drop(permit);
          result
        })
      }
    }
  }
}

/// Tower [`Service`] shaped like a queue-backed [`effect_rs::channel::QueueChannel`]: each
/// [`Service::call`] runs [`QueueChannel::write`](effect_rs::channel::QueueChannel::write) with the
/// request, then [`QueueChannel::read`](effect_rs::channel::QueueChannel::read) for the response.
/// Read errors are [`QueueError`].
pub struct ChannelService<S, Req, Res>
where
  Req: Send + 'static,
  Res: Send + Clone + 'static,
  S: 'static,
{
  state: S,
  channel: effect_rs::channel::QueueChannel<Res, Req, S>,
}

impl<S, Req, Res> ChannelService<S, Req, Res>
where
  Req: Send + 'static,
  Res: Send + Clone + 'static,
  S: 'static,
{
  /// Queue-backed service with environment `state` and shared `channel`.
  #[inline]
  pub fn new(state: S, channel: effect_rs::channel::QueueChannel<Res, Req, S>) -> Self {
    Self { state, channel }
  }

  /// Immutable reference to the channel environment `S`.
  #[inline]
  pub fn state(&self) -> &S {
    &self.state
  }

  /// Mutable reference to the channel environment `S`.
  #[inline]
  pub fn state_mut(&mut self) -> &mut S {
    &mut self.state
  }

  /// Borrow the underlying [`effect_rs::channel::QueueChannel`].
  #[inline]
  pub fn channel(&self) -> &effect_rs::channel::QueueChannel<Res, Req, S> {
    &self.channel
  }
}

impl<S, Req, Res> Clone for ChannelService<S, Req, Res>
where
  S: Clone + 'static,
  Req: Send + 'static,
  Res: Send + Clone + 'static,
{
  fn clone(&self) -> Self {
    Self {
      state: self.state.clone(),
      channel: self.channel.clone(),
    }
  }
}

impl<S, Req, Res> Service<Req> for ChannelService<S, Req, Res>
where
  S: Clone + Send + 'static,
  Req: Send + 'static,
  Res: Send + Clone + 'static,
{
  type Response = Res;
  type Error = QueueError;
  type Future = Pin<Box<dyn Future<Output = Result<Res, QueueError>> + 'static>>;

  fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
    Poll::Ready(Ok(()))
  }

  fn call(&mut self, req: Req) -> Self::Future {
    let env = self.state.clone();
    let ch = self.channel.clone();
    Box::pin(async move {
      run_effect_async(
        Effect::new_async(move |r: &mut S| {
          box_future(async move {
            ch.write(req).run(r).await.unwrap();
            match ch.read().run(r).await {
              Ok(Some(x)) => Ok(x),
              Ok(None) => Err(QueueError::Disconnected),
              Err(e) => Err(e),
            }
          })
        }),
        env,
      )
      .await
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use effect_rs::Metric;
  use effect_rs::channel::QueueChannel;
  use effect_rs::succeed;
  use std::time::Duration;
  use tower::{Service, ServiceExt};

  #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
  async fn tower_channel_service_returns_response() {
    let ch = run_effect_async(QueueChannel::<u32, u32, ()>::duplex_unbounded(), ())
      .await
      .expect("duplex channel");
    let mut svc = ChannelService::new((), ch);
    assert_eq!(svc.ready().await.unwrap().call(41).await, Ok(41));
  }

  #[tokio::test(flavor = "current_thread")]
  async fn service_runs_effect() {
    let mut svc = EffectService::new((), |_env: &mut (), x: u32| {
      succeed::<u32, (), _>(x.saturating_add(1))
    });
    assert_eq!(svc.ready().await.unwrap().call(10).await, Ok(11));
  }

  #[tokio::test(flavor = "current_thread")]
  async fn tower_latency_histogram_records_duration() {
    let timer = Metric::timer("tower_req", std::iter::empty());
    let errors = Metric::counter("tower_err", std::iter::empty());
    let mut svc = EffectService::new((), |_env: &mut (), _x: u32| {
      Effect::new_async(move |_r: &mut ()| {
        effect_rs::box_future(async move {
          tokio::time::sleep(Duration::from_millis(5)).await;
          Ok::<u32, ()>(7)
        })
      })
    })
    .with_request_metrics(timer.clone(), errors.clone());

    assert_eq!(svc.ready().await.unwrap().call(0).await, Ok(7));
    let obs = timer.snapshot_durations();
    assert_eq!(obs.len(), 1);
    assert!(obs[0] > effect_rs::duration::Duration::ZERO);
    assert_eq!(errors.snapshot_count(), 0);
  }

  #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
  async fn tower_service_concurrent_calls_count_correctly() {
    let local = tokio::task::LocalSet::new();
    local
      .run_until(async {
        let svc = EffectService::with_max_in_flight((), 4_usize, |_env: &mut (), _req: u32| {
          Effect::new_async(move |_r: &mut ()| {
            effect_rs::box_future(async move {
              tokio::time::sleep(Duration::from_millis(20)).await;
              Ok::<u32, ()>(1)
            })
          })
        });

        let mut handles = Vec::new();
        for i in 0..8_u32 {
          let mut s = svc.clone();
          handles.push(tokio::task::spawn_local(async move {
            s.ready().await.unwrap();
            s.call(i).await
          }));
        }
        for h in handles {
          assert_eq!(h.await.unwrap(), Ok(1));
        }

        let ctr = svc.in_flight_counter().expect("counter");
        let n = run_effect_async(ctr.get(), ())
          .await
          .expect("in_flight counter read");
        assert_eq!(n, 0, "in_flight should return to zero");
      })
      .await;
  }

  #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
  async fn tower_service_ready_blocks_until_capacity() {
    let mut slow = EffectService::with_max_in_flight((), 1_usize, |_env: &mut (), _req: ()| {
      Effect::new_async(move |_r: &mut ()| {
        effect_rs::box_future(async move {
          tokio::time::sleep(Duration::from_millis(200)).await;
          Ok::<(), ()>(())
        })
      })
    });
    let mut fast = slow.clone();

    slow.ready().await.unwrap();
    let long = slow.call(());

    let short_timeout = tokio::time::timeout(Duration::from_millis(40), fast.ready());
    assert!(
      short_timeout.await.is_err(),
      "second ready() should not complete while the single permit is held"
    );

    long.await.unwrap();

    tokio::time::timeout(Duration::from_secs(2), fast.ready())
      .await
      .expect("ready should complete after slow call finishes")
      .unwrap();
  }

  #[tokio::test(flavor = "current_thread")]
  async fn tower_error_metric_increments_on_failure() {
    let timer = Metric::timer("tower_req_fail", std::iter::empty());
    let errors = Metric::counter("tower_err_fail", std::iter::empty());
    let mut svc = EffectService::new((), |_env: &mut (), _x: u32| {
      effect_rs::fail::<u32, &str, ()>("oops")
    })
    .with_request_metrics(timer.clone(), errors.clone());

    let _ = svc.ready().await.unwrap().call(0).await;
    assert_eq!(errors.snapshot_count(), 1);
    assert_eq!(timer.snapshot_durations().len(), 1);
  }

  #[tokio::test(flavor = "current_thread")]
  async fn in_flight_counter_none_when_no_limit() {
    let svc: EffectService<(), _, u32> =
      EffectService::new((), |_env: &mut (), x: u32| succeed::<u32, (), ()>(x));
    assert!(svc.in_flight_counter().is_none());
  }
}
