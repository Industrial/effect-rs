//! Synchronous interior-mutable cell — mirrors Effect.ts `MutableRef` style API.

use std::sync::Mutex;

/// A mutex-backed mutable slot with value-style combinators.
pub struct MutableRef<A> {
  inner: Mutex<A>,
}

impl<A> MutableRef<A> {
  /// New cell holding `initial`.
  #[inline]
  pub fn make(initial: A) -> Self {
    Self {
      inner: Mutex::new(initial),
    }
  }

  /// Clone the current value.
  #[inline]
  pub fn get(&self) -> A
  where
    A: Clone,
  {
    self
      .inner
      .lock()
      .expect("mutable_ref mutex poisoned")
      .clone()
  }

  /// Replace the stored value with `value`.
  #[inline]
  pub fn set(&self, value: A) {
    *self.inner.lock().expect("mutable_ref mutex poisoned") = value;
  }

  /// Mutate the value in place with `f`.
  #[inline]
  pub fn update(&self, f: impl FnOnce(&mut A)) {
    let mut g = self.inner.lock().expect("mutable_ref mutex poisoned");
    f(&mut *g);
  }

  /// Runs `f` then returns a clone of the new value.
  #[inline]
  pub fn update_and_get(&self, f: impl FnOnce(&mut A)) -> A
  where
    A: Clone,
  {
    let mut g = self.inner.lock().expect("mutable_ref mutex poisoned");
    f(&mut *g);
    g.clone()
  }

  /// Returns a clone of the value before `f` runs.
  #[inline]
  pub fn get_and_update(&self, f: impl FnOnce(&mut A)) -> A
  where
    A: Clone,
  {
    let mut g = self.inner.lock().expect("mutable_ref mutex poisoned");
    let prev = g.clone();
    f(&mut *g);
    prev
  }

  /// Sets `value` and returns the previous value (cloned).
  #[inline]
  pub fn get_and_set(&self, value: A) -> A
  where
    A: Clone,
  {
    let mut g = self.inner.lock().expect("mutable_ref mutex poisoned");
    let prev = g.clone();
    *g = value;
    prev
  }

  /// Runs `f` under the lock and returns its result.
  #[inline]
  pub fn modify<B>(&self, f: impl FnOnce(&mut A) -> B) -> B {
    let mut g = self.inner.lock().expect("mutable_ref mutex poisoned");
    f(&mut *g)
  }

  /// If the current value equals `current`, replace with `new` and return `true`.
  #[inline]
  pub fn compare_and_set(&self, current: &A, new: A) -> bool
  where
    A: PartialEq,
  {
    let mut g = self.inner.lock().expect("mutable_ref mutex poisoned");
    if *g == *current {
      *g = new;
      true
    } else {
      false
    }
  }
}

impl MutableRef<bool> {
  /// Flip the boolean; returns the new value.
  #[inline]
  pub fn toggle(&self) -> bool {
    let mut g = self.inner.lock().expect("mutable_ref mutex poisoned");
    *g = !*g;
    *g
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn mutable_ref_cas_returns_false_on_mismatch() {
    let r = MutableRef::make(1_i32);
    assert!(!r.compare_and_set(&2, 9));
    assert_eq!(r.get(), 1);
    assert!(r.compare_and_set(&1, 5));
    assert_eq!(r.get(), 5);
  }

  #[test]
  fn mutable_ref_toggle_flips_bool() {
    let r = MutableRef::make(false);
    assert!(r.toggle());
    assert!(!r.toggle());
  }

  #[test]
  fn mutable_ref_get_and_set_returns_previous() {
    let r = MutableRef::make("a".to_string());
    assert_eq!(r.get_and_set("b".into()), "a");
    assert_eq!(r.get(), "b");
  }

  #[test]
  fn mutable_ref_set_replaces_value() {
    let r = MutableRef::make(0_i32);
    r.set(42);
    assert_eq!(r.get(), 42);
  }

  #[test]
  fn mutable_ref_update_mutates_in_place() {
    let r = MutableRef::make(vec![1_i32]);
    r.update(|v| v.push(2));
    assert_eq!(r.get(), vec![1, 2]);
  }

  #[test]
  fn mutable_ref_update_and_get_returns_new() {
    let r = MutableRef::make(10_i32);
    let new_val = r.update_and_get(|v| *v += 5);
    assert_eq!(new_val, 15);
    assert_eq!(r.get(), 15);
  }

  #[test]
  fn mutable_ref_get_and_update_returns_old() {
    let r = MutableRef::make(10_i32);
    let old_val = r.get_and_update(|v| *v += 5);
    assert_eq!(old_val, 10);
    assert_eq!(r.get(), 15);
  }

  #[test]
  fn mutable_ref_modify_returns_custom_value() {
    let r = MutableRef::make(7_i32);
    let result = r.modify(|v| {
      let prev = *v;
      *v = 0;
      prev * 2
    });
    assert_eq!(result, 14);
    assert_eq!(r.get(), 0);
  }
}
