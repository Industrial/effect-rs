//! Composable comparators — mirrors Effect.ts `Order` and `Ordering`.
//!
//! `Ordering` is re-exported from `std::cmp::Ordering` (Less/Equal/Greater).
//! `DynOrder<A>` is a heap-allocated comparator function that can be composed
//! at runtime using `reverse`, `combine`, `map_input`, etc.

pub use std::cmp::Ordering;

// ── DynOrder<A> ───────────────────────────────────────────────────────────────

/// A runtime-composable comparator — mirrors Effect.ts `Order<A>`.
///
/// Use `fn(&A, &A) -> Ordering` for static dispatch; use `DynOrder<A>` when
/// you need to pass comparators as values or compose them at runtime.
pub type DynOrder<A> = Box<dyn Fn(&A, &A) -> Ordering + Send + Sync>;

// ── ordering module ───────────────────────────────────────────────────────────

/// Free functions on `Ordering` values — mirrors Effect.ts `Ordering` namespace.
pub mod ordering {
  use super::Ordering;

  /// Reverse an ordering result: `Less` ↔ `Greater`, `Equal` stays.
  pub fn reverse(o: Ordering) -> Ordering {
    o.reverse()
  }

  /// Combine two orderings: return `first` unless it is `Equal`, then `second`.
  ///
  /// Mirrors Effect.ts `Ordering.combine`.
  pub fn combine(first: Ordering, second: Ordering) -> Ordering {
    match first {
      Ordering::Equal => second,
      other => other,
    }
  }

  /// Pattern-match on an `Ordering` value, returning the result of the matching branch.
  pub fn match_<A>(
    o: Ordering,
    on_less: impl FnOnce() -> A,
    on_equal: impl FnOnce() -> A,
    on_greater: impl FnOnce() -> A,
  ) -> A {
    match o {
      Ordering::Less => on_less(),
      Ordering::Equal => on_equal(),
      Ordering::Greater => on_greater(),
    }
  }
}

// ── order module ─────────────────────────────────────────────────────────────

/// Constructors and combinators for `DynOrder<A>` — mirrors Effect.ts `Order` namespace.
#[allow(clippy::module_inception)]
pub mod order {
  use super::{DynOrder, Ordering};
  use rayon::prelude::*;
  use std::time::Duration;

  // ── Primitive constructors ────────────────────────────────────────────────

  /// Total order on [`Duration`] (same as [`Duration::cmp`]).
  pub fn duration() -> DynOrder<Duration> {
    Box::new(|a: &Duration, b: &Duration| a.cmp(b))
  }

  /// Lexicographic order on `String`.
  pub fn string() -> DynOrder<String> {
    Box::new(|a: &String, b: &String| a.cmp(b))
  }

  /// Numeric order on `f64` (NaN treated as equal to itself, greater than all finite values).
  pub fn number_f64() -> DynOrder<f64> {
    Box::new(|a: &f64, b: &f64| a.partial_cmp(b).unwrap_or(Ordering::Equal))
  }

  /// Numeric order on `i64`.
  pub fn number_i64() -> DynOrder<i64> {
    Box::new(|a: &i64, b: &i64| a.cmp(b))
  }

  /// Natural order on `u64`.
  pub fn number_u64() -> DynOrder<u64> {
    Box::new(|a: &u64, b: &u64| a.cmp(b))
  }

  /// Natural order on `usize`.
  pub fn number_usize() -> DynOrder<usize> {
    Box::new(|a: &usize, b: &usize| a.cmp(b))
  }

  // ── Combinators ──────────────────────────────────────────────────────────

  /// Reverse a comparator: smallest becomes largest and vice versa.
  pub fn reverse<A: 'static>(ord: DynOrder<A>) -> DynOrder<A> {
    Box::new(move |a, b| ord(b, a))
  }

  /// Combine two comparators: use `first` unless it returns `Equal`, then `second`.
  pub fn combine<A: 'static>(first: DynOrder<A>, second: DynOrder<A>) -> DynOrder<A> {
    Box::new(move |a, b| super::ordering::combine(first(a, b), second(a, b)))
  }

  /// Contramap: apply `f` to inputs before comparing.
  ///
  /// Mirrors Effect.ts `Order.mapInput`.
  pub fn map_input<A: 'static, B: 'static>(
    ord: DynOrder<A>,
    f: impl Fn(&B) -> A + Send + Sync + 'static,
  ) -> DynOrder<B> {
    Box::new(move |a, b| ord(&f(a), &f(b)))
  }

  // ── Derived predicates ────────────────────────────────────────────────────

  /// Returns a predicate: `true` when `self < that`.
  pub fn less_than<A>(ord: &DynOrder<A>, a: &A, b: &A) -> bool {
    ord(a, b) == Ordering::Less
  }

  /// Returns a predicate: `true` when `self <= that`.
  pub fn less_than_or_equal_to<A>(ord: &DynOrder<A>, a: &A, b: &A) -> bool {
    ord(a, b) != Ordering::Greater
  }

  /// Returns a predicate: `true` when `self > that`.
  pub fn greater_than<A>(ord: &DynOrder<A>, a: &A, b: &A) -> bool {
    ord(a, b) == Ordering::Greater
  }

  /// Returns a predicate: `true` when `self >= that`.
  pub fn greater_than_or_equal_to<A>(ord: &DynOrder<A>, a: &A, b: &A) -> bool {
    ord(a, b) != Ordering::Less
  }

  // ── Derived value functions ───────────────────────────────────────────────

  /// Return the smaller of `a` and `b` according to `ord`.
  pub fn min<A: Clone>(ord: &DynOrder<A>, a: A, b: A) -> A {
    if ord(&a, &b) != Ordering::Greater {
      a
    } else {
      b
    }
  }

  /// Return the larger of `a` and `b` according to `ord`.
  pub fn max<A: Clone>(ord: &DynOrder<A>, a: A, b: A) -> A {
    if ord(&a, &b) != Ordering::Less { a } else { b }
  }

  /// Clamp `value` between `minimum` and `maximum`.
  pub fn clamp<A: Clone>(ord: &DynOrder<A>, value: A, minimum: A, maximum: A) -> A {
    if ord(&value, &minimum) == Ordering::Less {
      minimum
    } else if ord(&value, &maximum) == Ordering::Greater {
      maximum
    } else {
      value
    }
  }

  /// Return `true` if `minimum <= value <= maximum`.
  pub fn between<A>(ord: &DynOrder<A>, value: &A, minimum: &A, maximum: &A) -> bool {
    ord(value, minimum) != Ordering::Less && ord(value, maximum) != Ordering::Greater
  }

  /// Sort a `Vec<A>` using `ord` and return the sorted vec.
  pub fn sort_with<A: Clone>(ord: &DynOrder<A>, mut arr: Vec<A>) -> Vec<A> {
    arr.sort_by(|a, b| ord(a, b));
    arr
  }

  /// Like [`sort_with`], but uses a parallel sort (Rayon) — best for large inputs when `ord` is
  /// hot enough to amortize thread overhead.
  pub fn sort_with_par<A: Clone + Send + Sync>(ord: &DynOrder<A>, mut arr: Vec<A>) -> Vec<A> {
    arr.par_sort_by(|a, b| ord(a, b));
    arr
  }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
  use super::*;
  use rstest::rstest;
  use std::time::Duration;

  // ── order::duration ───────────────────────────────────────────────────────

  mod order_duration {
    use super::*;

    #[test]
    fn max_matches_std_duration_ordering() {
      let ord = order::duration();
      let a = Duration::from_millis(10);
      let b = Duration::from_millis(20);
      assert_eq!(order::max(&ord, a, b), b);
    }
  }

  // ── ordering::reverse ────────────────────────────────────────────────────

  mod ordering_reverse {
    use super::*;

    #[rstest]
    #[case::less(Ordering::Less, Ordering::Greater)]
    #[case::equal(Ordering::Equal, Ordering::Equal)]
    #[case::greater(Ordering::Greater, Ordering::Less)]
    fn reverses_all_variants(#[case] input: Ordering, #[case] expected: Ordering) {
      assert_eq!(ordering::reverse(input), expected);
    }
  }

  // ── ordering::combine ─────────────────────────────────────────────────────

  mod ordering_combine {
    use super::*;

    #[test]
    fn equal_defers_to_second() {
      assert_eq!(
        ordering::combine(Ordering::Equal, Ordering::Less),
        Ordering::Less
      );
      assert_eq!(
        ordering::combine(Ordering::Equal, Ordering::Greater),
        Ordering::Greater
      );
    }

    #[test]
    fn less_ignores_second() {
      assert_eq!(
        ordering::combine(Ordering::Less, Ordering::Greater),
        Ordering::Less
      );
    }

    #[test]
    fn greater_ignores_second() {
      assert_eq!(
        ordering::combine(Ordering::Greater, Ordering::Less),
        Ordering::Greater
      );
    }
  }

  // ── ordering::match_ ─────────────────────────────────────────────────────

  mod ordering_match {
    use super::*;

    #[rstest]
    #[case::less(Ordering::Less, "less")]
    #[case::equal(Ordering::Equal, "equal")]
    #[case::greater(Ordering::Greater, "greater")]
    fn dispatches_to_correct_branch(#[case] o: Ordering, #[case] expected: &str) {
      let result = ordering::match_(o, || "less", || "equal", || "greater");
      assert_eq!(result, expected);
    }
  }

  // ── order::string ────────────────────────────────────────────────────────

  mod order_string {
    use super::*;

    #[test]
    fn lexicographic_order_less() {
      let ord = order::string();
      assert_eq!(ord(&"a".to_string(), &"b".to_string()), Ordering::Less);
    }

    #[test]
    fn lexicographic_order_equal() {
      let ord = order::string();
      assert_eq!(ord(&"x".to_string(), &"x".to_string()), Ordering::Equal);
    }

    #[test]
    fn lexicographic_order_greater() {
      let ord = order::string();
      assert_eq!(ord(&"b".to_string(), &"a".to_string()), Ordering::Greater);
    }
  }

  // ── order::number_f64 ────────────────────────────────────────────────────

  mod order_number_f64 {
    use super::*;

    #[test]
    fn smaller_float_is_less() {
      let ord = order::number_f64();
      assert_eq!(ord(&1.0_f64, &2.0_f64), Ordering::Less);
    }

    #[test]
    fn equal_floats_are_equal() {
      let ord = order::number_f64();
      assert_eq!(ord(&3.5_f64, &3.5_f64), Ordering::Equal);
    }

    #[test]
    fn larger_float_is_greater() {
      let ord = order::number_f64();
      assert_eq!(ord(&5.0_f64, &2.0_f64), Ordering::Greater);
    }

    #[test]
    fn nan_does_not_panic() {
      let ord = order::number_f64();
      let _ = ord(&f64::NAN, &1.0_f64);
    }
  }

  // ── order::reverse ────────────────────────────────────────────────────────

  mod order_reverse {
    use super::*;

    #[test]
    fn reversed_i64_order_has_less_become_greater() {
      let ord = order::reverse(order::number_i64());
      assert_eq!(ord(&1_i64, &2_i64), Ordering::Greater);
    }

    #[test]
    fn reversed_order_equal_stays_equal() {
      let ord = order::reverse(order::number_i64());
      assert_eq!(ord(&5_i64, &5_i64), Ordering::Equal);
    }
  }

  // ── order::combine ───────────────────────────────────────────────────────

  mod order_combine {
    use super::*;

    #[test]
    fn when_first_is_equal_uses_second() {
      // First by string length (all same length "ab","ba","cc"), then lexicographic
      let by_len = order::map_input(order::number_usize(), |s: &String| s.len());
      let by_lex = order::string();
      let combined = order::combine(by_len, by_lex);
      assert_eq!(
        combined(&"ab".to_string(), &"ba".to_string()),
        Ordering::Less
      );
    }

    #[test]
    fn when_first_is_not_equal_ignores_second() {
      let short = "a".to_string();
      let longer = "bb".to_string();
      let by_len = order::map_input(order::number_usize(), |s: &String| s.len());
      let by_lex_rev = order::reverse(order::string());
      let combined = order::combine(by_len, by_lex_rev);
      assert_eq!(combined(&short, &longer), Ordering::Less);
    }
  }

  // ── order::map_input ────────────────────────────────────────────────────

  mod order_map_input {
    use super::*;

    #[test]
    fn contramap_on_string_length() {
      let by_len: DynOrder<String> = order::map_input(order::number_usize(), |s: &String| s.len());
      assert_eq!(by_len(&"a".to_string(), &"bb".to_string()), Ordering::Less);
      assert_eq!(
        by_len(&"aa".to_string(), &"bb".to_string()),
        Ordering::Equal
      );
    }
  }

  // ── order::min / max ─────────────────────────────────────────────────────

  mod order_min_max {
    use super::*;

    #[test]
    fn min_returns_smaller() {
      let ord = order::number_i64();
      assert_eq!(order::min(&ord, 3_i64, 7_i64), 3);
    }

    #[test]
    fn min_when_equal_returns_first() {
      let ord = order::number_i64();
      assert_eq!(order::min(&ord, 5_i64, 5_i64), 5);
    }

    #[test]
    fn max_returns_larger() {
      let ord = order::number_i64();
      assert_eq!(order::max(&ord, 3_i64, 7_i64), 7);
    }
  }

  // ── order::clamp ─────────────────────────────────────────────────────────

  mod order_clamp {
    use super::*;

    #[test]
    fn value_below_minimum_clamped_to_minimum() {
      let ord = order::number_i64();
      assert_eq!(order::clamp(&ord, -5_i64, 0_i64, 100_i64), 0);
    }

    #[test]
    fn value_above_maximum_clamped_to_maximum() {
      let ord = order::number_i64();
      assert_eq!(order::clamp(&ord, 200_i64, 0_i64, 100_i64), 100);
    }

    #[test]
    fn value_within_range_unchanged() {
      let ord = order::number_i64();
      assert_eq!(order::clamp(&ord, 50_i64, 0_i64, 100_i64), 50);
    }

    #[rstest]
    #[case::at_min(0_i64, 0)]
    #[case::at_max(100_i64, 100)]
    #[case::in_middle(50_i64, 50)]
    fn clamp_at_boundaries(#[case] value: i64, #[case] expected: i64) {
      let ord = order::number_i64();
      assert_eq!(order::clamp(&ord, value, 0_i64, 100_i64), expected);
    }
  }

  // ── order::between ───────────────────────────────────────────────────────

  mod order_between {
    use super::*;

    #[test]
    fn value_in_range_returns_true() {
      let ord = order::number_i64();
      assert!(order::between(&ord, &50_i64, &0_i64, &100_i64));
    }

    #[test]
    fn value_at_minimum_returns_true() {
      let ord = order::number_i64();
      assert!(order::between(&ord, &0_i64, &0_i64, &100_i64));
    }

    #[test]
    fn value_at_maximum_returns_true() {
      let ord = order::number_i64();
      assert!(order::between(&ord, &100_i64, &0_i64, &100_i64));
    }

    #[test]
    fn value_below_range_returns_false() {
      let ord = order::number_i64();
      assert!(!order::between(&ord, &-1_i64, &0_i64, &100_i64));
    }

    #[test]
    fn value_above_range_returns_false() {
      let ord = order::number_i64();
      assert!(!order::between(&ord, &101_i64, &0_i64, &100_i64));
    }
  }

  // ── order::sort_with ─────────────────────────────────────────────────────

  mod order_sort_with {
    use super::*;

    #[test]
    fn sort_integers_ascending() {
      let ord = order::number_i64();
      let result = order::sort_with(&ord, vec![3_i64, 1, 4, 1, 5, 9, 2]);
      assert_eq!(result, vec![1, 1, 2, 3, 4, 5, 9]);
    }

    #[test]
    fn sort_integers_descending_via_reverse() {
      let ord = order::reverse(order::number_i64());
      let result = order::sort_with(&ord, vec![3_i64, 1, 4]);
      assert_eq!(result, vec![4, 3, 1]);
    }

    #[test]
    fn sort_empty_vec_returns_empty() {
      let ord = order::number_i64();
      assert_eq!(order::sort_with(&ord, vec![]), Vec::<i64>::new());
    }

    #[test]
    fn sort_single_element_returns_same() {
      let ord = order::number_i64();
      assert_eq!(order::sort_with(&ord, vec![42_i64]), vec![42]);
    }

    #[test]
    fn sort_with_par_matches_sort_with() {
      let ord = order::number_i64();
      let data = vec![3_i64, 1, 4, 1, 5, 9, 2];
      assert_eq!(
        order::sort_with_par(&ord, data.clone()),
        order::sort_with(&ord, data)
      );
    }
  }

  // ── order::less_than / greater_than ──────────────────────────────────────

  mod order_predicates {
    use super::*;

    #[test]
    fn less_than_true_when_a_less() {
      let ord = order::number_i64();
      assert!(order::less_than(&ord, &1_i64, &2_i64));
    }

    #[test]
    fn less_than_false_when_equal() {
      let ord = order::number_i64();
      assert!(!order::less_than(&ord, &2_i64, &2_i64));
    }

    #[test]
    fn less_than_false_when_a_greater() {
      let ord = order::number_i64();
      assert!(!order::less_than(&ord, &3_i64, &2_i64));
    }

    #[test]
    fn greater_than_true_when_a_greater() {
      let ord = order::number_i64();
      assert!(order::greater_than(&ord, &3_i64, &2_i64));
    }

    #[test]
    fn less_than_or_equal_true_when_equal() {
      let ord = order::number_i64();
      assert!(order::less_than_or_equal_to(&ord, &5_i64, &5_i64));
    }
  }
}
