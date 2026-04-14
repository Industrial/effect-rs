# Beyond Result — Why Cause Exists

`Result<T, E>` handles the errors you expect. But what about the errors that aren't your `E`?

## Expected vs. Unexpected

```rust
// Expected: you planned for this
Effect<User, UserNotFound, Db>

// But what if the database panics?
// What if the fiber is cancelled?
// What if the process runs out of memory?
// None of these are UserNotFound.
```

Traditional Rust handles unexpected failures through panics, which unwind (or abort) and bypass all your error handling. In async code, panics in tasks can silently swallow errors or leave resources unreleased.

## The Cause Type

`Cause<E>` is effect-rs's complete taxonomy of failure:

```rust
use effect::Cause;

enum Cause<E> {
    Fail(E),                // Your typed, expected error
    Die(Box<dyn Any>),      // A panic or defect — something that shouldn't happen
    Interrupt,              // The fiber was cancelled
}
```

Every failure in the effect runtime is one of these three. Together they cover the full space of "things that can go wrong."

- `Cause::Fail(e)` — an error you declared in `E`, handled with `catch` or `map_error`
- `Cause::Die(payload)` — a panic, logic bug, or fatal error; should be logged and treated as a defect
- `Cause::Interrupt` — clean cancellation; the fiber was asked to stop and cooperated

## Why This Matters

Without `Cause`, you can only handle `Cause::Fail`. The other two propagate invisibly up the fiber tree and may silently swallow logs or leave resources unreleased.

With `Cause`, you can handle *all* failure modes in a structured way:

```rust
my_effect.catch_all(|cause| match cause {
    Cause::Fail(e)    => recover_from_expected(e),
    Cause::Die(panic) => log_defect_and_fail(panic),
    Cause::Interrupt  => succeed(default_value()),
})
```

Resource finalizers (Chapter 10) use this same model — they run on any `Cause`, ensuring cleanup regardless of how the fiber ends.

## Day-to-Day Usage

In normal application code you rarely inspect `Cause` directly. You use:
- `.catch(f)` for handling `Cause::Fail`
- `.catch_all(f)` when you need to handle panics or interruption too
- `Exit` (next section) when you need to inspect the terminal outcome

The `Cause` type is mostly visible at infrastructure boundaries — resource finalizers, fiber supervisors, and top-level error handlers.
