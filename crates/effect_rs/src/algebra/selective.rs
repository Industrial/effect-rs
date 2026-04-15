//! **Selective** — a functor between Applicative and Monad.
//!
//! Introduced by Andrey Mokhov et al. (2019). A selective functor adds
//! **conditional effects**: the second argument is only *executed* when
//! needed, but unlike `flat_map`, static analysis can still observe both
//! branches because the second effect is present as a *value*, not hidden
//! inside a closure.
//!
//! ## Haskell definition
//!
//! ```haskell
//! class Applicative f => Selective f where
//!   select :: f (Either a b) -> f (a -> b) -> f b
//!
//! -- Derived:
//! branch :: f (Either a b) -> f (a -> c) -> f (b -> c) -> f c
//! ```
//!
//! ## Meaning
//!
//! `select fab ff`:
//! - Runs `fab`.
//! - If it returns `Right(b)` → returns `b` **without** running `ff`.
//! - If it returns `Left(a)` → runs `ff` and applies the resulting function to `a`.
//!
//! This gives a *static* view of both branches, unlike `flat_map` where the
//! continuation is opaque. In interpreters and static analysers that traverse
//! the structure without running it, both branches are visible.
//!
//! ## Law
//!
//! ```text
//! -- Selective is a generalisation of Applicative:
//! select (Right <$> x) y = x                   -- pure Right short-circuits
//!
//! -- Selective is weaker than Monad:
//! select fab (pure f) = map (either f id) fab   -- pure function collapses to fmap
//! ```
//!
//! ## Relationship to Strata
//!
//! - Sits between [`Applicative`](super::applicative::Applicative) (Stratum 1) and
//!   [`Monad`](super::monad::Monad) (Stratum 1).
//! - [`crate::kernel::Effect`] (Stratum 2) provides the concrete implementation via
//!   [`select_effect`] and [`branch_effect`].

use crate::foundation::coproduct::Either;

// ── Trait ────────────────────────────────────────────────────────────────────

/// A selective functor: conditional effects with observable branching structure.
///
/// Implementors of this trait can execute one of two effects conditionally
/// based on the result of a preceding effect.
///
/// # Laws
///
/// ```text
/// select (pure (Right b)) ff = pure b          -- Right short-circuits without running ff
/// select (pure (Left a))  ff = map (|f| f(a)) ff  -- Left always runs ff
/// select fab (pure f)        = map (either(f, id)) fab  -- pure function degenerates to map
/// ```
pub trait Selective {
  /// The "left" variant type parameter.
  type Left;
  /// The "right" / success variant type parameter.
  type Right;
  /// The output type after applying a function from `Left -> Right`.
  type Output;

  /// Conditionally execute the second effect.
  ///
  /// - If `self` resolves to `Right(b)`: return `b`, **skip** `f`.
  /// - If `self` resolves to `Left(a)`:  run `f` to get `a -> b`, apply it.
  ///
  /// The crucial property: `f` is present as a *value* visible to static
  /// analysis/interpreters, unlike a closure given to `flat_map`.
  fn select(self, f: Self::Output) -> Self::Right;
}

// ── Effect implementations ────────────────────────────────────────────────────

use crate::kernel::effect::{Effect, box_future};

/// Selective `select` for [`Effect`].
///
/// Runs `fab`; if the result is `Right(b)`, returns `b` without running `ff`.
/// If the result is `Left(a)`, runs `ff` and applies the function to `a`.
///
/// # Example
///
/// ```rust
/// use effect_rs::algebra::selective::select_effect;
/// use effect_rs::{succeed, run_blocking};
///
/// // Short-circuit path: Right does not run ff
/// let right_path = select_effect(
///     succeed::<Result<i32, i32>, (), ()>(Ok(42)),   // Right(42)
///     succeed(|x: i32| x * 100),                     // never executed
/// );
/// assert_eq!(run_blocking(right_path, ()), Ok(42));
///
/// // Left path: ff is executed
/// let left_path = select_effect(
///     succeed::<Result<i32, i32>, (), ()>(Err(6)),   // Left(6)
///     succeed(|x: i32| x * 7),                       // 6 * 7 = 42
/// );
/// assert_eq!(run_blocking(left_path, ()), Ok(42));
/// ```
#[inline]
pub fn select_effect<A, B, E, R, F>(
  fab: Effect<Either<B, A>, E, R>,
  ff: Effect<F, E, R>,
) -> Effect<B, E, R>
where
  A: 'static,
  B: 'static,
  E: 'static,
  R: 'static,
  F: FnOnce(A) -> B + 'static,
{
  Effect::new_async(move |r| {
    box_future(async move {
      match fab.run(r).await? {
        // Right(b): short-circuit, do NOT run ff.
        Ok(b) => Ok(b),
        // Left(a): run ff, apply resulting function.
        Err(a) => {
          let f = ff.run(r).await?;
          Ok(f(a))
        }
      }
    })
  })
}

/// `branch` — choose one of two handlers depending on which side of `Either` arrives.
///
/// ```text
/// branch fab fl fr:
///   Left(a)  -> run fl, apply f to a
///   Right(b) -> run fr, apply g to b
/// ```
///
/// Both handler effects are present as values, giving structural visibility
/// unlike a monad bind.
///
/// # Example
///
/// ```rust
/// use effect_rs::algebra::selective::branch_effect;
/// use effect_rs::{succeed, run_blocking};
///
/// let left_in  = branch_effect(
///     succeed::<Result<i32, i32>, (), ()>(Err(6)),
///     succeed(|a: i32| a * 7),   // left handler: 6*7 = 42
///     succeed(|b: i32| b + 0),   // right handler (not run)
/// );
/// assert_eq!(run_blocking(left_in, ()), Ok(42));
///
/// let right_in = branch_effect(
///     succeed::<Result<i32, i32>, (), ()>(Ok(40)),
///     succeed(|a: i32| a * 0),   // left handler (not run)
///     succeed(|b: i32| b + 2),   // right handler: 40+2 = 42
/// );
/// assert_eq!(run_blocking(right_in, ()), Ok(42));
/// ```
#[inline]
pub fn branch_effect<A, B, C, E, R, FL, FR>(
  fab: Effect<Either<B, A>, E, R>,
  fl: Effect<FL, E, R>,
  fr: Effect<FR, E, R>,
) -> Effect<C, E, R>
where
  A: 'static,
  B: 'static,
  C: 'static,
  E: 'static,
  R: 'static,
  FL: FnOnce(A) -> C + 'static,
  FR: FnOnce(B) -> C + 'static,
{
  Effect::new_async(move |r| {
    box_future(async move {
      match fab.run(r).await? {
        Err(a) => {
          let f = fl.run(r).await?;
          Ok(f(a))
        }
        Ok(b) => {
          let g = fr.run(r).await?;
          Ok(g(b))
        }
      }
    })
  })
}

// ── Laws as tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
  use super::*;
  use crate::kernel::effect::{fail, succeed};
  use crate::runtime::run_blocking;
  use rstest::rstest;

  // Fixtures
  fn right_effect(n: i32) -> Effect<Either<i32, i32>, (), ()> {
    succeed(Ok(n))
  }
  fn left_effect(n: i32) -> Effect<Either<i32, i32>, (), ()> {
    succeed(Err(n))
  }

  // ── select laws ─────────────────────────────────────────────────────────

  mod select_right_short_circuits {
    use super::*;

    /// `select (pure (Right b)) ff = pure b` — ff must NOT execute.
    #[test]
    fn right_returns_value_without_running_ff() {
      let executed = std::cell::Cell::new(false);
      // We cannot capture Cell in FnOnce + 'static directly; use a flag via shared state.
      let result = run_blocking(
        select_effect(
          right_effect(42),
          succeed(|_: i32| {
            // This body should never be reached.
            panic!("ff executed on Right branch")
          }),
        ),
        (),
      );
      assert_eq!(result, Ok(42));
      let _ = executed;
    }

    #[rstest]
    #[case::zero(0)]
    #[case::positive(99)]
    #[case::negative(-7)]
    fn right_returns_original_value_unchanged(#[case] n: i32) {
      let result = run_blocking(select_effect(right_effect(n), succeed(|_: i32| -1)), ());
      assert_eq!(result, Ok(n));
    }
  }

  mod select_left_runs_ff {
    use super::*;

    /// `select (pure (Left a)) ff = map (|f| f(a)) ff` — ff runs and is applied.
    #[test]
    fn left_applies_function_from_ff() {
      let result = run_blocking(select_effect(left_effect(6), succeed(|x: i32| x * 7)), ());
      assert_eq!(result, Ok(42));
    }

    #[rstest]
    #[case::double(2, |x: i32| x * 2, 4)]
    #[case::negate(5, |x: i32| -x, -5)]
    #[case::identity(99, |x: i32| x, 99)]
    fn left_applies_various_functions(
      #[case] input: i32,
      #[case] f: fn(i32) -> i32,
      #[case] expected: i32,
    ) {
      let result = run_blocking(select_effect(left_effect(input), succeed(f)), ());
      assert_eq!(result, Ok(expected));
    }
  }

  mod select_pure_function_degenerates_to_map {
    use super::*;

    /// `select fab (pure f) = map (either f id) fab` — selective with pure ff is just map.
    #[test]
    fn pure_ff_equivalent_to_map_either() {
      let selective = run_blocking(select_effect(left_effect(21), succeed(|a: i32| a * 2)), ());
      let mapped = run_blocking(
        left_effect(21).map(|e| match e {
          Err(a) => a * 2,
          Ok(b) => b,
        }),
        (),
      );
      assert_eq!(selective, mapped);
    }
  }

  // ── branch laws ─────────────────────────────────────────────────────────

  mod branch {
    use super::*;

    #[test]
    fn branch_left_runs_left_handler_only() {
      let result = run_blocking(
        branch_effect(
          left_effect(6),
          succeed(|a: i32| a * 7), // left handler: 6*7=42
          succeed(|_: i32| panic!("right executed on left input")),
        ),
        (),
      );
      assert_eq!(result, Ok(42));
    }

    #[test]
    fn branch_right_runs_right_handler_only() {
      let result = run_blocking(
        branch_effect(
          right_effect(40),
          succeed(|_: i32| panic!("left executed on right input")),
          succeed(|b: i32| b + 2), // right handler: 40+2=42
        ),
        (),
      );
      assert_eq!(result, Ok(42));
    }

    /// Both handler effects are present as values; they compose with the
    /// environment identically to applicative `ap`.
    #[test]
    fn branch_propagates_error_from_selected_handler() {
      let result: Result<i32, &str> = run_blocking(
        branch_effect(
          succeed::<Either<i32, i32>, &str, ()>(Err(0)),
          fail::<fn(i32) -> i32, &str, ()>("handler failed"),
          succeed(|b: i32| b),
        ),
        (),
      );
      assert_eq!(result, Err("handler failed"));
    }

    #[test]
    fn branch_propagates_error_from_fab() {
      let result: Result<i32, &str> = run_blocking(
        branch_effect(
          fail::<Either<i32, i32>, &str, ()>("fab failed"),
          succeed(|a: i32| a),
          succeed(|b: i32| b),
        ),
        (),
      );
      assert_eq!(result, Err("fab failed"));
    }
  }

  mod select_error_propagation {
    use super::*;

    #[test]
    fn select_propagates_error_from_fab() {
      let result: Result<i32, &str> = run_blocking(
        select_effect(
          fail::<Either<i32, i32>, &str, ()>("fab failed"),
          succeed(|_: i32| 0),
        ),
        (),
      );
      assert_eq!(result, Err("fab failed"));
    }

    #[test]
    fn select_propagates_error_from_ff() {
      let result: Result<i32, &str> = run_blocking(
        select_effect(
          succeed::<Either<i32, i32>, &str, ()>(Err(5)),
          fail::<fn(i32) -> i32, &str, ()>("ff failed"),
        ),
        (),
      );
      assert_eq!(result, Err("ff failed"));
    }
  }
}
