# Cancellation — Interrupting Gracefully

Cancellation in async code is notoriously difficult to get right. effect-rs makes it explicit and cooperative.

## The Model: Cooperative Cancellation

Fibers aren't forcibly killed. They're *interrupted* — given a signal to stop at the next safe checkpoint. The fiber cooperates by checking for interruption at yield points.

The simplest yield point is `check_interrupt`:

```rust
use effect::check_interrupt;

effect! {
    for chunk in large_dataset.chunks(1000) {
        ~ check_interrupt();  // yields; if interrupted, stops here
        process_chunk(chunk);
    }
    "done"
}
```

Every `~` binding is also an implicit yield point. If a fiber is interrupted while awaiting an effect, the interruption propagates to the next `~` bind.

## CancellationToken

For external cancellation (e.g., from HTTP request handlers or UI cancel buttons):

```rust
use effect::CancellationToken;

// Create a cancellation token
let token = CancellationToken::new();

// Pass it to a long-running effect
let effect = long_running_job().with_cancellation(&token);

// Spawn it
let handle = effect.fork();

// Later, cancel from outside
token.cancel();

// The fiber will stop at the next check_interrupt
let exit = handle.join().await;
// exit will be Exit::Failure(Cause::Interrupt)
```

Tokens can be cloned and shared. Cancelling any clone cancels all effects sharing that token.

## Interrupting Directly via FiberHandle

```rust
let handle = background_work().fork();

// After some timeout or external event:
handle.interrupt();

let exit = handle.join().await;
// Cleanup (finalizers, scopes) runs before the handle resolves
```

`.interrupt()` sends the interruption signal. The fiber's finalizers (Chapter 10) still run. `.join()` waits for them to complete.

## Graceful Shutdown

Interruption is the mechanism for graceful shutdown. The pattern:

1. Signal all top-level fibers with `.interrupt()`
2. Wait for all handles to join (with a timeout)
3. If any fiber doesn't stop within the timeout, escalate

```rust
let handles: Vec<FiberHandle<_, _>> = workers.iter().map(|w| w.fork()).collect();

// Shutdown signal received
for h in &handles { h.interrupt(); }

// Wait with timeout
for h in handles {
    tokio::time::timeout(Duration::from_secs(5), h.join()).await;
}
```

Because effect finalizers run on interruption, all resources are cleaned up as fibers stop — no manual cleanup required at the shutdown handler.

## Uninterruptible Regions

Some operations shouldn't be interrupted mid-way (e.g., writing to a database inside a transaction). Mark them uninterruptible:

```rust
use effect::uninterruptible;

// This block runs to completion even if interrupted
let committed = uninterruptible(effect! {
    ~ begin_transaction();
    ~ insert_records();
    ~ commit();
});
```

The interruption is deferred until `committed` completes. Use sparingly — long uninterruptible regions delay shutdown.
