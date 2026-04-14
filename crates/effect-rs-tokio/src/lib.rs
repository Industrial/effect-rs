//! Tokio integration for [`effect`]: [`TokioRuntime`] implements [`effect::Runtime`] with
//! cooperative sleep/yield, and **runs forked effects** on Tokio’s **blocking thread pool** via
//! [`tokio::runtime::Handle::spawn_blocking`] (the `Effect` interpreter is driven with
//! [`run_blocking`]; it is not `Send` for [`tokio::spawn`]).
//!
//! Tower, Axum, and other Tokio-based adapters should depend on **`effect-tokio`** for this wiring.
//!
//! ## Examples
//!
//! See `examples/` (e.g. `109_tokio_end_to_end`). Re-exports
//! [`run_async`], [`run_blocking`], [`run_fork`], and [`yield_now`] from `effect` for use at the
//! async boundary alongside [`TokioRuntime`].

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use effect::{Effect, FiberHandle, FiberId, Never, Runtime, from_async};

/// Commonly used at the async boundary together with [`TokioRuntime`].
pub use effect::{run_async, run_blocking, run_fork, yield_now};

/// Tokio-backed [`Runtime`] adapter (async `sleep` / `yield_now`).
pub struct TokioRuntime {
  _owned: Option<Arc<tokio::runtime::Runtime>>,
  _handle: tokio::runtime::Handle,
}

impl TokioRuntime {
  /// Adapter for the current Tokio context.
  pub fn current() -> Result<Self, std::io::Error> {
    let handle = tokio::runtime::Handle::try_current()
      .map_err(|e| std::io::Error::other(format!("no current tokio runtime: {e}")))?;
    Ok(Self {
      _owned: None,
      _handle: handle,
    })
  }

  /// Adapter from an explicit Tokio handle (e.g. `axum::serve` / `#[tokio::main]`).
  #[inline]
  pub fn from_handle(handle: tokio::runtime::Handle) -> Self {
    Self {
      _owned: None,
      _handle: handle,
    }
  }

  /// Owns a single-threaded Tokio runtime (tests, examples, `main` without `#[tokio::main]`).
  pub fn new_current_thread() -> Result<Self, std::io::Error> {
    let runtime = tokio::runtime::Builder::new_current_thread()
      .enable_time()
      .build()?;
    let runtime = Arc::new(runtime);
    let handle = runtime.handle().clone();
    Ok(Self {
      _owned: Some(runtime),
      _handle: handle,
    })
  }

  /// Run a future on the owned runtime when this adapter was built with [`Self::new_current_thread`].
  ///
  /// When constructed with [`Self::from_handle`] / [`Self::current`], this panics — use the
  /// surrounding runtime’s `block_on` instead.
  pub fn block_on<F: std::future::Future>(&self, f: F) -> F::Output {
    match &self._owned {
      Some(rt) => rt.block_on(f),
      None => panic!(
        "TokioRuntime::block_on requires TokioRuntime::new_current_thread(); \
         otherwise use your Runtime::block_on / #[tokio::main] with from_handle"
      ),
    }
  }
}

impl Runtime for TokioRuntime {
  fn spawn_with<A, E, R, F>(&self, f: F) -> FiberHandle<A, E>
  where
    A: Clone + Send + Sync + 'static,
    E: Clone + Send + Sync + 'static,
    R: 'static,
    F: FnOnce() -> (Effect<A, E, R>, R) + Send + 'static,
  {
    let handle = FiberHandle::pending(FiberId::fresh());
    let h = handle.clone();
    let rt = self._handle.clone();
    // `run_async` is not `Send`; drive the effect with `run_blocking` on Tokio's blocking pool.
    let _join = rt.spawn_blocking(move || {
      let (effect, env) = f();
      h.mark_completed(run_blocking(effect, env));
    });
    handle
  }

  fn sleep(&self, duration: Duration) -> Effect<(), Never, ()> {
    from_async(move |_env| async move {
      tokio::time::sleep(duration).await;
      Ok::<(), Never>(())
    })
  }

  #[inline]
  fn now(&self) -> Instant {
    instant_now_blocking()
  }

  fn yield_now(&self) -> Effect<(), Never, ()> {
    from_async(move |_env| async move {
      tokio::task::yield_now().await;
      Ok::<(), Never>(())
    })
  }
}

#[inline]
fn instant_now_blocking() -> Instant {
  // Dylint `effect_no_instant_now_outside_boundary`: wall time is only allowed in `run_*`-adjacent
  // helpers (`*_blocking`); `Runtime::now` is the Tokio clock boundary for `LiveClock` / scheduling.
  Instant::now()
}

#[cfg(test)]
mod tests {
  use super::*;
  use effect::kernel::succeed;
  use std::time::Duration;

  #[test]
  fn new_current_thread_runs_sleep_and_yield_under_block_on() {
    let rt = TokioRuntime::new_current_thread().expect("tokio runtime should build");
    rt.block_on(async {
      assert_eq!(
        run_async(rt.sleep(Duration::from_millis(0)), ()).await,
        Ok(())
      );
      assert_eq!(run_async(yield_now(&rt), ()).await, Ok(()));
    });
  }

  #[test]
  fn spawn_runs_effect_to_completion_on_runtime() {
    let rt = TokioRuntime::new_current_thread().expect("tokio runtime should build");
    rt.block_on(async {
      let h = run_fork(&rt, || (succeed::<u8, (), ()>(7), ()));
      assert_eq!(h.join().await, Ok(7));
    });
  }
}
