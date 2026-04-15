//! [`Runtime`] trait, [`ThreadSleepRuntime`], `run_fork`, and `yield_now` (§ 6.1, § 6.2 tail).
//!
//! `run_fork` and `yield_now` are free-function wrappers around [`Runtime`] methods, placed here
//! because they require a concrete `Runtime` value and would create a cross-module cycle if kept
//! in [`super::execute`].

use std::time::{Duration, Instant};

use crate::kernel::Effect;

use super::execute::{Never, run_blocking};
use crate::concurrency::{FiberHandle, FiberId};

/// Runtime interface for executing effects and scheduling cooperative work.
pub trait Runtime {
  /// Spawn an effect as a fiber; returns a handle to observe completion.
  ///
  /// `f` is called on the runtime’s worker (a dedicated OS thread for [`ThreadSleepRuntime`], a
  /// blocking-pool thread for `TokioRuntime` in workspace crate `effect_rs_tokio`). It must return
  /// `(effect, env)` there so
  /// [`Effect`] need not be [`Send`] — only the **factory** closure is [`Send`].
  ///
  /// Prefer `|| (succeed(x), env)` over capturing a pre-built [`Effect`] in the closure unless you
  /// know that value is [`Send`].
  fn spawn_with<A, E, R, F>(&self, f: F) -> FiberHandle<A, E>
  where
    A: Clone + Send + Sync + 'static,
    E: Clone + Send + Sync + 'static,
    R: 'static,
    F: FnOnce() -> (Effect<A, E, R>, R) + Send + 'static;

  /// Same as [`Self::spawn_with`], with an explicit parent [`FiberId`] for hierarchy / tooling.
  ///
  /// Implementations in this workspace currently ignore `parent`; it is reserved for future
  /// scheduler policies.
  fn spawn_scoped_with<A, E, R, F>(&self, f: F, parent: FiberId) -> FiberHandle<A, E>
  where
    A: Clone + Send + Sync + 'static,
    E: Clone + Send + Sync + 'static,
    R: 'static,
    F: FnOnce() -> (Effect<A, E, R>, R) + Send + 'static,
  {
    let _ = parent;
    self.spawn_with(f)
  }

  /// Non-blocking sleep effect for this runtime.
  fn sleep(&self, duration: Duration) -> Effect<(), Never, ()>;
  /// Current time from the runtime clock.
  fn now(&self) -> Instant;
  /// Cooperative yield as an effect.
  fn yield_now(&self) -> Effect<(), Never, ()>;
}

/// [`Runtime`] that sleeps and yields using OS threads (no async driver).
///
/// Suitable for [`crate::scheduling::schedule::repeat`] / [`crate::scheduling::schedule::retry`] defaults and other
/// synchronous drivers. For Tokio-friendly delays inside an async runtime, use the `TokioRuntime`
/// type from workspace crate `effect_rs_tokio`.
#[derive(Clone, Copy, Debug, Default)]
pub struct ThreadSleepRuntime;

impl Runtime for ThreadSleepRuntime {
  fn spawn_with<A, E, R, F>(&self, f: F) -> FiberHandle<A, E>
  where
    A: Clone + Send + Sync + 'static,
    E: Clone + Send + Sync + 'static,
    R: 'static,
    F: FnOnce() -> (Effect<A, E, R>, R) + Send + 'static,
  {
    let handle = FiberHandle::pending(FiberId::fresh());
    let complete = handle.clone();
    std::thread::spawn(move || {
      let (effect, env) = f();
      complete.mark_completed(run_blocking(effect, env));
    });
    handle
  }

  fn sleep(&self, duration: Duration) -> Effect<(), Never, ()> {
    Effect::new(move |_env| {
      std::thread::sleep(duration);
      Ok::<(), Never>(())
    })
  }

  #[inline]
  fn now(&self) -> Instant {
    Instant::now()
  }

  fn yield_now(&self) -> Effect<(), Never, ()> {
    Effect::new(move |_env| {
      std::thread::yield_now();
      Ok::<(), Never>(())
    })
  }
}

/// Spawn a fiber using `runtime` (delegates to [`Runtime::spawn_with`]).
///
/// Pass a [`Send`] factory, e.g. `|| (succeed(42), ())`, so the [`Effect`] is built on the worker.
#[inline]
pub fn run_fork<RT, A, E, R, F>(runtime: &RT, f: F) -> FiberHandle<A, E>
where
  RT: Runtime,
  A: Clone + Send + Sync + 'static,
  E: Clone + Send + Sync + 'static,
  R: 'static,
  F: FnOnce() -> (Effect<A, E, R>, R) + Send + 'static,
{
  runtime.spawn_with(f)
}

/// Cooperative yield via `runtime` (delegates to [`Runtime::yield_now`]).
#[inline]
pub fn yield_now<RT>(runtime: &RT) -> Effect<(), Never, ()>
where
  RT: Runtime,
{
  runtime.yield_now()
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::FiberStatus;
  use crate::failure::Cause;
  use crate::kernel::{fail, succeed};
  use rstest::rstest;

  #[derive(Default)]
  struct TestRuntime;

  impl Runtime for TestRuntime {
    fn spawn_with<A, E, R, F>(&self, f: F) -> FiberHandle<A, E>
    where
      A: Clone + Send + Sync + 'static,
      E: Clone + Send + Sync + 'static,
      R: 'static,
      F: FnOnce() -> (Effect<A, E, R>, R) + Send + 'static,
    {
      ThreadSleepRuntime.spawn_with(f)
    }

    fn sleep(&self, duration: Duration) -> Effect<(), Never, ()> {
      Effect::new(move |_env| {
        std::thread::sleep(duration);
        Ok::<(), Never>(())
      })
    }

    fn now(&self) -> Instant {
      Instant::now()
    }

    fn yield_now(&self) -> Effect<(), Never, ()> {
      Effect::new(move |_env| {
        std::thread::yield_now();
        Ok::<(), Never>(())
      })
    }
  }

  mod runtime_contract {
    use super::*;

    #[test]
    fn now_when_called_sequentially_is_monotonic_enough_for_contract() {
      let rt = TestRuntime;
      let t1 = rt.now();
      let t2 = rt.now();
      assert!(t2 >= t1);
    }

    #[rstest]
    #[case::zero(Duration::from_millis(0))]
    #[case::short(Duration::from_millis(1))]
    fn sleep_when_invoked_returns_infallible_success(#[case] duration: Duration) {
      let rt = TestRuntime;
      let slept = pollster::block_on(rt.sleep(duration).run(&mut ()));
      assert_eq!(slept, Ok(()));
    }

    #[test]
    fn yield_now_when_invoked_returns_infallible_success() {
      let rt = TestRuntime;
      let yielded = pollster::block_on(rt.yield_now().run(&mut ()));
      assert_eq!(yielded, Ok(()));
    }

    #[test]
    fn spawn_and_spawn_scoped_when_called_return_distinct_fiber_ids() {
      let rt = TestRuntime;
      let h1 = rt.spawn_with(|| (succeed::<u8, (), ()>(1), ()));
      let h2 = rt.spawn_scoped_with(|| (succeed::<u8, (), ()>(2), ()), h1.id());
      assert_ne!(h1.id(), h2.id());
    }
  }

  mod thread_sleep_runtime {
    use super::*;

    #[test]
    fn when_constructed_runs_sleep_and_yield_effects_under_pollster() {
      let rt = ThreadSleepRuntime;
      let slept = pollster::block_on(rt.sleep(Duration::from_millis(0)).run(&mut ()));
      let yielded = pollster::block_on(rt.yield_now().run(&mut ()));
      assert_eq!(slept, Ok(()));
      assert_eq!(yielded, Ok(()));
    }

    #[test]
    fn run_fork_when_effect_succeeds_join_returns_value() {
      let rt = ThreadSleepRuntime;
      let h = run_fork(&rt, || (succeed::<u8, (), ()>(42), ()));
      assert_eq!(pollster::block_on(h.join()), Ok(42));
      assert_eq!(h.status(), FiberStatus::Succeeded);
    }

    #[test]
    fn run_fork_when_effect_fails_join_returns_fail_cause() {
      let rt = ThreadSleepRuntime;
      let h = run_fork(&rt, || (fail::<u8, &str, ()>("nope"), ()));
      assert_eq!(pollster::block_on(h.join()), Err(Cause::Fail("nope")));
      assert_eq!(h.status(), FiberStatus::Failed);
    }

    #[test]
    fn thread_sleep_runtime_now_returns_valid_instant() {
      let rt = ThreadSleepRuntime;
      let t = rt.now();
      assert!(t.elapsed().as_secs() < 5, "should be very recent");
    }
  }

  mod run_fork {
    use super::*;

    #[test]
    fn when_called_delegates_to_runtime_spawn_and_returns_positive_fiber_id() {
      let rt = TestRuntime;
      let handle = run_fork(&rt, || (succeed::<u8, (), ()>(5), ()));
      assert!(handle.id().as_u64() > 0);
    }
  }
}
