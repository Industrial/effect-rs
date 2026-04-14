# TRef — Transactional References

`TRef<T>` is the fundamental mutable cell in effect-rs's STM system. It wraps a value that can be read and written inside transactions.

## Creating a TRef

```rust
use effect::TRef;

let counter: TRef<i32> = TRef::new(0);
let balance: TRef<f64> = TRef::new(1000.0);
```

`TRef::new(value)` creates a transactional reference with an initial value. TRefs are typically created once (at startup or when initialising shared state) and then shared across fibers via `Arc`.

## Transactional Operations

All TRef operations return `Stm<_>` — transactional descriptions, not effects. They only work inside `stm!` (or when run through `commit`/`atomically`):

```rust
use effect::{TRef, stm};

let counter = TRef::new(0);

// Read inside a transaction
let read_op: Stm<i32> = counter.read_stm();

// Write inside a transaction
let write_op: Stm<()> = counter.write_stm(42);

// Modify (read-write atomically)
let modify_op: Stm<()> = counter.modify_stm(|n| n + 1);
```

These are descriptions. Nothing happens until they're committed.

## Running Inside stm!

The `stm!` macro provides do-notation for composing `Stm` operations, exactly like `effect!` does for `Effect`:

```rust
use effect::{stm, TRef};

let counter = TRef::new(0_i32);
let total   = TRef::new(0_i32);

let transaction: Stm<()> = stm! {
    let count = ~ counter.read_stm();
    let sum   = ~ total.read_stm();
    ~ counter.write_stm(count + 1);
    ~ total.write_stm(sum + count);
    ()
};
```

## Sharing TRefs

TRefs are `Clone + Send + Sync`. Wrap in `Arc` to share across fibers:

```rust
use std::sync::Arc;

let shared: Arc<TRef<i32>> = Arc::new(TRef::new(0));

// Clone for each fiber
let clone1 = Arc::clone(&shared);
let clone2 = Arc::clone(&shared);

fiber_all(vec![
    increment_n_times(clone1, 1000),
    increment_n_times(clone2, 1000),
])
// Result: counter = 2000 (atomically, without locks)
```

## TRef vs. Mutex<T>

| Property | TRef | Mutex |
|----------|------|-------|
| Composable across updates | ✓ | ✗ |
| Deadlock-free | ✓ | ✗ |
| Blocking read | ✗ never | ✓ blocks writers |
| Works with I/O | ✗ | ✓ |
| Overhead | retry cost | lock/unlock cost |

Use `TRef` for short, composable state mutations. Use `Mutex` when you need to hold a lock across I/O (though ideally you redesign to avoid that).
