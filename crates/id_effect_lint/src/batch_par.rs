//! Parallel batch helpers (rayon) for **independent** work in tooling and tests.
//!
//! The rustc driver runs the Effect.rs lint pass on a single thread; do not use these from
//! [`LateLintPass::check_*`](rustc_lint::LateLintPass) with shared `LateContext` (HIR access is
//! not parallel-safe there).

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
    let xs = [10_i32, 20, 30];
    let y = map_slice_par(&xs, |n| n.saturating_mul(2));
    assert_eq!(y, vec![20, 40, 60]);
  }
}
