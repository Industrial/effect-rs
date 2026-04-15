//! **Bifunctor** — a functor in two type parameters.
//!
//! A bifunctor is a type `F<_, _>` with operations to map over both
//! type parameters independently.
//!
//! ## Definition
//!
//! ```text
//! BIFUNCTOR[F] ::= (
//!   F<_, _>,
//!   bimap: (A → C) → (B → D) → F<A, B> → F<C, D>,
//!   first:  (A → C) → F<A, B> → F<C, B>,
//!   second: (B → D) → F<A, B> → F<A, D>
//! )
//! ```
//!
//! ## Laws
//!
//! - **Identity**: `bimap(id, id)(fab) = fab`
//! - **Composition**: `bimap(f1 ∘ f2, g1 ∘ g2) = bimap(f1, g1) ∘ bimap(f2, g2)`
//! - **First/Second**: `bimap(f, g) = first(f) ∘ second(g) = second(g) ∘ first(f)`
//!
//! ## Examples in this system
//!
//! - `(A, B)` — map over either or both components
//! - `Result<A, E>` — map over success or error
//! - `Either<L, R>` — map over left or right
//!
//! ## Relationship to Stratum 0 & 1
//!
//! - Uses: [`bimap_product`](super::super::foundation::product::bimap_product) for tuples
//! - Related to: [`Functor`](super::functor) (bifunctor restricted to one parameter)

/// A type with two type parameters that can each be mapped over.
///
/// # Laws
///
/// ```text
/// bimap(|x| x, |x| x)(fab) = fab                    // Identity
/// bimap(f, g) = first(f).second(g)                  // Decomposition
/// ```
pub trait Bifunctor {
  /// The first type parameter.
  type First;
  /// The second type parameter.
  type Second;

  /// Result type after mapping both parameters.
  type Output<C, D>;

  /// Map over both type parameters.
  fn bimap<C, D>(
    self,
    f: impl FnOnce(Self::First) -> C,
    g: impl FnOnce(Self::Second) -> D,
  ) -> Self::Output<C, D>;

  /// Map over the first type parameter only.
  fn map_first<C>(self, f: impl FnOnce(Self::First) -> C) -> Self::Output<C, Self::Second>
  where
    Self: Sized,
    Self::Second: Sized,
  {
    self.bimap(f, |b| b)
  }

  /// Map over the second type parameter only.
  fn map_second<D>(self, g: impl FnOnce(Self::Second) -> D) -> Self::Output<Self::First, D>
  where
    Self: Sized,
    Self::First: Sized,
  {
    self.bimap(|a| a, g)
  }
}

/// Map over both type parameters (free function).
#[inline]
pub fn bimap<F: Bifunctor, C, D>(
  fab: F,
  f: impl FnOnce(F::First) -> C,
  g: impl FnOnce(F::Second) -> D,
) -> F::Output<C, D> {
  fab.bimap(f, g)
}

/// Map over the first type parameter (free function).
#[inline]
pub fn map_first<F: Bifunctor, C>(fab: F, f: impl FnOnce(F::First) -> C) -> F::Output<C, F::Second>
where
  F::Second: Sized,
{
  fab.map_first(f)
}

/// Map over the second type parameter (free function).
#[inline]
pub fn map_second<F: Bifunctor, D>(fab: F, g: impl FnOnce(F::Second) -> D) -> F::Output<F::First, D>
where
  F::First: Sized,
{
  fab.map_second(g)
}

// ── Bifunctor Module Functions ───────────────────────────────────────────────

/// Bifunctor operations for tuples `(A, B)`.
pub mod tuple {
  /// Map over both components.
  #[inline]
  pub fn bimap<A, B, C, D>(pair: (A, B), f: impl FnOnce(A) -> C, g: impl FnOnce(B) -> D) -> (C, D) {
    (f(pair.0), g(pair.1))
  }

  /// Map over the first component.
  #[inline]
  pub fn map_first<A, B, C>(pair: (A, B), f: impl FnOnce(A) -> C) -> (C, B) {
    (f(pair.0), pair.1)
  }

  /// Map over the second component.
  #[inline]
  pub fn map_second<A, B, D>(pair: (A, B), g: impl FnOnce(B) -> D) -> (A, D) {
    (pair.0, g(pair.1))
  }

  /// Swap the components.
  #[inline]
  pub fn swap<A, B>(pair: (A, B)) -> (B, A) {
    (pair.1, pair.0)
  }
}

/// Bifunctor operations for `Result<A, E>`.
pub mod result {
  /// Map over both Ok and Err.
  #[inline]
  pub fn bimap<A, E, B, F>(
    result: Result<A, E>,
    on_ok: impl FnOnce(A) -> B,
    on_err: impl FnOnce(E) -> F,
  ) -> Result<B, F> {
    match result {
      Ok(a) => Ok(on_ok(a)),
      Err(e) => Err(on_err(e)),
    }
  }

  /// Map over the Ok value (same as Result::map).
  #[inline]
  pub fn map_first<A, E, B>(result: Result<A, E>, f: impl FnOnce(A) -> B) -> Result<B, E> {
    result.map(f)
  }

  /// Map over the Err value (same as Result::map_err).
  #[inline]
  pub fn map_second<A, E, F>(result: Result<A, E>, f: impl FnOnce(E) -> F) -> Result<A, F> {
    result.map_err(f)
  }

  /// Swap Ok and Err.
  #[inline]
  pub fn swap<A, E>(result: Result<A, E>) -> Result<E, A> {
    match result {
      Ok(a) => Err(a),
      Err(e) => Ok(e),
    }
  }
}

// ── Trait Implementations ────────────────────────────────────────────────────

impl<A, B> Bifunctor for (A, B) {
  type First = A;
  type Second = B;
  type Output<C, D> = (C, D);

  #[inline]
  fn bimap<C, D>(self, f: impl FnOnce(A) -> C, g: impl FnOnce(B) -> D) -> (C, D) {
    (f(self.0), g(self.1))
  }
}

impl<A, E> Bifunctor for Result<A, E> {
  type First = A;
  type Second = E;
  type Output<C, D> = Result<C, D>;

  #[inline]
  fn bimap<C, D>(self, f: impl FnOnce(A) -> C, g: impl FnOnce(E) -> D) -> Result<C, D> {
    match self {
      Ok(a) => Ok(f(a)),
      Err(e) => Err(g(e)),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use rstest::rstest;

  mod tuple_bifunctor {
    use super::*;

    #[test]
    fn bimap_transforms_both() {
      let pair = (5, "hello");
      let result = pair.bimap(|x| x * 2, |s| s.len());
      assert_eq!(result, (10, 5));
    }

    #[test]
    fn map_first_transforms_first() {
      let pair = (5, "hello");
      let result = pair.map_first(|x| x * 2);
      assert_eq!(result, (10, "hello"));
    }

    #[test]
    fn map_second_transforms_second() {
      let pair = (5, "hello");
      let result = pair.map_second(|s| s.len());
      assert_eq!(result, (5, 5));
    }

    #[test]
    fn identity_law() {
      let pair = (42, "test");
      let result = pair.bimap(|x| x, |s| s);
      assert_eq!(result, (42, "test"));
    }

    #[test]
    fn composition_law() {
      let pair = (5, 3);
      let f1 = |x: i32| x + 1;
      let f2 = |x: i32| x * 2;
      let g1 = |x: i32| x - 1;
      let g2 = |x: i32| x * 3;

      // bimap(f1 ∘ f2, g1 ∘ g2) = bimap(f1, g1) ∘ bimap(f2, g2)
      let left = pair.bimap(|x| f1(f2(x)), |y| g1(g2(y)));
      let right = pair.bimap(f2, g2).bimap(f1, g1);
      assert_eq!(left, right);
    }
  }

  mod result_bifunctor {
    use super::*;

    #[test]
    fn bimap_ok_transforms_first() {
      let r: Result<i32, &str> = Ok(5);
      let result = r.bimap(|x| x * 2, |e| e.len());
      assert_eq!(result, Ok(10));
    }

    #[test]
    fn bimap_err_transforms_second() {
      let r: Result<i32, &str> = Err("error");
      let result = r.bimap(|x| x * 2, |e| e.len());
      assert_eq!(result, Err(5));
    }

    #[test]
    fn identity_law() {
      let ok: Result<i32, &str> = Ok(42);
      let err: Result<i32, &str> = Err("e");

      assert_eq!(ok.clone().bimap(|x| x, |e| e), ok);
      assert_eq!(err.clone().bimap(|x| x, |e| e), err);
    }

    #[test]
    fn map_first_is_map() {
      let r: Result<i32, &str> = Ok(5);
      assert_eq!(r.clone().map_first(|x| x * 2), r.map(|x| x * 2));
    }

    #[test]
    fn map_second_is_map_err() {
      let r: Result<i32, &str> = Err("e");
      assert_eq!(r.clone().map_second(|e| e.len()), r.map_err(|e| e.len()));
    }
  }

  mod tuple_module {
    use super::*;

    #[test]
    fn bimap_function() {
      assert_eq!(tuple::bimap((1, 2), |x| x + 10, |y| y * 2), (11, 4));
    }

    #[test]
    fn swap_exchanges() {
      assert_eq!(tuple::swap((1, "hello")), ("hello", 1));
    }

    #[test]
    fn map_first_transforms_first_component() {
      assert_eq!(tuple::map_first((5, "hi"), |x| x * 2), (10, "hi"));
    }

    #[test]
    fn map_second_transforms_second_component() {
      assert_eq!(tuple::map_second((5, "hi"), |s| s.len()), (5, 2));
    }

    #[rstest]
    #[case::ints((1, 2), (2, 1))]
    #[case::same_type((5, 5), (5, 5))]
    fn swap_cases<T: PartialEq + std::fmt::Debug + Clone>(
      #[case] input: (T, T),
      #[case] expected: (T, T),
    ) {
      assert_eq!(tuple::swap(input), expected);
    }
  }

  mod result_module {
    use super::*;

    #[test]
    fn bimap_on_ok() {
      let r: Result<i32, i32> = Ok(5);
      assert_eq!(result::bimap(r, |x| x + 1, |e| e - 1), Ok(6));
    }

    #[test]
    fn bimap_on_err() {
      let r: Result<i32, i32> = Err(5);
      assert_eq!(result::bimap(r, |x| x + 1, |e| e - 1), Err(4));
    }

    #[test]
    fn swap_ok_to_err() {
      assert_eq!(result::swap(Ok::<i32, &str>(5)), Err(5));
    }

    #[test]
    fn swap_err_to_ok() {
      assert_eq!(result::swap(Err::<i32, &str>("e")), Ok("e"));
    }

    #[test]
    fn map_first_on_ok() {
      assert_eq!(result::map_first(Ok::<i32, &str>(3), |x| x + 1), Ok(4));
    }

    #[test]
    fn map_first_on_err_is_noop() {
      assert_eq!(
        result::map_first(Err::<i32, &str>("e"), |x| x + 1),
        Err("e")
      );
    }

    #[test]
    fn map_second_on_err() {
      assert_eq!(
        result::map_second(Err::<i32, &str>("e"), |e| e.len()),
        Err(1)
      );
    }

    #[test]
    fn map_second_on_ok_is_noop() {
      assert_eq!(result::map_second(Ok::<i32, &str>(5), |e| e.len()), Ok(5));
    }
  }

  mod free_functions {
    use super::*;

    #[test]
    fn bimap_on_tuple() {
      assert_eq!(bimap((1, 2), |x| x + 10, |y| y * 2), (11, 4));
    }

    #[test]
    fn map_first_on_tuple() {
      assert_eq!(map_first((1, 2), |x| x + 10), (11, 2));
    }

    #[test]
    fn map_second_on_tuple() {
      assert_eq!(map_second((1, 2), |y| y * 2), (1, 4));
    }
  }

  mod laws {
    use super::*;

    #[test]
    fn bimap_first_then_second_equals_bimap() {
      let pair = (3, 7);
      let f = |x: i32| x + 1;
      let g = |x: i32| x * 2;

      let via_bimap = pair.bimap(f, g);
      let via_steps = pair.map_first(f).map_second(g);
      assert_eq!(via_bimap, via_steps);
    }

    #[test]
    fn bimap_second_then_first_equals_bimap() {
      let pair = (3, 7);
      let f = |x: i32| x + 1;
      let g = |x: i32| x * 2;

      let via_bimap = pair.bimap(f, g);
      let via_steps = pair.map_second(g).map_first(f);
      assert_eq!(via_bimap, via_steps);
    }
  }
}
