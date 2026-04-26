//! Immutable persistent hash sets (`im::HashSet`) plus mutable `std::collections::HashSet` newtypes.

use std::collections::HashSet;
use std::hash::Hash;

use rayon::prelude::*;

/// Persistent hash set — backed by [`im::HashSet`].
pub type EffectHashSet<A> = im::HashSet<A>;

/// Empty persistent set.
#[inline]
pub fn empty<A>() -> EffectHashSet<A>
where
  A: Hash + Eq + Clone,
{
  EffectHashSet::new()
}

/// Builds a set from an iterator of elements.
#[inline]
pub fn from_iter<A, I>(iter: I) -> EffectHashSet<A>
where
  I: IntoIterator<Item = A>,
  A: Hash + Eq + Clone,
{
  iter.into_iter().collect()
}

/// Membership test for `value`.
#[inline]
pub fn has<A, Q>(set: &EffectHashSet<A>, value: &Q) -> bool
where
  Q: Hash + Eq + ?Sized,
  A: Hash + Eq + Clone + std::borrow::Borrow<Q>,
{
  set.contains(value)
}

/// Returns a new set including `value`.
#[inline]
pub fn insert<A>(set: &EffectHashSet<A>, value: A) -> EffectHashSet<A>
where
  A: Hash + Eq + Clone,
{
  set.update(value)
}

/// Returns a new set without `value`.
#[inline]
pub fn remove<A, Q>(set: &EffectHashSet<A>, value: &Q) -> EffectHashSet<A>
where
  Q: Hash + Eq + ?Sized,
  A: Hash + Eq + Clone + std::borrow::Borrow<Q>,
{
  set.without(value)
}

/// Insert if absent, remove if present — returns the new set and whether the value is now in the set.
#[inline]
pub fn toggle<A>(set: &EffectHashSet<A>, value: A) -> (EffectHashSet<A>, bool)
where
  A: Hash + Eq + Clone,
{
  if set.contains(&value) {
    (set.without(&value), false)
  } else {
    (set.update(value), true)
  }
}

/// Set union of `left` and `right`.
#[inline]
pub fn union<A>(left: EffectHashSet<A>, right: EffectHashSet<A>) -> EffectHashSet<A>
where
  A: Hash + Eq + Clone,
{
  left.union(right)
}

/// Number of elements.
#[inline]
pub fn size<A>(set: &EffectHashSet<A>) -> usize
where
  A: Hash + Eq + Clone,
{
  set.len()
}

/// True when the set has no elements.
#[inline]
pub fn is_empty<A>(set: &EffectHashSet<A>) -> bool
where
  A: Hash + Eq + Clone,
{
  set.is_empty()
}

/// All elements as a cloned vector (order unspecified).
#[inline]
pub fn values<A>(set: &EffectHashSet<A>) -> Vec<A>
where
  A: Hash + Eq + Clone,
{
  set.iter().cloned().collect()
}

/// Like [`values`], but clones elements in parallel (Rayon). Output order is unspecified; sort if
/// a deterministic order is required.
pub fn values_par<A>(set: &EffectHashSet<A>) -> Vec<A>
where
  A: Hash + Eq + Clone + Send + Sync,
{
  set
    .iter()
    .cloned()
    .collect::<Vec<_>>()
    .into_par_iter()
    .collect()
}

// ── MutableHashSet ───────────────────────────────────────────────────────────

/// In-place mutable set mirroring the immutable helpers.
#[derive(Debug, Clone, Default)]
pub struct MutableHashSet<A>(
  /// Backing standard library set.
  pub HashSet<A>,
);

impl<A: Hash + Eq + Clone> MutableHashSet<A> {
  /// Empty set.
  #[inline]
  pub fn new() -> Self {
    Self(HashSet::new())
  }

  /// Whether `value` is in the set.
  #[inline]
  pub fn has<Q: Hash + Eq + ?Sized>(&self, value: &Q) -> bool
  where
    A: std::borrow::Borrow<Q>,
  {
    self.0.contains(value)
  }

  /// Adds `value` to the set.
  #[inline]
  pub fn insert(&mut self, value: A) {
    self.0.insert(value);
  }

  /// Removes `value`; returns whether it was present.
  #[inline]
  pub fn remove<Q: Hash + Eq + ?Sized>(&mut self, value: &Q) -> bool
  where
    A: std::borrow::Borrow<Q>,
  {
    self.0.remove(value)
  }

  /// Insert-if-absent / remove-if-present; returns whether `value` is now in the set.
  #[inline]
  pub fn toggle(&mut self, value: A) -> bool {
    if self.0.remove(&value) {
      false
    } else {
      self.0.insert(value);
      true
    }
  }

  /// Element count.
  #[inline]
  pub fn size(&self) -> usize {
    self.0.len()
  }

  /// True when empty.
  #[inline]
  pub fn is_empty(&self) -> bool {
    self.0.is_empty()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn hash_set_toggle_adds_then_removes() {
    let s = empty::<i32>();
    let (s, now_in) = toggle(&s, 5);
    assert!(now_in);
    assert!(has(&s, &5));
    let (s, now_in) = toggle(&s, 5);
    assert!(!now_in);
    assert!(!has(&s, &5));
  }

  #[test]
  fn mutable_set_toggle_matches_immutable_semantics() {
    let mut m = MutableHashSet::new();
    assert!(m.toggle(7));
    assert!(m.has(&7));
    assert!(!m.toggle(7));
    assert!(!m.has(&7));
  }

  #[test]
  fn from_iter_creates_deduplicated_set() {
    let s = from_iter([1i32, 2, 3, 2, 1]);
    assert_eq!(size(&s), 3);
  }

  #[test]
  fn insert_adds_element() {
    let s = empty::<i32>();
    let s = insert(&s, 1);
    assert!(has(&s, &1));
    assert_eq!(size(&s), 1);
  }

  #[test]
  fn remove_takes_element_out() {
    let s = from_iter([1i32, 2, 3]);
    let s2 = remove(&s, &2);
    assert!(!has(&s2, &2));
    assert_eq!(size(&s2), 2);
  }

  #[test]
  fn remove_absent_element_is_noop() {
    let s = from_iter([1i32, 2]);
    let s2 = remove(&s, &99);
    assert_eq!(size(&s2), 2);
  }

  #[test]
  fn union_combines_sets() {
    let a = from_iter([1i32, 2]);
    let b = from_iter([2i32, 3]);
    let u = union(a, b);
    assert_eq!(size(&u), 3);
    assert!(has(&u, &1));
    assert!(has(&u, &2));
    assert!(has(&u, &3));
  }

  #[test]
  fn size_and_is_empty() {
    let s = empty::<i32>();
    assert!(is_empty(&s));
    assert_eq!(size(&s), 0);
    let s = insert(&s, 42);
    assert_eq!(size(&s), 1);
    assert!(!is_empty(&s));
  }

  #[test]
  fn values_returns_all_elements() {
    let s = from_iter([1i32, 2, 3]);
    let mut v = values(&s);
    v.sort();
    assert_eq!(v, vec![1, 2, 3]);
  }

  #[test]
  fn mutable_set_new_insert_has() {
    let mut ms = MutableHashSet::new();
    assert!(!ms.has(&1i32));
    ms.insert(1);
    assert!(ms.has(&1));
  }

  #[test]
  fn mutable_set_remove_returns_whether_present() {
    let mut ms = MutableHashSet::new();
    ms.insert(5i32);
    assert!(ms.remove(&5));
    assert!(!ms.remove(&5));
  }

  #[test]
  fn mutable_set_size_and_is_empty() {
    let mut ms = MutableHashSet::<i32>::new();
    assert!(ms.is_empty());
    assert_eq!(ms.size(), 0);
    ms.insert(10);
    assert_eq!(ms.size(), 1);
    assert!(!ms.is_empty());
  }
}
