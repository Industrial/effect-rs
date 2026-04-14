# Error Handling Inside effect!

The `~` operator short-circuits on failure — if a bound effect fails, the whole `effect!` block fails with that error. But you can also handle errors *within* the block.

## The Default: Short-Circuit

```rust
effect! {
    let a = ~ step_a();    // if this fails → whole block fails
    let b = ~ step_b(a);   // if this fails → whole block fails
    b
}
```

This matches `?` in `Result`. You get clean sequencing at the cost of aborting early. For most code, that's exactly what you want.

## Catching Errors Mid-block

To handle an error inline and continue, use `.catch` before the `~`:

```rust
effect! {
    let user = ~ fetch_user(id).catch(|_| succeed(User::anonymous()));
    // If fetch_user fails, we get User::anonymous() and continue
    render_user(user)
}
```

`.catch` converts a failure into a success (or a different effect). The `~` then sees a successful effect.

## Converting Errors with map_error

Often you have multiple effect types with different `E` parameters and need to unify them:

```rust
#[derive(Debug)]
enum AppError {
    Db(DbError),
    Network(HttpError),
}

effect! {
    let user = ~ fetch_user(id).map_error(AppError::Db);
    let data = ~ fetch_external_data(user.id).map_error(AppError::Network);
    process(user, data)
}
```

Both effects are converted to the same `AppError` before binding. The block's `E` parameter is `AppError` throughout.

## Handling Errors with fold

`fold` handles both success and failure paths:

```rust
effect! {
    let outcome = ~ risky_operation().fold(
        |err| format!("Error: {err}"),
        |val| format!("Success: {val}"),
    );
    // outcome is always Ok(String), never fails here
    log_outcome(outcome)
}
```

`fold` is like pattern matching on the effect — you handle both arms and produce a uniform success value.

## Re-raising Errors

Inside a `.catch` handler, you can inspect the error and decide whether to recover or re-fail:

```rust
effect! {
    let result = ~ db_operation().catch(|error| {
        if error.is_transient() {
            // Transient: retry once with a fallback
            fallback_db_operation()
        } else {
            // Permanent: re-raise
            fail(error)
        }
    });
    result
}
```

`fail(error)` inside a handler produces a failing effect — the outer `~` then propagates it.

## Accumulating Multiple Errors

Short-circuit stops at the first error. When you need *all* errors (like form validation), use `validate_all` outside the macro:

```rust
// Not inside effect! — runs all regardless of failures
let results = validate_all(vec![
    validate_name(&input.name),
    validate_email(&input.email),
    validate_age(input.age),
]);

// results is Effect<Vec<Ok>, Vec<Err>, ()>
```

Chapter 8 covers `validate_all` and error accumulation patterns in detail.

## The Rule of Thumb

| Want | Do |
|------|----|
| Stop at first failure | plain `~ effect` |
| Provide a fallback | `~ effect.catch(|e| fallback)` |
| Unify error types | `~ effect.map_error(Into::into)` |
| Pattern match both arms | `~ effect.fold(on_err, on_ok)` |
| Collect all failures | `validate_all` outside the macro |
