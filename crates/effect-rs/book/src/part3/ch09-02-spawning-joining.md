# Spawning and Joining — fiber_all and Friends

Running a single fiber is useful; running many concurrently is where Fibers shine.

## fork: Spawn One Fiber

```rust
let handle = compute_expensive_result().fork();

// Do other work while the fiber runs
let local_result = local_computation();

// Now join the fiber
let remote_result = handle.join().await.into_result_or_panic()?;

(local_result, remote_result)
```

`fork` spawns the effect as a concurrent fiber. You can do other work and join later.

## fiber_all: Run Many, Collect All

```rust
use effect::fiber_all;

// Run all concurrently; collect all results
let results: Vec<User> = run_blocking(
    fiber_all(user_ids.iter().map(|&id| fetch_user(id)))
)?;
```

`fiber_all` takes an iterable of effects, runs them all concurrently, and waits for every one to complete. If any fails, the first failure is returned (and any remaining fibers are cancelled).

For independent work where all results are needed, `fiber_all` is the idiomatic choice.

## fiber_race: First to Complete Wins

```rust
use effect::fiber_race;

// Try primary and backup concurrently — take whichever responds first
let data = run_blocking(
    fiber_race(vec![fetch_from_primary(), fetch_from_backup()])
)?;
// The slower fiber is automatically cancelled
```

`fiber_race` returns as soon as any fiber succeeds. The others are interrupted. Useful for timeout patterns, geographic failover, and speculative execution.

## fiber_any: First Success

```rust
use effect::fiber_any;

// Try all; return first success (ignore failures until all done)
let result = fiber_any(vec![
    try_region_us(),
    try_region_eu(),
    try_region_ap(),
])?;
```

`fiber_any` differs from `fiber_race` in that it ignores failures and waits for the first *success*. Only if all fail does it return an error.

## run_fork: Low-Level Spawn

For cases where you need to spawn with full control:

```rust
use effect::run_fork;

let runtime = Runtime::current();
let handle = run_fork(runtime, || (my_effect, my_env));
```

`run_fork` is the low-level primitive. `effect.fork()` is syntactic sugar over it when you're already inside an effect context.

## Error Behaviour

| Combinator | On any failure |
|------------|---------------|
| `fiber_all` | Cancel remaining, return first error |
| `fiber_race` | Cancel remaining, return first success |
| `fiber_any` | Wait for all, return first success or all errors |
| `fork` + `join` | Whatever the individual fiber's `Exit` says |

Choose based on whether partial success is acceptable and whether you want to wait for everyone.
