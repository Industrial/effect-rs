# Recovery Combinators — catch, fold, and Friends

Knowing about `Cause` and `Exit` is only useful if you can act on them. effect-rs provides a focused set of recovery combinators.

## catch: Handle Expected Errors

`catch` intercepts `Cause::Fail(e)` and gives you a chance to recover:

```rust
let resilient = risky_db_call()
    .catch(|error: DbError| {
        match error {
            DbError::NotFound => succeed(User::anonymous()),
            other => fail(other),  // re-raise anything else
        }
    });
```

If `risky_db_call` fails with `Cause::Fail(e)`, the closure runs. If it fails with `Cause::Die` or `Cause::Interrupt`, those propagate unchanged — `catch` only handles typed failures.

## catch_all: Handle Everything

`catch_all` intercepts any `Cause`:

```rust
let bulletproof = my_effect.catch_all(|cause| match cause {
    Cause::Fail(e)   => handle_expected_error(e),
    Cause::Die(_)    => succeed(fallback_value()),
    Cause::Interrupt => succeed(cancelled_gracefully()),
});
```

Use `catch_all` when you genuinely need to handle panics or cancellation — typically at resource boundaries or top-level handlers. Don't use it to swallow defects silently.

## fold: Handle Both Paths

`fold` transforms both success and failure into a uniform success type:

```rust
let always_string: Effect<String, Never, ()> = risky_call()
    .fold(
        |error| format!("Error: {error}"),
        |value| format!("Success: {value}"),
    );
```

After `fold`, the effect never fails (`E = Never`). Both arms produce the same type. This is useful for logging, metrics, or converting to a neutral representation.

## or_else: Try an Alternative

`or_else` runs an alternative effect on failure:

```rust
let with_fallback = primary_source()
    .or_else(|_err| secondary_source());
```

If `primary_source` fails, `secondary_source` runs. If that also fails, the combined effect fails with the second error. Useful for fallback chains.

## ignore_error: Discard Failures

When you genuinely don't care about an operation's success:

```rust
// Log "best effort" — failure is acceptable
let logged_effect = log_metrics()
    .ignore_error()
    .flat_map(|_| actual_work());
```

`ignore_error` converts `Effect<A, E, R>` to `Effect<Option<A>, Never, R>`. The effect always "succeeds" — with `Some(value)` on success or `None` on failure.

## The Recovery Hierarchy

```
catch(f)      — handles Cause::Fail only
catch_all(f)  — handles all Cause variants
fold(on_e, on_a) — transforms both paths to success
or_else(f)    — runs alternative on failure
ignore_error  — converts failure to Option
```

Prefer the narrowest combinator that solves your problem. `catch` for expected errors. `catch_all` only when you need to touch panics or cancellation.
