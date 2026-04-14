# Creating Effects — succeed, fail, and pure

Every effect starts as either a success or a failure. The two constructors that express this are `succeed` and `fail`.

## succeed

`succeed` wraps a value into an effect that, when run, immediately produces that value:

```rust
use effect::{Effect, succeed};

let answer: Effect<i32, String, ()> = succeed(42);
let greeting: Effect<String, String, ()> = succeed("Hello, world!".to_string());
```

Nothing happens when you call `succeed`. You get back a description — a lazy recipe that says "produce this value when someone asks." The `42` is already there, but no computation has been executed.

The type parameters are important:
- `A = i32` — the value we produce
- `E = String` — the error type (unused here, but we still have to pick one)
- `R = ()` — no environment needed

If you prefer the FP vocabulary, `pure` is an alias for `succeed`:

```rust
use effect::pure;

let effect = pure(42_i32);
```

Both names refer to exactly the same thing. Use whichever feels natural in context.

## fail

`fail` wraps an error into an effect that, when run, immediately fails with that error:

```rust
use effect::{Effect, fail};

let oops: Effect<i32, String, ()> = fail("something went wrong".to_string());
```

Again, nothing executes. `oops` is a description of a failure, not the failure itself. You can pass it around, store it, and transform it without triggering any error handling.

The type annotation matters: `Effect<i32, String, ()>` says this would have produced an `i32` on success — we just know it won't.

## From a Closure

For cases where you want to capture some computation in an effect (but still defer it):

```rust
use effect::{Effect, effect};

let computed: Effect<i32, String, ()> = effect!(|_r: &mut ()| {
    let x = expensive_calculation();
    x * 2
});
```

The body of `effect!` runs lazily — only when the effect is executed. This is the workhorse macro we'll cover thoroughly in Chapter 3.

## Type Inference

Rust's type inference often lets you skip the annotations:

```rust
// Types inferred from usage
let answer = succeed(42);      // Effect<i32, _, ()>
let greeting = succeed("hi"); // Effect<&str, _, ()>
```

The error type `E` is usually inferred from how the effect is used later — when you chain it with other effects that can fail, the error type propagates. You'll only need to annotate explicitly when the compiler asks.

## Quick Reference

```rust
succeed(value)    // Effect that produces value
pure(value)       // Alias for succeed
fail(error)       // Effect that fails with error
effect!(|_r| { … }) // Effect from a lazy closure
```

These three constructors cover every starting point. Everything else is transformation and composition.
