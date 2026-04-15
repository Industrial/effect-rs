//! Chunked data container used by stream internals.
//!
//! To build a [`Chunk`] incrementally via [`crate::MutableList`], use [`crate::ChunkBuilder`].

use std::cmp::Ordering;

use crate::schema::order::DynOrder;

/// Small wrapper over `Vec<A>` that expresses chunk-level stream semantics.
///
/// [`crate::schema::equal::EffectHash`] is implemented when `A: Hash`, so chunks of hashable
/// elements can be used as `HashMap` / `HashSet` keys (see §14).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Chunk<A> {
  items: Vec<A>,
}

impl<A> Chunk<A> {
  /// Construct an empty chunk.
  #[inline]
  pub fn empty() -> Self {
    Self { items: Vec::new() }
  }

  /// Construct a one-element chunk.
  #[inline]
  pub fn singleton(value: A) -> Self {
    Self { items: vec![value] }
  }

  /// Construct a chunk from an existing vector.
  #[inline]
  pub fn from_vec(values: Vec<A>) -> Self {
    Self { items: values }
  }

  /// Number of elements in the chunk.
  #[inline]
  pub fn len(&self) -> usize {
    self.items.len()
  }

  /// Whether this chunk has zero elements.
  #[inline]
  pub fn is_empty(&self) -> bool {
    self.items.is_empty()
  }

  /// Consume the chunk and return the underlying vector.
  #[inline]
  pub fn into_vec(self) -> Vec<A> {
    self.items
  }

  /// Iterate over chunk elements by reference.
  #[inline]
  pub fn iter(&self) -> impl Iterator<Item = &A> {
    self.items.iter()
  }

  /// Transform each element, preserving chunk ordering.
  #[inline]
  pub fn map<B, F>(self, f: F) -> Chunk<B>
  where
    F: FnMut(A) -> B,
  {
    Chunk {
      items: self.items.into_iter().map(f).collect(),
    }
  }

  /// Sort elements in place using a runtime [`DynOrder`].
  #[inline]
  pub fn sort_with(&mut self, order: &DynOrder<A>) {
    self.items.sort_by(|a, b| order(a, b));
  }

  /// Lexicographic comparison of two chunks using `order` on each pair of elements.
  ///
  /// Empty chunks compare [`Equal`](Ordering::Equal). If one chunk is a prefix of the other,
  /// the shorter chunk is [`Less`](Ordering::Less).
  pub fn compare_by(&self, other: &Self, order: &DynOrder<A>) -> Ordering {
    let mut ia = self.items.iter();
    let mut ib = other.items.iter();
    loop {
      match (ia.next(), ib.next()) {
        (None, None) => return Ordering::Equal,
        (None, Some(_)) => return Ordering::Less,
        (Some(_), None) => return Ordering::Greater,
        (Some(a), Some(b)) => match order(a, b) {
          Ordering::Equal => {}
          o => return o,
        },
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::Chunk;
  use rstest::rstest;

  mod empty {
    use super::*;

    #[test]
    fn empty_when_constructed_returns_zero_length_and_is_empty_true() {
      let chunk = Chunk::<u8>::empty();
      assert_eq!(chunk.len(), 0);
      assert!(chunk.is_empty());
      assert_eq!(chunk.into_vec(), Vec::<u8>::new());
    }
  }

  mod singleton {
    use super::*;

    #[test]
    fn singleton_with_value_returns_one_element_chunk() {
      let chunk = Chunk::singleton(7u8);
      assert_eq!(chunk.len(), 1);
      assert!(!chunk.is_empty());
      assert_eq!(chunk.into_vec(), vec![7]);
    }
  }

  mod from_vec {
    use super::*;

    #[test]
    fn from_vec_with_multiple_items_preserves_original_order() {
      let chunk = Chunk::from_vec(vec![1, 2, 3, 4]);
      let seen = chunk.iter().copied().collect::<Vec<_>>();
      assert_eq!(seen, vec![1, 2, 3, 4]);
      assert_eq!(chunk.into_vec(), vec![1, 2, 3, 4]);
    }
  }

  mod len_and_is_empty {
    use super::*;

    #[rstest]
    #[case::empty(vec![], 0, true)]
    #[case::single(vec![1], 1, false)]
    #[case::multiple(vec![1, 2, 3], 3, false)]
    fn len_and_is_empty_with_input_vector_report_expected_shape(
      #[case] values: Vec<i32>,
      #[case] expected_len: usize,
      #[case] expected_is_empty: bool,
    ) {
      let chunk = Chunk::from_vec(values);
      assert_eq!(chunk.len(), expected_len);
      assert_eq!(chunk.is_empty(), expected_is_empty);
    }
  }

  mod into_vec {
    use super::*;

    #[rstest]
    #[case::empty(vec![])]
    #[case::single(vec![42])]
    #[case::multiple(vec![3, 1, 4, 1, 5])]
    fn into_vec_with_any_chunk_returns_original_items(#[case] values: Vec<i32>) {
      let expected = values.clone();
      let chunk = Chunk::from_vec(values);
      assert_eq!(chunk.into_vec(), expected);
    }
  }

  mod iter {
    use super::*;

    #[test]
    fn iter_when_chunk_has_items_yields_references_in_order() {
      let chunk = Chunk::from_vec(vec![10, 20, 30]);
      let seen = chunk.iter().copied().collect::<Vec<_>>();
      assert_eq!(seen, vec![10, 20, 30]);
    }

    #[test]
    fn iter_when_chunk_is_empty_yields_no_items() {
      let chunk = Chunk::<i32>::empty();
      assert_eq!(chunk.iter().count(), 0);
    }
  }

  mod map {
    use super::*;

    #[test]
    fn map_with_non_empty_chunk_transforms_every_item_without_reordering() {
      let mapped = Chunk::from_vec(vec![1, 2, 3]).map(|n| n * 10);
      assert_eq!(mapped.into_vec(), vec![10, 20, 30]);
    }

    #[test]
    fn map_with_empty_chunk_returns_empty_chunk() {
      let mapped = Chunk::<i32>::empty().map(|n| n * 10);
      assert!(mapped.is_empty());
      assert_eq!(mapped.into_vec(), Vec::<i32>::new());
    }
  }

  mod sort_with {
    use super::Chunk;
    use crate::schema::order::order;

    #[test]
    fn sort_with_empty_chunk_returns_empty() {
      let mut chunk = Chunk::<i64>::empty();
      chunk.sort_with(&order::number_i64());
      assert!(chunk.is_empty());
    }

    #[test]
    fn sort_with_already_sorted_preserves_order() {
      let mut chunk = Chunk::from_vec(vec![1i64, 2, 3, 4]);
      chunk.sort_with(&order::number_i64());
      assert_eq!(chunk.into_vec(), vec![1, 2, 3, 4]);
    }

    #[test]
    fn sort_with_reversed_input_produces_ascending() {
      let mut chunk = Chunk::from_vec(vec![4i64, 1, 3, 2]);
      chunk.sort_with(&order::number_i64());
      assert_eq!(chunk.into_vec(), vec![1, 2, 3, 4]);
    }

    #[test]
    fn sort_with_reverse_order_produces_descending() {
      let mut chunk = Chunk::from_vec(vec![1i64, 2, 3, 4]);
      let desc = order::reverse(order::number_i64());
      chunk.sort_with(&desc);
      assert_eq!(chunk.into_vec(), vec![4, 3, 2, 1]);
    }
  }

  mod compare_by {
    use super::Chunk;
    use crate::schema::order::order;
    use std::cmp::Ordering;

    #[test]
    fn compare_by_empty_chunks_are_equal() {
      let a = Chunk::<i64>::empty();
      let b = Chunk::<i64>::empty();
      assert_eq!(a.compare_by(&b, &order::number_i64()), Ordering::Equal);
    }

    #[test]
    fn compare_by_prefix_is_less_than_longer_chunk() {
      let a = Chunk::from_vec(vec![1i64, 2]);
      let b = Chunk::from_vec(vec![1i64, 2, 3]);
      assert_eq!(a.compare_by(&b, &order::number_i64()), Ordering::Less);
      assert_eq!(b.compare_by(&a, &order::number_i64()), Ordering::Greater);
    }

    #[test]
    fn compare_by_differs_on_first_unequal_element() {
      let a = Chunk::from_vec(vec![1i64, 9, 3]);
      let b = Chunk::from_vec(vec![1i64, 2, 3]);
      assert_eq!(a.compare_by(&b, &order::number_i64()), Ordering::Greater);
    }

    #[test]
    fn compare_by_equal_length_equal_elements_are_equal() {
      let a = Chunk::from_vec(vec![1i64, 2, 3]);
      let b = Chunk::from_vec(vec![1i64, 2, 3]);
      assert_eq!(a.compare_by(&b, &order::number_i64()), Ordering::Equal);
    }
  }

  mod effect_hash {
    use super::Chunk;
    use crate::schema::equal::EffectHash;
    use std::collections::HashMap;

    #[test]
    fn chunk_is_usable_as_hash_map_key_when_element_hashes() {
      let mut m = HashMap::new();
      m.insert(Chunk::from_vec(vec![1i32, 2]), "a");
      m.insert(Chunk::from_vec(vec![3i32]), "b");
      assert_eq!(m.get(&Chunk::from_vec(vec![1, 2])), Some(&"a"));
      assert_eq!(m.get(&Chunk::from_vec(vec![3])), Some(&"b"));
      assert_eq!(m.len(), 2);
    }

    #[test]
    fn effect_hash_matches_std_hash_for_equal_chunks() {
      let x = Chunk::from_vec(vec![7u8, 8]);
      let y = Chunk::from_vec(vec![7u8, 8]);
      assert_eq!(x.effect_hash(), y.effect_hash());
    }
  }
}
