//! Structural data markers — Effect.ts `Data`–style equality and hashing.
//!
//! [`EffectData`] is implemented automatically for any type that already implements
//! [`PartialEq`], [`Eq`], and [`Hash`]. Use the [`EffectData`](effect_rs_proc_macro::EffectData)
//! derive macro to generate those impls field-wise for your own structs and enums.
//!
//! [`DataStruct`] and [`DataTuple`] are newtype wrappers for values you want to treat as
//! opaque “data” values in APIs. [`DataError`] combines [`std::error::Error`] with
//! [`EffectData`] for error types used in maps and sets.

use std::hash::{Hash, Hasher};

/// Marker for Effect-style **structural** equality and hashing (`PartialEq` + `Eq` + [`Hash`]).
///
/// Blanket-implemented for all types that already satisfy those bounds. Prefer
/// `#[derive(effect_rs::EffectData)]` on ADTs so the compiler generates consistent
/// field-wise `PartialEq` / `Eq` / `Hash` impls (see the `effect-proc-macro` crate).
pub trait EffectData: PartialEq + Eq + Hash {}

impl<T: PartialEq + Eq + Hash + ?Sized> EffectData for T {}

/// Newtype wrapping a struct (or any value type) that is compared and hashed by its inner value.
#[derive(Clone, Debug)]
pub struct DataStruct<T: PartialEq + Eq + Hash>(pub T);

impl<T: PartialEq + Eq + Hash> PartialEq for DataStruct<T> {
  fn eq(&self, other: &Self) -> bool {
    self.0 == other.0
  }
}

impl<T: PartialEq + Eq + Hash> Eq for DataStruct<T> {}

impl<T: PartialEq + Eq + Hash> Hash for DataStruct<T> {
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.0.hash(state);
  }
}

/// Newtype wrapping a tuple (or tuple-like product) for structural equality and hashing.
#[derive(Clone, Debug)]
pub struct DataTuple<T: PartialEq + Eq + Hash>(pub T);

impl<T: PartialEq + Eq + Hash> PartialEq for DataTuple<T> {
  fn eq(&self, other: &Self) -> bool {
    self.0 == other.0
  }
}

impl<T: PartialEq + Eq + Hash> Eq for DataTuple<T> {}

impl<T: PartialEq + Eq + Hash> Hash for DataTuple<T> {
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.0.hash(state);
  }
}

/// Errors that are also [`EffectData`] (structurally comparable / hashable).
pub trait DataError: std::error::Error + EffectData {}

impl<T: std::error::Error + EffectData> DataError for T {}

#[cfg(test)]
mod tests {
  use super::*;
  use rstest::rstest;
  use std::collections::HashMap;

  #[derive(Clone, Debug, effect_rs_proc_macro::EffectData)]
  struct Person {
    name: String,
    age: u32,
  }

  #[derive(Clone, Debug, crate::EffectData)]
  struct Point(i32, i32);

  #[derive(Clone, Debug, crate::EffectData)]
  enum Shape {
    Dot,
    Rect(u32, u32),
    Line(i32, i32),
  }

  #[crate::effect_tagged("TaggedRow")]
  #[derive(Clone, Debug, crate::EffectData)]
  struct TaggedRow {
    id: u32,
  }

  #[test]
  fn data_struct_eq_by_value() {
    let a = DataStruct(Person {
      name: "ada".into(),
      age: 36,
    });
    let b = DataStruct(Person {
      name: "ada".into(),
      age: 36,
    });
    assert_eq!(a, b);
  }

  #[test]
  fn data_struct_hash_same_for_equal_values() {
    let a = DataStruct(10_u32);
    let b = DataStruct(10_u32);
    let mut h1 = std::collections::hash_map::DefaultHasher::new();
    let mut h2 = std::collections::hash_map::DefaultHasher::new();
    a.hash(&mut h1);
    b.hash(&mut h2);
    assert_eq!(h1.finish(), h2.finish());
  }

  #[test]
  fn effect_data_usable_as_hashmap_key() {
    let mut m: HashMap<Person, &'static str> = HashMap::new();
    let p = Person {
      name: "bob".into(),
      age: 40,
    };
    m.insert(p.clone(), "ok");
    assert_eq!(m.get(&p), Some(&"ok"));
  }

  #[rstest]
  #[case((1_u32, 2_u32))]
  #[case((3_u32,))]
  #[case(())]
  fn data_tuple_matches_inner_tuple_equality<T>(#[case] inner: T)
  where
    T: Clone + PartialEq + Eq + Hash + std::fmt::Debug,
  {
    let a = DataTuple(inner.clone());
    let b = DataTuple(inner);
    assert_eq!(a, b);
  }

  #[test]
  fn effect_data_derive_enum_distinguishes_variants() {
    assert_ne!(Shape::Dot, Shape::Rect(0, 0));
    assert_eq!(Shape::Rect(1, 2), Shape::Rect(1, 2));
    assert_eq!(Shape::Line(1, 2), Shape::Line(1, 2));
    assert_eq!(Point(0, 0), Point(0, 0));
  }

  #[test]
  fn effect_tagged_row_exposes_tag_and_has_tag() {
    let row = TaggedRow {
      _tag: TaggedRow::EFFECT_TAGGED_TAG,
      id: 7,
    };
    assert_eq!(row._tag, "TaggedRow");
    assert_eq!(crate::HasTag::tag(&row), "TaggedRow");
  }

  #[test]
  fn data_tuple_hash_is_consistent() {
    let a = DataTuple((1_u32, 2_u32));
    let b = DataTuple((1_u32, 2_u32));
    let mut h1 = std::collections::hash_map::DefaultHasher::new();
    let mut h2 = std::collections::hash_map::DefaultHasher::new();
    a.hash(&mut h1);
    b.hash(&mut h2);
    assert_eq!(h1.finish(), h2.finish());
  }
}
