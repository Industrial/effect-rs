# What Are Fibers? — Lightweight Structured Tasks

A Fiber is an effect-managed async task. It's lighter than an OS thread and safer than a raw `tokio::spawn`.

## Fibers vs. Raw Tasks

```rust
// Raw tokio::spawn — fire and forget
// Who owns this? What happens if it panics?
// When does it stop? Who cleans up?
tokio::spawn(async {
    do_something().await;
});

// Effect Fiber — explicit lifecycle
let handle: FiberHandle<Result, Error> = my_effect.fork();
// You hold the handle. The fiber is yours.
let exit: Exit<Error, Result> = handle.join().await;
// The fiber stops when you join or drop the handle.
```

With `tokio::spawn`, the task runs independently. If it panics, the panic is captured by Tokio and may or may not surface to you. There's no built-in way to cancel it or guarantee its resources are cleaned up.

With `fork`, you get a `FiberHandle`. When you `join()`, you get the full `Exit` — success, typed failure, panic, or cancellation. When you drop the handle without joining, the fiber is cancelled automatically.

## Structured Concurrency

The key property of Fibers is *structured* lifecycle:

- A fiber cannot outlive its parent scope without explicit permission
- All spawned fibers are joined (or cancelled) before the parent effect completes
- Panics and failures propagate through the fiber tree, not silently into the void

This makes concurrent code much easier to reason about. When `process_batch` completes, all its helper fibers have completed too — or been cancelled and cleaned up.

## FiberId

Each Fiber has a unique `FiberId`. You can use it for logging, tracing, and correlation:

```rust
use effect::FiberId;

effect! {
    let id = ~ current_fiber_id();
    ~ log(&format!("[fiber:{id}] starting work"));
    // ...
}
```

`FiberId` flows through the fiber's execution automatically. You don't thread it manually.

## FiberHandle and FiberStatus

`FiberHandle<E, A>` is the control interface for a spawned fiber:

```rust
let handle = my_effect.fork();

// Check status without blocking
let status: FiberStatus = handle.status();

// Join — blocks until the fiber completes
let exit: Exit<E, A> = handle.join().await;

// Interrupt — ask the fiber to stop
handle.interrupt();
```

`FiberStatus` can be `Running`, `Completed`, or `Interrupted`. Unlike `tokio::JoinHandle`, you can inspect status without consuming the handle.
