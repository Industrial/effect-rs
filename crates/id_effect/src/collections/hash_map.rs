//! Immutable persistent hash maps (`im::HashMap`) plus mutable `std::collections::HashMap` newtypes.
//!
//! Free functions below follow an **immutable** style: they take `&EffectHashMap` or owned maps and
//! return updated maps. [`MutableHashMap`] uses **methods** with the same names for in-place updates.

use std::collections::HashMap;
use std::hash::Hash;

use rayon::prelude::*;

/// Persistent hash map — mirrors Effect.ts-style immutable maps; backed by [`im::HashMap`].
pub type EffectHashMap<K, V> = im::HashMap<K, V>;

/// Empty persistent map.
#[inline]
pub fn empty<K, V>() -> EffectHashMap<K, V>
where
  K: Hash + Eq + Clone,
  V: Clone,
{
  EffectHashMap::new()
}

/// Builds a map from key–value pairs (immutable collect).
#[inline]
pub fn from_iter<K, V, I>(iter: I) -> EffectHashMap<K, V>
where
  I: IntoIterator<Item = (K, V)>,
  K: Hash + Eq + Clone,
  V: Clone,
{
  iter.into_iter().collect()
}

/// Looks up `key` without mutating the map.
#[inline]
pub fn get<'a, K, V, Q>(map: &'a EffectHashMap<K, V>, key: &Q) -> Option<&'a V>
where
  Q: Hash + Eq + ?Sized,
  K: Hash + Eq + Clone + std::borrow::Borrow<Q>,
  V: Clone,
{
  map.get(key)
}

/// Returns whether `key` is present.
#[inline]
pub fn has<K, V, Q>(map: &EffectHashMap<K, V>, key: &Q) -> bool
where
  Q: Hash + Eq + ?Sized,
  K: Hash + Eq + Clone + std::borrow::Borrow<Q>,
  V: Clone,
{
  map.contains_key(key)
}

/// Inserts or updates `key` → `value`, returning a new map.
#[inline]
pub fn set<K, V>(map: &EffectHashMap<K, V>, key: K, value: V) -> EffectHashMap<K, V>
where
  K: Hash + Eq + Clone,
  V: Clone,
{
  map.update(key, value)
}

/// Returns a new map without `key` (no-op if absent).
#[inline]
pub fn remove<K, V, Q>(map: &EffectHashMap<K, V>, key: &Q) -> EffectHashMap<K, V>
where
  Q: Hash + Eq + ?Sized,
  K: Hash + Eq + Clone + std::borrow::Borrow<Q>,
  V: Clone,
{
  map.without(key)
}

/// [`EffectHashMap::alter`] — `None` from `f` removes the key.
#[inline]
pub fn modify<K, V, F>(map: &EffectHashMap<K, V>, key: K, f: F) -> EffectHashMap<K, V>
where
  K: Hash + Eq + Clone,
  V: Clone,
  F: FnOnce(Option<V>) -> Option<V>,
{
  map.alter(f, key)
}

/// Updates an existing entry with `f(&value)`; if the key is missing, returns a clone of `map`.
#[inline]
pub fn modify_at<K, V, F>(map: &EffectHashMap<K, V>, key: K, f: F) -> EffectHashMap<K, V>
where
  K: Hash + Eq + Clone,
  V: Clone,
  F: FnOnce(&V) -> V,
{
  match map.get(&key) {
    Some(v) => map.update(key, f(v)),
    None => map.clone(),
  }
}

/// Maps every value in place (consumes the map, produces a new one).
#[inline]
pub fn map_values<K, V, W, F>(map: EffectHashMap<K, V>, mut f: F) -> EffectHashMap<K, W>
where
  K: Hash + Eq + Clone,
  V: Clone,
  W: Clone,
  F: FnMut(V) -> W,
{
  map.into_iter().map(|(k, v)| (k, f(v))).collect()
}

/// Like [`map_values`], but maps values in parallel (Rayon). `K`, `V`, and `W` must be
/// `Send` — required for the parallel work pool.
pub fn map_values_par<K, V, W, F>(map: EffectHashMap<K, V>, f: F) -> EffectHashMap<K, W>
where
  K: Hash + Eq + Clone + Send + Sync,
  V: Clone + Send,
  W: Clone + Send,
  F: Fn(V) -> W + Send + Sync,
{
  let pairs: Vec<(K, V)> = map.into_iter().collect();
  let out: Vec<(K, W)> = pairs.into_par_iter().map(|(k, v)| (k, f(v))).collect();
  from_iter(out)
}

/// Keeps entries where `pred(key, value)` is true.
#[inline]
pub fn filter<K, V, F>(map: &EffectHashMap<K, V>, mut pred: F) -> EffectHashMap<K, V>
where
  K: Hash + Eq + Clone,
  V: Clone,
  F: FnMut(&K, &V) -> bool,
{
  map
    .iter()
    .filter(|(k, v)| pred(k, v))
    .map(|(k, v)| (k.clone(), v.clone()))
    .collect()
}

/// Like [`filter`], but tests entries in parallel (Rayon).
pub fn filter_par<K, V, F>(map: &EffectHashMap<K, V>, pred: F) -> EffectHashMap<K, V>
where
  K: Hash + Eq + Clone + Send + Sync,
  V: Clone + Send,
  F: Fn(&K, &V) -> bool + Send + Sync,
{
  let pairs: Vec<(K, V)> = map.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
  let kept: Vec<(K, V)> = pairs.into_par_iter().filter(|(k, v)| pred(k, v)).collect();
  from_iter(kept)
}

/// Folds over `(key, value)` pairs (iterator order).
#[inline]
pub fn reduce<K, V, Acc, F>(map: &EffectHashMap<K, V>, init: Acc, f: F) -> Acc
where
  K: Hash + Eq + Clone,
  V: Clone,
  F: FnMut(Acc, (&K, &V)) -> Acc,
{
  map.iter().fold(init, f)
}

/// Union: keys in both maps keep the value from `left` (see [`im::HashMap::union`]).
#[inline]
pub fn union<K, V>(left: EffectHashMap<K, V>, right: EffectHashMap<K, V>) -> EffectHashMap<K, V>
where
  K: Hash + Eq + Clone,
  V: Clone,
{
  left.union(right)
}

/// Clones all keys into a vector.
#[inline]
pub fn keys<K, V>(map: &EffectHashMap<K, V>) -> Vec<K>
where
  K: Hash + Eq + Clone,
  V: Clone,
{
  map.keys().cloned().collect()
}

/// Clones all values into a vector.
#[inline]
pub fn values<K, V>(map: &EffectHashMap<K, V>) -> Vec<V>
where
  K: Hash + Eq + Clone,
  V: Clone,
{
  map.values().cloned().collect()
}

/// Number of entries.
#[inline]
pub fn size<K, V>(map: &EffectHashMap<K, V>) -> usize
where
  K: Hash + Eq + Clone,
  V: Clone,
{
  map.len()
}

/// True when the map has no entries.
#[inline]
pub fn is_empty<K, V>(map: &EffectHashMap<K, V>) -> bool
where
  K: Hash + Eq + Clone,
  V: Clone,
{
  map.is_empty()
}

/// Remove `key` and return `(value, new_map)` if it was present.
#[inline]
pub fn pop<K, V, Q>(map: &EffectHashMap<K, V>, key: &Q) -> Option<(V, EffectHashMap<K, V>)>
where
  Q: Hash + Eq + ?Sized,
  K: Hash + Eq + Clone + std::borrow::Borrow<Q>,
  V: Clone,
{
  map.extract(key)
}

// ── MutableHashMap ───────────────────────────────────────────────────────────

/// In-place mutable map with the same logical operations as the immutable free functions.
#[derive(Debug, Clone, Default)]
pub struct MutableHashMap<K, V>(
  /// Backing standard library map.
  pub HashMap<K, V>,
);

impl<K: Hash + Eq, V> MutableHashMap<K, V> {
  /// Empty map.
  #[inline]
  pub fn new() -> Self {
    Self(HashMap::new())
  }

  /// Borrows the value for `key`, if present.
  #[inline]
  pub fn get<Q: Hash + Eq + ?Sized>(&self, key: &Q) -> Option<&V>
  where
    K: std::borrow::Borrow<Q>,
  {
    self.0.get(key)
  }

  /// Whether `key` exists.
  #[inline]
  pub fn has<Q: Hash + Eq + ?Sized>(&self, key: &Q) -> bool
  where
    K: std::borrow::Borrow<Q>,
  {
    self.0.contains_key(key)
  }

  /// Inserts or overwrites `key`.
  #[inline]
  pub fn set(&mut self, key: K, value: V) {
    self.0.insert(key, value);
  }

  /// Removes `key` and returns the previous value, if any.
  #[inline]
  pub fn remove<Q: Hash + Eq + ?Sized>(&mut self, key: &Q) -> Option<V>
  where
    K: std::borrow::Borrow<Q>,
  {
    self.0.remove(key)
  }

  /// Alters or removes `key`: `None` from `f` deletes the entry.
  #[inline]
  pub fn modify<F>(&mut self, key: K, f: F)
  where
    F: FnOnce(Option<V>) -> Option<V>,
  {
    let old = self.0.remove(&key);
    if let Some(v) = f(old) {
      self.0.insert(key, v);
    }
  }

  /// Updates an existing value with `f`; no-op if `key` is missing.
  #[inline]
  pub fn modify_at<F>(&mut self, key: K, f: F)
  where
    F: FnOnce(&V) -> V,
  {
    if let Some(v) = self.0.get(&key) {
      let nv = f(v);
      self.0.insert(key, nv);
    }
  }

  /// Cloned keys.
  #[inline]
  pub fn keys(&self) -> Vec<K>
  where
    K: Clone,
  {
    self.0.keys().cloned().collect()
  }

  /// Cloned values.
  #[inline]
  pub fn values(&self) -> Vec<V>
  where
    V: Clone,
  {
    self.0.values().cloned().collect()
  }

  /// Entry count.
  #[inline]
  pub fn size(&self) -> usize {
    self.0.len()
  }

  /// True when there are no entries.
  #[inline]
  pub fn is_empty(&self) -> bool {
    self.0.is_empty()
  }

  /// Removes `key` and returns its value (alias of [`Self::remove`] with borrowed key type `K`).
  #[inline]
  pub fn pop(&mut self, key: &K) -> Option<V> {
    self.0.remove(key)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use rstest::rstest;

  #[test]
  fn hash_map_set_then_get_returns_value() {
    let m = empty::<&str, i32>();
    let m = set(&m, "a", 1);
    assert_eq!(get(&m, "a"), Some(&1));
  }

  #[test]
  fn hash_map_remove_absent_key_is_noop() {
    let m = empty::<i32, i32>();
    let m2 = remove(&m, &1i32);
    assert_eq!(m, m2);
    assert_eq!(size(&m2), 0);
  }

  #[test]
  fn hash_map_union_prefers_left_on_conflict() {
    let a = from_iter([(1, 10), (2, 20)]);
    let b = from_iter([(2, 99), (3, 30)]);
    let u = union(a, b);
    assert_eq!(get(&u, &1), Some(&10));
    assert_eq!(get(&u, &2), Some(&20));
    assert_eq!(get(&u, &3), Some(&30));
  }

  #[rstest]
  #[case::empty(empty::<i32,i32>(), 0)]
  #[case::single(from_iter([(1, 1)]), 1)]
  #[case::multi(from_iter([(1, 1), (2, 2), (3, 3)]), 3)]
  fn hash_map_size_tracks_entries(#[case] m: EffectHashMap<i32, i32>, #[case] expected: usize) {
    assert_eq!(size(&m), expected);
  }

  #[test]
  fn mutable_hash_map_mutates_in_place() {
    let mut m = MutableHashMap::new();
    m.set("x", 1i32);
    assert_eq!(m.get("x"), Some(&1));
    m.set("x", 2);
    assert_eq!(m.get("x"), Some(&2));
    assert_eq!(m.pop(&"x"), Some(2));
    assert!(m.is_empty());
  }

  #[test]
  fn modify_removes_when_closure_returns_none() {
    let m = set(&empty(), "k", 1);
    let m2 = modify(&m, "k", |_| None::<i32>);
    assert!(!has(&m2, "k"));
  }

  #[test]
  fn pop_returns_value_and_new_map() {
    let m = set(&empty(), 1u8, "a");
    let (v, rest) = pop(&m, &1u8).expect("pop");
    assert_eq!(v, "a");
    assert!(is_empty(&rest));
  }

  #[test]
  fn modify_at_updates_existing_key() {
    let m = set(&empty(), "a", 1i32);
    let m2 = modify_at(&m, "a", |v| v + 10);
    assert_eq!(get(&m2, "a"), Some(&11));
  }

  #[test]
  fn modify_at_noop_when_key_missing() {
    let m = set(&empty(), "a", 1i32);
    let m2 = modify_at(&m, "b", |v| v + 10);
    assert_eq!(get(&m2, "b"), None);
    assert_eq!(get(&m2, "a"), Some(&1));
  }

  #[test]
  fn map_values_transforms_all() {
    let m = from_iter([("a", 1i32), ("b", 2)]);
    let m2 = map_values(m, |v| v * 2);
    assert_eq!(get(&m2, "a"), Some(&2));
    assert_eq!(get(&m2, "b"), Some(&4));
  }

  #[test]
  fn map_values_par_matches_map_values() {
    let m = from_iter([("a", 1i32), ("b", 2), ("c", 3)]);
    let a = map_values(m.clone(), |v| v * 2);
    let b = map_values_par(m, |v| v * 2);
    assert_eq!(a, b);
  }

  #[test]
  fn filter_keeps_matching_entries() {
    let m = from_iter([(1i32, 10), (2, 20), (3, 30)]);
    let m2 = filter(&m, |k, _v| *k > 1);
    assert!(!has(&m2, &1));
    assert!(has(&m2, &2));
    assert!(has(&m2, &3));
  }

  #[test]
  fn filter_par_matches_filter() {
    let m = from_iter([(1i32, 10), (2, 20), (3, 30), (4, 40)]);
    let a = filter(&m, |k, _v| *k > 1);
    let b = filter_par(&m, |k, _v| *k > 1);
    assert_eq!(a, b);
  }

  #[test]
  fn reduce_sums_values() {
    let m = from_iter([("a", 1i32), ("b", 2), ("c", 3)]);
    let total = reduce(&m, 0, |acc, (_, v)| acc + v);
    assert_eq!(total, 6);
  }

  #[test]
  fn keys_and_values_return_all_entries() {
    let m = from_iter([(1i32, "a"), (2, "b")]);
    let mut ks = keys(&m);
    ks.sort();
    assert_eq!(ks, vec![1, 2]);
    assert_eq!(values(&m).len(), 2);
  }

  #[test]
  fn is_empty_and_size_on_empty_map() {
    let m = empty::<i32, i32>();
    assert!(is_empty(&m));
    assert_eq!(size(&m), 0);
  }

  #[test]
  fn mutable_hash_map_modify_at_updates_existing() {
    let mut m = MutableHashMap::new();
    m.set("x", 5i32);
    m.modify_at("x", |v| v + 1);
    assert_eq!(m.get("x"), Some(&6));
  }

  #[test]
  fn mutable_hash_map_modify_at_noop_when_missing() {
    let mut m = MutableHashMap::<&str, i32>::new();
    m.modify_at("missing", |v| v + 1);
    assert_eq!(m.get("missing"), None);
  }

  #[test]
  fn mutable_hash_map_has_and_remove() {
    let mut m = MutableHashMap::new();
    m.set(1i32, "a");
    assert!(m.has(&1));
    assert!(!m.has(&2));
    let old = m.remove(&1);
    assert_eq!(old, Some("a"));
    assert!(!m.has(&1));
  }

  #[test]
  fn mutable_hash_map_keys_and_values() {
    let mut m = MutableHashMap::new();
    m.set(1i32, "a");
    m.set(2, "b");
    let mut ks = m.keys();
    ks.sort();
    assert_eq!(ks, vec![1, 2]);
    assert_eq!(m.values().len(), 2);
    assert_eq!(m.size(), 2);
    assert!(!m.is_empty());
  }

  #[test]
  fn mutable_hash_map_modify_deletes_when_none() {
    let mut m = MutableHashMap::new();
    m.set("k", 10i32);
    m.modify("k", |_| None);
    assert!(!m.has("k"));
  }

  #[test]
  fn mutable_hash_map_modify_inserts_when_missing() {
    let mut m = MutableHashMap::<&str, i32>::new();
    m.modify("k", |_| Some(99));
    assert_eq!(m.get("k"), Some(&99));
  }
}
