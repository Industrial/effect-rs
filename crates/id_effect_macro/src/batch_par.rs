//! Optional rayon helpers for **independent** batch work in code that also uses this crate’s
//! macros. Macro expansion itself is **compile-time only**; this module is for runtime data-parallel
//! `map` over slices when you need it.

use rayon::prelude::*;

/// Map `f` over `items` in parallel. Output order matches `items`.
#[inline]
pub fn map_slice_par<T, R, F>(items: &[T], f: F) -> Vec<R>
where
  T: Sync,
  R: Send,
  F: Fn(&T) -> R + Send + Sync,
{
  items.par_iter().map(f).collect()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn map_slice_par_preserves_order() {
    let xs = [1_i32, 2, 3];
    let y = map_slice_par(&xs, |n| n + 10);
    assert_eq!(y, vec![11, 12, 13]);
  }
}
