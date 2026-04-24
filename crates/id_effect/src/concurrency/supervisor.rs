//! **Fiber supervision** — declarative restart, backoff, escalation, and scope integration (Phase F).
//!
//! A [`Supervisor`] ties a [`Scope`] fork to a [`CancellationToken`]: closing the parent [`Scope`]
//! cascades to the child scope, whose finalizer cancels the token so [`supervised`] loops stop
//! cooperatively with [`Cause::Interrupt`].
//!
//! ## Semantics vs [`Cause`]
//!
//! - **Typed failure** ([`Err`] from [`Effect::run`]) is surfaced as [`Cause::Fail`].
//! - **Interrupt** is modeled when the supervisor token is cancelled (parent scope closed, or
//!   manual [`CancellationToken::cancel`]), not from a successful `Result` return.
//! - **Defects** ([`Cause::Die`]) are not produced by plain `run`; panics inside the interpreter
//!   remain defects at the runtime boundary (see crate-level safety docs).

use crate::concurrency::cancel::CancellationToken;
use crate::failure::cause::Cause;
use crate::kernel::{Effect, box_future};
use crate::resource::scope::Scope;
use crate::runtime::{Never, Runtime, run_fork};
use crate::scheduling::clock::Clock;
use crate::scheduling::schedule::{Schedule, ScheduleInput};
use crate::{FiberHandle, FiberId, succeed};

/// Declarative policy for [`supervised`] loops.
///
/// `A` is only required for [`SupervisorPolicy::Ignore`], which supplies the success value used
/// when the child effect returns [`Err`].
#[derive(Clone)]
pub enum SupervisorPolicy<A: Clone = ()> {
  /// Run the child once; propagate the first [`Cause::Fail`] (or success).
  Terminate,
  /// On each [`Err`], wait per [`Schedule`] step then run the factory again. Success stops the loop.
  Restart {
    /// Backoff between attempts (`Schedule::spaced` with zero delay for tight loops in tests).
    schedule: Schedule,
  },
  /// Like [`SupervisorPolicy::Restart`], but after `limit` **retry attempts** have been consumed,
  /// the supervisor fails with an aggregated [`Cause::Then`] chain.
  ///
  /// `limit` is the number of **retries after failure** (not counting the initial run). For
  /// example, `limit == 0` escalates on the first [`Err`] without sleeping or retrying.
  RestartWithLimit {
    /// Maximum retry attempts after a failure.
    limit: u64,
    /// Backoff between attempts (same role as [`SupervisorPolicy::Restart::schedule`]).
    schedule: Schedule,
  },
  /// Fail fast on first [`Err`] (same operational shape as [`SupervisorPolicy::Terminate`] for a
  /// single child; kept for documentation parity with Effect.ts naming).
  Escalate,
  /// On [`Err`], complete with [`SupervisorPolicy::Ignore::recover`] instead of failing.
  Ignore {
    /// Success value substituted when the supervised child returns [`Err`].
    recover: A,
  },
}

/// Lightweight supervision handle: **child [`Scope`]** + **shutdown [`CancellationToken`]**.
///
/// Created with [`Supervisor::attach`], [`Supervisor::detached`], or [`Supervisor::from_parts`].
#[derive(Clone)]
pub struct Supervisor {
  scope: Scope,
  token: CancellationToken,
}

impl Supervisor {
  /// Forks a [`Scope`] under `parent` and wires lifecycle:
  /// - a **child finalizer** cancels [`Self::token`] when the child scope closes;
  /// - when the parent scope closes, the forked child scope closes via [`Scope`]'s hierarchy.
  pub fn attach(parent: &Scope) -> Self {
    let scope = parent.fork();
    let token = CancellationToken::new();
    let tok = token.clone();
    let _ = scope.add_finalizer(Box::new(move |_exit| {
      tok.cancel();
      succeed::<(), Never, ()>(())
    }));
    Self { scope, token }
  }

  /// Root scope with a fresh token (tests and demos without a parent [`Scope`]).
  pub fn detached() -> Self {
    let scope = Scope::make();
    let token = CancellationToken::new();
    let tok = token.clone();
    let _ = scope.add_finalizer(Box::new(move |_exit| {
      tok.cancel();
      succeed::<(), Never, ()>(())
    }));
    Self { scope, token }
  }

  /// Low-level constructor when callers manage finalizers themselves.
  #[inline]
  pub fn from_parts(scope: Scope, token: CancellationToken) -> Self {
    Self { scope, token }
  }

  /// Child [`Scope`] for attaching resources and finalizers.
  #[inline]
  pub fn scope(&self) -> &Scope {
    &self.scope
  }

  /// Cooperative shutdown signal (child effects should use [`check_interrupt`](crate::check_interrupt) on this token).
  #[inline]
  pub fn token(&self) -> &CancellationToken {
    &self.token
  }

  /// Spawn a supervised fiber on `runtime` using environment `env`.
  #[inline]
  pub fn spawn<RT, A, E, R, F, C>(
    &self,
    runtime: &RT,
    policy: SupervisorPolicy<A>,
    clock: C,
    env: R,
    make: F,
  ) -> FiberHandle<A, Cause<E>>
  where
    RT: Runtime,
    A: Clone + Send + Sync + 'static,
    E: Clone + Send + Sync + 'static,
    R: Send + 'static,
    F: FnMut() -> Effect<A, E, R> + Send + 'static,
    C: Clock + Clone + Send + Sync + 'static,
  {
    let sup = self.clone();
    run_fork(runtime, move || {
      (supervised(&sup, policy, clock, make), env)
    })
  }
}

/// Run `make` under `supervisor` until policy completes, honoring `clock` for schedule delays.
pub fn supervised<A, E, R, F, C>(
  supervisor: &Supervisor,
  policy: SupervisorPolicy<A>,
  clock: C,
  mut make: F,
) -> Effect<A, Cause<E>, R>
where
  A: Clone + Send + Sync + 'static,
  E: Clone + Send + Sync + 'static,
  R: 'static,
  F: FnMut() -> Effect<A, E, R> + 'static,
  C: Clock + Clone + 'static,
{
  let token = supervisor.token().clone();
  let supervisor_id = FiberId::fresh();
  Effect::new_async(move |r: &mut R| {
    let clock = clock.clone();
    box_future(async move {
      let mut schedule_restart: Option<Schedule> = None;
      let mut schedule_limited: Option<Schedule> = None;
      let mut aggregated: Option<Cause<E>> = None;
      let mut restarts_used: u64 = 0;
      let mut attempt: u64 = 0;

      loop {
        if token.is_cancelled() {
          return Err(Cause::Interrupt(supervisor_id));
        }

        match make().run(r).await {
          Ok(a) => return Ok(a),
          Err(e) => {
            let cause = Cause::Fail(e);
            match &policy {
              SupervisorPolicy::Terminate | SupervisorPolicy::Escalate => {
                return Err(cause);
              }
              SupervisorPolicy::Ignore { recover } => {
                return Ok(recover.clone());
              }
              SupervisorPolicy::Restart { schedule } => {
                let sched = schedule_restart.get_or_insert_with(|| schedule.clone());
                if let Some(sleep_eff) = sched.next_sleep(&clock, ScheduleInput { attempt }) {
                  match sleep_eff.run(&mut ()).await {
                    Ok(()) => {}
                    Err(never) => match never {},
                  }
                  attempt = attempt.saturating_add(1);
                  continue;
                } else {
                  return Err(cause);
                }
              }
              SupervisorPolicy::RestartWithLimit { limit, schedule } => {
                if restarts_used >= *limit {
                  return Err(match aggregated {
                    None => cause,
                    Some(prev) => Cause::then(prev, cause),
                  });
                }
                aggregated = Some(match aggregated {
                  None => cause.clone(),
                  Some(prev) => Cause::then(prev, cause.clone()),
                });
                let sched = schedule_limited.get_or_insert_with(|| schedule.clone());
                if let Some(sleep_eff) = sched.next_sleep(&clock, ScheduleInput { attempt }) {
                  match sleep_eff.run(&mut ()).await {
                    Ok(()) => {}
                    Err(never) => match never {},
                  }
                  restarts_used = restarts_used.saturating_add(1);
                  attempt = attempt.saturating_add(1);
                  continue;
                } else {
                  return Err(aggregated.take().unwrap_or(cause));
                }
              }
            }
          }
        }
      }
    })
  })
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::runtime::run_async;
  use crate::scheduling::duration::duration as duration_const;
  use crate::{TestClock, fail, succeed};
  use std::sync::Arc;
  use std::sync::atomic::{AtomicUsize, Ordering};
  use std::time::Instant;

  mod supervisor_attach {
    use super::*;

    #[tokio::test]
    async fn closing_parent_scope_cancels_supervisor_token_via_child_finalizer() {
      let parent = Scope::make();
      let sup = Supervisor::attach(&parent);
      assert!(!sup.token().is_cancelled());
      assert!(parent.close());
      assert!(sup.token().is_cancelled());
    }

    #[test]
    fn supervised_loop_observes_interrupt_when_token_cancelled() {
      let sup = Supervisor::detached();
      let token = sup.token().clone();
      let sup_thread = sup.clone();
      let calls = Arc::new(AtomicUsize::new(0));
      let calls_c = Arc::clone(&calls);
      let worker = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
          .enable_all()
          .build()
          .expect("current_thread runtime");
        let eff = supervised(
          &sup_thread,
          SupervisorPolicy::Restart {
            schedule: Schedule::spaced(duration_const::ZERO),
          },
          TestClock::new(Instant::now()),
          move || {
            calls_c.fetch_add(1, Ordering::SeqCst);
            fail::<(), &'static str, ()>("again")
          },
        );
        rt.block_on(run_async(eff, ()))
      });
      std::thread::sleep(std::time::Duration::from_millis(30));
      token.cancel();
      let out = worker
        .join()
        .expect("join supervised task")
        .expect_err("interrupt");
      assert!(matches!(out, Cause::Interrupt(_)));
      assert!(calls.load(Ordering::SeqCst) >= 1);
    }
  }

  mod supervised_policy {
    use super::*;

    mod terminate {
      use super::*;

      #[tokio::test]
      async fn returns_ok_on_first_success() {
        let sup = Supervisor::detached();
        let out = run_async(
          supervised(
            &sup,
            SupervisorPolicy::Terminate,
            TestClock::new(Instant::now()),
            || succeed::<u8, &'static str, ()>(7),
          ),
          (),
        )
        .await
        .expect("supervised");
        assert_eq!(out, 7);
      }

      #[tokio::test]
      async fn returns_fail_cause_on_first_error_without_retry() {
        let sup = Supervisor::detached();
        let out = run_async(
          supervised(
            &sup,
            SupervisorPolicy::Terminate,
            TestClock::new(Instant::now()),
            || fail::<u8, &'static str, ()>("boom"),
          ),
          (),
        )
        .await;
        assert_eq!(out, Err(Cause::Fail("boom")));
      }
    }

    mod restart {
      use super::*;

      #[tokio::test]
      async fn retries_until_child_succeeds_with_zero_delay_schedule() {
        let sup = Supervisor::detached();
        let n = Arc::new(AtomicUsize::new(0));
        let n_c = Arc::clone(&n);
        let out = run_async(
          supervised(
            &sup,
            SupervisorPolicy::Restart {
              schedule: Schedule::spaced(duration_const::ZERO),
            },
            TestClock::new(Instant::now()),
            move || {
              let c = Arc::clone(&n_c);
              let v = c.fetch_add(1, Ordering::SeqCst);
              if v < 2 {
                fail::<u8, &'static str, ()>("retry")
              } else {
                succeed::<u8, &'static str, ()>(42)
              }
            },
          ),
          (),
        )
        .await
        .expect("supervised");
        assert_eq!(out, 42);
        assert_eq!(n.load(Ordering::SeqCst), 3);
      }
    }

    mod restart_with_limit {
      use super::*;

      #[tokio::test]
      async fn escalates_after_limit_retries_with_then_aggregated_cause() {
        let sup = Supervisor::detached();
        let clock = TestClock::new(Instant::now());
        let n = Arc::new(AtomicUsize::new(0));
        let n_c = Arc::clone(&n);
        let out = run_async(
          supervised(
            &sup,
            SupervisorPolicy::RestartWithLimit {
              limit: 1,
              schedule: Schedule::spaced(duration_const::ZERO),
            },
            clock,
            move || {
              let c = Arc::clone(&n_c);
              let _ = c.fetch_add(1, Ordering::SeqCst);
              fail::<u8, &'static str, ()>("e")
            },
          ),
          (),
        )
        .await;
        let err = out.expect_err("expected escalation");
        match err {
          Cause::Then(a, b) => {
            assert!(matches!(*a, Cause::Fail("e")));
            assert!(matches!(*b, Cause::Fail("e")));
          }
          other => panic!("unexpected cause: {other:?}"),
        }
        assert_eq!(n.load(Ordering::SeqCst), 2);
      }

      #[tokio::test]
      async fn tight_restart_loop_stays_bounded_under_high_limit_with_test_clock() {
        let sup = Supervisor::detached();
        let clock = TestClock::new(Instant::now());
        let n = Arc::new(AtomicUsize::new(0));
        let n_c = Arc::clone(&n);
        let out = run_async(
          supervised(
            &sup,
            SupervisorPolicy::RestartWithLimit {
              limit: 50,
              schedule: Schedule::spaced(duration_const::ZERO),
            },
            clock,
            move || {
              n_c.fetch_add(1, Ordering::SeqCst);
              fail::<(), &'static str, ()>("x")
            },
          ),
          (),
        )
        .await;
        assert!(out.is_err());
        assert_eq!(n.load(Ordering::SeqCst), 51);
      }
    }

    mod ignore {
      use super::*;

      #[tokio::test]
      async fn returns_recover_value_when_child_fails() {
        let sup = Supervisor::detached();
        let out = run_async(
          supervised(
            &sup,
            SupervisorPolicy::Ignore { recover: 99_u8 },
            TestClock::new(Instant::now()),
            || fail::<u8, &'static str, ()>("ignored"),
          ),
          (),
        )
        .await
        .expect("ignore");
        assert_eq!(out, 99);
      }
    }

    mod escalate {
      use super::*;

      #[tokio::test]
      async fn behaves_like_terminate_for_single_child_failure() {
        let sup = Supervisor::detached();
        let out = run_async(
          supervised(
            &sup,
            SupervisorPolicy::Escalate,
            TestClock::new(Instant::now()),
            || fail::<u8, &'static str, ()>("up"),
          ),
          (),
        )
        .await;
        assert_eq!(out, Err(Cause::Fail("up")));
      }
    }
  }

  mod supervisor_spawn {
    use super::*;
    use crate::runtime::ThreadSleepRuntime;

    #[test]
    fn spawn_runs_supervised_body_on_runtime_worker() {
      let rt = ThreadSleepRuntime;
      let sup = Supervisor::detached();
      let h = sup.spawn(
        &rt,
        SupervisorPolicy::Terminate,
        TestClock::new(Instant::now()),
        (),
        || succeed::<u8, (), ()>(3),
      );
      let out = pollster::block_on(h.join());
      assert_eq!(out, Ok(3));
    }
  }
}
