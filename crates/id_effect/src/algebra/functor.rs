//! **Functor** — a type constructor that can be mapped over.
//!
//! A functor is a type `F<_>` with a `map` operation that applies a function
//! to the "inside" while preserving the structure.
//!
//! ## Definition
//!
//! ```text
//! FUNCTOR[F] ::= (F<_>, map: (A → B) → F<A> → F<B>)
//! ```
//!
//! ## Laws
//!
//! - **Identity**: `map(id)(fa) = fa`
//! - **Composition**: `map(f ∘ g) = map(f) ∘ map(g)`
//!
//! ## Examples in this system
//!
//! - `Option<A>` — maps over the inner value if `Some`
//! - `Result<A, E>` — maps over the `Ok` value
//! - `Vec<A>` — maps over each element
//! - `Effect<A, E, R>` — maps over the success value
//!
//! ## Relationship to Stratum 0
//!
//! - Uses: [`identity`](super::super::foundation::function::identity),
//!         [`compose`](super::super::foundation::function::compose) for laws
//! - Used by: [`Applicative`](super::applicative), [`Monad`](super::monad)

/// A type that can be mapped over (covariant functor).
///
/// # Laws
///
/// Implementations must satisfy:
///
/// ```text
/// fa.map(|x| x) = fa                              // Identity
/// fa.map(|x| f(g(x))) = fa.map(g).map(f)          // Composition
/// ```
///
/// # Note on Higher-Kinded Types
///
/// Rust lacks HKT, so this trait uses an associated type `Output<B>` to
/// represent the result of mapping. Implementors must ensure the "shape"
/// is preserved (e.g., `Option<A>.map(f)` returns `Option<B>`).
pub trait Functor {
  /// The type inside the functor.
  type Inner;

  /// The result of mapping to type `B`.
  type Output<B>;

  /// Apply a function to the inner value(s).
  fn map<B>(self, f: impl FnOnce(Self::Inner) -> B) -> Self::Output<B>;
}

/// Map a function over a functor (free function).
#[inline]
pub fn map<F: Functor, B>(fa: F, f: impl FnOnce(F::Inner) -> B) -> F::Output<B> {
  fa.map(f)
}

/// Replace all inner values with a constant.
///
/// `as_(fa, b) = fa.map(|_| b)`
#[inline]
pub fn as_<F: Functor, B: Clone>(fa: F, value: B) -> F::Output<B> {
  fa.map(|_| value)
}

/// Flip the arguments of map for point-free style.
///
/// `map_to(f)(fa) = fa.map(f)`
#[inline]
pub fn map_to<A, B>(f: impl FnOnce(A) -> B) -> impl FnOnce(Option<A>) -> Option<B> {
  move |fa| fa.map(f)
}

// ── Functor Module Functions ─────────────────────────────────────────────────

/// Functor operations for `Option<A>`.
pub mod option {
  /// Map a function over an Option.
  #[inline]
  pub fn map<A, B>(fa: Option<A>, f: impl FnOnce(A) -> B) -> Option<B> {
    fa.map(f)
  }

  /// Replace the inner value with a constant if Some.
  #[inline]
  pub fn as_<A, B>(fa: Option<A>, value: B) -> Option<B> {
    fa.map(|_| value)
  }

  /// Discard the inner value, keeping the structure.
  #[inline]
  pub fn void<A>(fa: Option<A>) -> Option<()> {
    fa.map(|_| ())
  }

  /// Tuple the inner value with the result of applying f.
  #[inline]
  pub fn tap<A: Clone, B>(fa: Option<A>, f: impl FnOnce(&A) -> B) -> Option<(A, B)> {
    fa.map(|a| {
      let b = f(&a);
      (a, b)
    })
  }
}

/// Functor operations for `Result<A, E>`.
pub mod result {
  /// Map a function over the Ok value.
  #[inline]
  pub fn map<A, B, E>(fa: Result<A, E>, f: impl FnOnce(A) -> B) -> Result<B, E> {
    fa.map(f)
  }

  /// Replace the Ok value with a constant.
  #[inline]
  pub fn as_<A, B, E>(fa: Result<A, E>, value: B) -> Result<B, E> {
    fa.map(|_| value)
  }

  /// Discard the Ok value, keeping the structure.
  #[inline]
  pub fn void<A, E>(fa: Result<A, E>) -> Result<(), E> {
    fa.map(|_| ())
  }
}

/// Functor operations for `Vec<A>`.
pub mod vec {
  use rayon::prelude::*;

  /// Map a function over each element.
  #[inline]
  pub fn map<A, B>(fa: Vec<A>, f: impl FnMut(A) -> B) -> Vec<B> {
    fa.into_iter().map(f).collect()
  }

  /// Like [`map`], but applies `f` in parallel (Rayon). `A` and `B` must be `Send`, and `f` must be
  /// `Sync` (shared by worker threads).
  #[inline]
  pub fn map_par<A, B, F>(fa: Vec<A>, f: F) -> Vec<B>
  where
    A: Send,
    B: Send,
    F: Fn(A) -> B + Send + Sync,
  {
    fa.into_par_iter().map(f).collect()
  }

  /// Replace all elements with a constant.
  #[inline]
  pub fn as_<A, B: Clone>(fa: Vec<A>, value: B) -> Vec<B> {
    fa.into_iter().map(|_| value.clone()).collect()
  }

  /// Discard all element values.
  #[inline]
  pub fn void<A>(fa: Vec<A>) -> Vec<()> {
    fa.into_iter().map(|_| ()).collect()
  }
}

// ── Trait Implementations ────────────────────────────────────────────────────

impl<A> Functor for Option<A> {
  type Inner = A;
  type Output<B> = Option<B>;

  #[inline]
  fn map<B>(self, f: impl FnOnce(A) -> B) -> Option<B> {
    self.map(f)
  }
}

impl<A, E> Functor for Result<A, E> {
  type Inner = A;
  type Output<B> = Result<B, E>;

  #[inline]
  fn map<B>(self, f: impl FnOnce(A) -> B) -> Result<B, E> {
    self.map(f)
  }
}

impl<A> Functor for Vec<A> {
  type Inner = A;
  type Output<B> = Vec<B>;

  #[inline]
  fn map<B>(self, f: impl FnOnce(A) -> B) -> Vec<B> {
    // Note: FnOnce doesn't work well with iterators, so we use a workaround
    // For Vec we typically want FnMut, but the trait requires FnOnce
    // This implementation only works for single-element vecs with FnOnce
    // In practice, use the vec::map function with FnMut
    let mut f = Some(f);
    self
      .into_iter()
      .map(|a| {
        if let Some(func) = f.take() {
          func(a)
        } else {
          panic!("FnOnce called multiple times in Vec::map")
        }
      })
      .collect()
  }
}

impl<A, const N: usize> Functor for [A; N] {
  type Inner = A;
  type Output<B> = [B; N];

  #[inline]
  fn map<B>(self, f: impl FnOnce(A) -> B) -> [B; N] {
    let mut f = Some(f);
    self.map(|a| {
      if let Some(func) = f.take() {
        func(a)
      } else {
        panic!("FnOnce called multiple times in array::map")
      }
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use rstest::rstest;

  mod option_functor {
    use super::*;

    #[test]
    fn map_some_applies_function() {
      assert_eq!(Some(3).map(|x| x * 2), Some(6));
    }

    #[test]
    fn map_none_returns_none() {
      assert_eq!(None::<i32>.map(|x| x * 2), None);
    }

    #[test]
    fn identity_law() {
      let fa = Some(42);
      assert_eq!(fa.map(|x| x), fa);
    }

    #[test]
    fn composition_law() {
      let fa = Some(5);
      let f = |x: i32| x + 1;
      let g = |x: i32| x * 2;

      // map(f ∘ g) = map(f) ∘ map(g)
      let left = fa.map(|x| f(g(x)));
      let right = fa.map(g).map(f);
      assert_eq!(left, right);
    }

    #[rstest]
    #[case::positive(Some(5_i32), 10)]
    #[case::zero(Some(0_i32), 0)]
    #[case::negative(Some(-3_i32), -6)]
    fn map_doubles(#[case] input: Option<i32>, #[case] expected: i32) {
      assert_eq!(input.map(|x| x * 2), Some(expected));
    }
  }

  mod result_functor {
    #[test]
    fn map_ok_applies_function() {
      assert_eq!(Ok::<i32, &str>(3).map(|x| x * 2), Ok(6));
    }

    #[test]
    fn map_err_returns_err() {
      assert_eq!(Err::<i32, &str>("fail").map(|x| x * 2), Err("fail"));
    }

    #[test]
    fn identity_law() {
      let fa: Result<i32, &str> = Ok(42);
      assert_eq!(fa.clone().map(|x| x), fa);
    }

    #[test]
    fn composition_law() {
      let fa: Result<i32, &str> = Ok(5);
      let f = |x: i32| x + 1;
      let g = |x: i32| x * 2;

      let left = fa.clone().map(|x| f(g(x)));
      let right = fa.map(g).map(f);
      assert_eq!(left, right);
    }
  }

  mod option_module {
    use super::*;

    #[test]
    fn as_replaces_value() {
      assert_eq!(option::as_(Some(5), "replaced"), Some("replaced"));
    }

    #[test]
    fn as_none_returns_none() {
      assert_eq!(option::as_(None::<i32>, "replaced"), None);
    }

    #[test]
    fn void_discards_value() {
      assert_eq!(option::void(Some(42)), Some(()));
    }

    #[test]
    fn tap_tuples_with_derived() {
      let result = option::tap(Some(5), |x| x * 2);
      assert_eq!(result, Some((5, 10)));
    }
  }

  mod result_module {
    use super::*;

    #[test]
    fn as_replaces_ok_value() {
      assert_eq!(result::as_(Ok::<i32, &str>(5), "replaced"), Ok("replaced"));
    }

    #[test]
    fn void_discards_ok_value() {
      assert_eq!(result::void(Ok::<i32, &str>(42)), Ok(()));
    }

    #[test]
    fn void_preserves_err() {
      assert_eq!(result::void(Err::<i32, &str>("e")), Err("e"));
    }
  }

  mod vec_module {
    use super::*;

    #[test]
    fn map_transforms_elements() {
      assert_eq!(vec::map(vec![1, 2, 3], |x| x * 2), vec![2, 4, 6]);
    }

    #[test]
    fn map_par_transforms_elements() {
      assert_eq!(vec::map_par(vec![1, 2, 3, 4], |x| x * 2), vec![2, 4, 6, 8]);
    }

    #[test]
    fn map_empty_vec() {
      assert_eq!(
        vec::map(Vec::<i32>::new(), |x: i32| x * 2),
        Vec::<i32>::new()
      );
    }

    #[test]
    fn as_replaces_all() {
      assert_eq!(vec::as_(vec![1, 2, 3], "x"), vec!["x", "x", "x"]);
    }

    #[test]
    fn void_discards_all() {
      assert_eq!(vec::void(vec![1, 2, 3]), vec![(), (), ()]);
    }
  }

  mod free_functions {
    use super::*;

    #[test]
    fn map_works_on_option() {
      assert_eq!(map(Some(3), |x| x + 1), Some(4));
    }

    #[test]
    fn as_works_on_option() {
      assert_eq!(as_(Some(3), "const"), Some("const"));
    }
  }

  mod laws {
    #[test]
    fn option_identity_law_exhaustive() {
      for val in [None, Some(0), Some(42), Some(-7)] {
        assert_eq!(val.map(|x| x), val, "identity failed for {val:?}");
      }
    }

    #[test]
    fn result_identity_law_exhaustive() {
      let cases: Vec<Result<i32, &str>> = vec![Ok(0), Ok(42), Err("e")];
      for val in cases {
        assert_eq!(val.clone().map(|x| x), val, "identity failed for {val:?}");
      }
    }

    #[test]
    fn option_composition_law_exhaustive() {
      let f = |x: i32| x + 10;
      let g = |x: i32| x * 3;

      for val in [None, Some(0), Some(5), Some(-2)] {
        let left = val.map(|x| f(g(x)));
        let right = val.map(g).map(f);
        assert_eq!(left, right, "composition failed for {val:?}");
      }
    }
  }

  // ── Property-based functor laws (proptest) ─────────────────────────────────

  mod property_laws {
    use proptest::prelude::*;

    proptest! {
      /// Functor identity law for Option: `fmap(id) = id`.
      #[test]
      fn option_functor_identity(x: Option<i8>) {
        prop_assert_eq!(x.map(|v| v), x);
      }

      /// Functor composition law for Option:
      /// `fmap(f ∘ g) = fmap(f) ∘ fmap(g)`.
      #[test]
      fn option_functor_composition(x: Option<i8>, a in 0i8..10, b in 0i8..10) {
        let f = move |v: i8| v.saturating_add(a);
        let g = move |v: i8| v.saturating_mul(b);
        let composed = x.map(move |v| f(g(v)));
        let sequential = x.map(g).map(f);
        prop_assert_eq!(composed, sequential);
      }

      /// Functor identity law for Result<i32, &str>.
      #[test]
      fn result_functor_identity(ok: bool, n: i32) {
        let x: Result<i32, &str> = if ok { Ok(n) } else { Err("e") };
        prop_assert_eq!(x.map(|v| v), x);
      }

      /// Functor composition law for Result.
      #[test]
      fn result_functor_composition(n: i8, a in 0i8..10, b in 0i8..10) {
        let x: Result<i8, &str> = Ok(n);
        let f = move |v: i8| v.saturating_add(a);
        let g = move |v: i8| v.saturating_mul(b);
        prop_assert_eq!(x.map(move |v| f(g(v))), x.map(g).map(f));
      }
    }
  }

  // ── map_to free function ───────────────────────────────────────────────────

  mod map_to_fn {
    use super::*;

    #[test]
    fn map_to_applies_to_some() {
      let f = map_to(|x: i32| x * 3);
      assert_eq!(f(Some(4)), Some(12));
    }

    #[test]
    fn map_to_applies_to_none() {
      let f = map_to(|x: i32| x + 1);
      assert_eq!(f(None), None);
    }
  }

  // ── Functor trait for [A; N] ──────────────────────────────────────────────

  mod array_functor {
    #[test]
    fn array_map_single_element() {
      let arr: [i32; 1] = [5];
      let result = arr.map(|x| x * 2);
      assert_eq!(result, [10]);
    }

    #[test]
    fn array_zero_size_map() {
      let arr: [i32; 0] = [];
      let result: [String; 0] = arr.map(|x| x.to_string());
      assert_eq!(result, [] as [String; 0]);
    }
  }

  // ── Functor trait for Vec<A> (single-element, FnOnce path) ──────────────

  mod vec_functor_trait {
    use super::*;

    #[test]
    fn vec_map_single_element_via_trait() {
      let v: Vec<i32> = vec![7];
      let result = Functor::map(v, |x| x + 10);
      assert_eq!(result, vec![17]);
    }

    #[test]
    fn vec_empty_map_via_trait() {
      let v: Vec<i32> = vec![];
      let result = Functor::map(v, |x: i32| x * 2);
      assert_eq!(result, Vec::<i32>::new());
    }
  }

  // ── Free function map / as_ ───────────────────────────────────────────────

  mod free_functions_result {
    use super::*;

    #[test]
    fn free_map_ok_result() {
      let r: Result<i32, &str> = Ok(10);
      assert_eq!(map(r, |x| x + 1), Ok(11));
    }

    #[test]
    fn free_map_err_result() {
      let r: Result<i32, &str> = Err("fail");
      let mapped = map(r, |x: i32| x + 1);
      assert_eq!(mapped, Err("fail"));
    }

    #[test]
    fn free_as_replaces_ok_value() {
      let r: Result<i32, &str> = Ok(42);
      let result = as_(r, "replaced");
      assert_eq!(result, Ok("replaced"));
    }
  }

  mod result_module_fns {
    use super::*;

    #[test]
    fn result_map_ok() {
      assert_eq!(result::map(Ok::<i32, &str>(5), |x| x * 2), Ok(10));
    }

    #[test]
    fn result_map_err_passes_through() {
      assert_eq!(result::map(Err::<i32, &str>("e"), |x| x * 2), Err("e"));
    }

    #[test]
    fn result_as_ok() {
      assert_eq!(result::as_(Ok::<i32, &str>(5), "c"), Ok("c"));
    }

    #[test]
    fn result_void_ok() {
      assert_eq!(result::void(Ok::<i32, &str>(5)), Ok(()));
    }

    #[test]
    fn result_void_err() {
      assert_eq!(result::void(Err::<i32, &str>("e")), Err("e"));
    }
  }

  mod vec_module_fns {
    use super::*;

    #[test]
    fn vec_map_transforms_each() {
      assert_eq!(vec::map(vec![1_i32, 2, 3], |x| x * 2), vec![2, 4, 6]);
    }

    #[test]
    fn vec_as_replaces_all() {
      assert_eq!(vec::as_(vec![1_i32, 2, 3], 0_i32), vec![0, 0, 0]);
    }

    #[test]
    fn vec_void_discards_values() {
      assert_eq!(vec::void(vec![1_i32, 2, 3]), vec![(), (), ()]);
    }
  }

  mod option_module_fns {
    use super::*;

    #[test]
    fn option_map_some() {
      assert_eq!(option::map(Some(3_i32), |x| x * 2), Some(6));
    }

    #[test]
    fn option_map_none() {
      assert_eq!(option::map(None::<i32>, |x| x * 2), None);
    }

    #[test]
    fn option_as_some() {
      assert_eq!(option::as_(Some(3_i32), "replaced"), Some("replaced"));
    }

    #[test]
    fn option_as_none() {
      assert_eq!(option::as_(None::<i32>, "replaced"), None);
    }

    #[test]
    fn option_void_some() {
      assert_eq!(option::void(Some(42_i32)), Some(()));
    }

    #[test]
    fn option_void_none() {
      assert_eq!(option::void(None::<i32>), None);
    }

    #[test]
    fn option_tap_some() {
      let result = option::tap(Some(5_i32), |x| x * 3);
      assert_eq!(result, Some((5, 15)));
    }

    #[test]
    fn option_tap_none() {
      let result: Option<(i32, i32)> = option::tap(None::<i32>, |x| x * 3);
      assert_eq!(result, None);
    }
  }
}
