# The Resource Problem — Cleanup in Async

RAII in synchronous code:

```rust
{
    let file = File::open("data.txt")?;
    process(&file)?;
}  // file.drop() runs here, always, unconditionally
```

Reliable. Simple. The drop happens when the scope ends — no exceptions (unless you have exceptions).

## The Async Complication

```rust
async fn process_data() -> Result<(), Error> {
    let conn = open_connection().await?;
    let data = fetch_data(&conn).await?;  // What if this is cancelled?
    transform_and_save(data).await?;      // Or this?
    conn.close().await?;                  // May never reach here
    Ok(())
}
```

Three problems:

1. **Cancellation**: If this async function is cancelled mid-execution, `conn.close()` never runs.
2. **Panic**: If `transform_and_save` panics, the async task is dropped. `conn.close()` is skipped.
3. **Async Drop**: `impl Drop for Connection` can only do synchronous cleanup. If closing a connection requires `.await`, you can't do it in `Drop`.

`conn.close()` must be an async call, but `Drop` can't be async. This is a fundamental mismatch.

## The Root Cause

RAII relies on `Drop` running synchronously when a value goes out of scope. In async code, "going out of scope" and "running cleanup" can be decoupled — by cancellation, by executor scheduling, or by the fact that async closures are state machines that might never reach certain states.

## The Solution Preview

effect-rs solves this with:
- **`Scope`** — a region where finalizers are registered and guaranteed to run (even on cancellation or panic)
- **`acquire_release`** — a combinator that pairs acquisition with its cleanup
- **`Pool`** — for long-lived resources that need controlled reuse

All three run cleanup effects (not just synchronous `Drop`), and all three run them unconditionally — success, failure, or interruption.
