# Transforming Success — map and its Friends

You have an effect. It produces some value. But you want a different value — or a different error. That's what `map` and `map_error` are for.

## map

`map` transforms the success value without running any new effects:

```rust
use effect::{succeed, Effect};

let number: Effect<i32, String, ()> = succeed(21);
let doubled: Effect<i32, String, ()> = number.map(|n| n * 2);
let text: Effect<String, String, ()> = doubled.map(|n| n.to_string());
```

None of these `.map()` calls executes anything. Each one wraps the previous description in a new layer: "and then transform the result with this function." The chain of transformations only runs when you call `run_blocking` or similar.

The type of the effect changes with each `map`. The `A` parameter shifts:

```rust
// Effect<i32, String, ()>
//   .map(|n: i32| n.to_string())
// → Effect<String, String, ()>
```

The `E` (error type) and `R` (requirements) stay the same. `.map` touches only the success path.

## map_error

`map_error` transforms the failure type, leaving the success path untouched:

```rust
use effect::fail;

#[derive(Debug)]
struct AppError(String);

let db_err: Effect<String, String, ()> = fail("db connection failed".to_string());
let app_err: Effect<String, AppError, ()> = db_err.map_error(|s| AppError(s));
```

This is typically used at module boundaries when you need to unify error types. A database layer might return `DbError`, but your application layer needs `AppError`. `map_error` does the conversion without touching anything else.

## Why These Don't Execute Anything

It's worth repeating: neither `map` nor `map_error` runs any computation.

```rust
let effect = succeed(42)
    .map(|n| { println!("mapping!"); n + 1 })
    .map(|n| n * 2);

// At this point: nothing has printed, nothing has computed.
// We have a description of three steps.

let result = run_blocking(effect.provide(()));
// NOW the effect runs. "mapping!" prints once. Result is 86.
```

This is the promise of laziness: you can build pipelines of transformations without triggering side effects until the moment you choose.

## Combining map and map_error

A common pattern is calling both to normalise an effect into your domain's types:

```rust
fn fetch_user_record(id: u64) -> Effect<User, AppError, ()> {
    raw_db_fetch(id)
        .map(|row| User::from_row(row))
        .map_error(|e| AppError::Database(e))
}
```

The effect goes in with raw DB types; it comes out with domain types. The transformation chain documents the conversion at a glance.

## and_then / tap (convenience)

Two more helpers are worth knowing:

```rust
// and_then: map + flatten (when your mapper returns an Option or Result)
let validated: Effect<i32, String, ()> = succeed(42)
    .and_then(|n| if n > 0 { Some(n) } else { None });

// tap: inspect the success value without changing it
let logged: Effect<i32, String, ()> = succeed(42)
    .tap(|n| println!("value: {n}"));  // side-effect, same type flows through
```

`tap` is particularly useful for debugging — add it anywhere in a chain without disrupting the types.

## Summary

| Method | Changes | Does not change |
|--------|---------|-----------------|
| `.map(f)` | `A` (success type) | `E`, `R` |
| `.map_error(f)` | `E` (error type) | `A`, `R` |
| `.tap(f)` | nothing | `A`, `E`, `R` |

None of them execute the effect. They all return new, larger descriptions.
