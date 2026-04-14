# retry and repeat — Applying Policies

`Schedule` is a policy description. `retry` and `repeat` are the two operations that apply it.

## retry: On Failure, Try Again

```rust
use effect::Schedule;

let result = flaky_api_call()
    .retry(Schedule::exponential(Duration::from_millis(100)).take(3));
```

`retry` runs the effect. If it fails, it checks the schedule. If the schedule says "continue", it waits the indicated delay and tries again. When the schedule says "done" or the effect succeeds, `retry` returns.

Return value: the success value on success, or the last error if all retries are exhausted.

## retry_while: Conditional Retry

Not all errors are retriable. Retry only when the error matches a condition:

```rust
let result = api_call()
    .retry_while(
        Schedule::exponential(Duration::from_millis(100)).take(5),
        |error| error.is_transient(),  // only retry transient errors
    );
```

Permanent errors (e.g., 404 Not Found, permission denied) shouldn't be retried — they won't go away. `.retry_while` lets you distinguish them.

## repeat: On Success, Run Again

```rust
let polling = check_job_status()
    .repeat(Schedule::spaced(Duration::from_secs(5)));
```

`repeat` runs the effect. When it *succeeds*, it checks the schedule. If the schedule says "continue", it waits and runs again. This is the complement of `retry`: same mechanism, triggered by success instead of failure.

Use cases:
- Poll for job completion every 5 seconds
- Send heartbeats every 30 seconds
- Refresh a cache on a fixed interval

## repeat_until: Stop When Condition Met

```rust
let waiting_for_ready = poll_service()
    .repeat_until(
        Schedule::spaced(Duration::from_secs(1)),
        |status| status == ServiceStatus::Ready,
    );
```

`repeat_until` repeats until the success value satisfies a predicate. When the condition is met, it stops and returns the value.

## Composition with Other Operations

`retry` and `repeat` return effects — they compose like everything else:

```rust
// Retry the individual call, then repeat the whole batch
let batch = process_single_item(item)
    .retry(Schedule::exponential(100.ms()).take(3));

let continuous = batch
    .repeat(Schedule::spaced(Duration::from_secs(60)));
```

## Error Information in Retry

If you need to inspect errors during retry (for logging, metrics, etc.):

```rust
let instrumented = risky_call()
    .retry_with_feedback(
        Schedule::exponential(100.ms()).take(3),
        |attempt, error| {
            // Called before each retry
            println!("Attempt {attempt} failed: {error:?}");
        },
    );
```

`retry_with_feedback` passes the attempt number and the error to a side-effectful callback before each retry. Useful for structured logging of retry behaviour.
