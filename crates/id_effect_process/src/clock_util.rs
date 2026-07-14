//! [`SpinClock`] — a dependency-free [`Clock`] for thread-per-process
//! nodes.
//!
//! `sleep` is implemented as a self-waking poll loop against
//! [`Instant::now`], so it works under [`id_effect::run_blocking`]'s
//! noop-waker interpreter *and* under real executors (it always schedules
//! its own wake-up). Timeouts burn a poll per scheduler pass; production
//! nodes should prefer `LiveClock<TokioRuntime>` (see
//! [`crate::node::ProcessNode::on_runtime`]).

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use id_effect::runtime::Never;
use id_effect::scheduling::Clock;
use id_effect::{Effect, box_future};

/// Busy-polling clock (see module docs).
#[derive(Clone, Copy, Debug, Default)]
pub struct SpinClock;

struct SleepUntil {
  deadline: Instant,
}

impl Future for SleepUntil {
  type Output = ();

  fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
    if Instant::now() >= self.deadline {
      Poll::Ready(())
    } else {
      // Self-wake so real executors re-poll us; the noop-waker spin
      // interpreter re-polls unconditionally anyway.
      cx.waker().wake_by_ref();
      std::thread::yield_now();
      Poll::Pending
    }
  }
}

impl Clock for SpinClock {
  fn now(&self) -> Instant {
    Instant::now()
  }

  fn sleep(&self, duration: Duration) -> Effect<(), Never, ()> {
    let deadline = Instant::now() + duration;
    Effect::new_async(move |_r| {
      box_future(async move {
        SleepUntil { deadline }.await;
        Ok(())
      })
    })
  }

  fn sleep_until(&self, deadline: Instant) -> Effect<(), Never, ()> {
    Effect::new_async(move |_r| {
      box_future(async move {
        SleepUntil { deadline }.await;
        Ok(())
      })
    })
  }
}
