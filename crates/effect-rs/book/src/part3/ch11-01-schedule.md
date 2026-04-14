# Schedule — The Retry/Repeat Policy Type

A `Schedule` is not just a number of retries or a delay. It's a *policy* — a function that takes the current state (attempt count, elapsed time, last output) and decides whether to continue and how long to wait.

## The Core Concept

```rust
use effect::Schedule;

// A Schedule answers: "Given where we are, should we continue? And after how long?"
// Input: attempt number, elapsed time, last result
// Output: Continue(delay) or Done
```

This abstraction is more powerful than "retry 3 times with 1-second delay." A Schedule can:
- Increase delay exponentially (standard backoff)
- Cap the total time regardless of attempts
- Stop after a maximum number of attempts
- Adjust based on the error type or last result
- Combine policies with `&&` and `||`

## Creating Schedules

```rust
use effect::Schedule;

// Fixed delay: always wait the same amount
let fixed = Schedule::spaced(Duration::from_secs(1));

// Exponential: 100ms, 200ms, 400ms, 800ms, ...
let exponential = Schedule::exponential(Duration::from_millis(100));

// Fibonacci: 100ms, 100ms, 200ms, 300ms, 500ms, 800ms, ...
let fibonacci = Schedule::fibonacci(Duration::from_millis(100));

// Forever: repeat indefinitely with no delay
let forever = Schedule::forever();

// Once: run exactly once (useful for testing)
let once = Schedule::once();
```

## Combining Schedules

Schedules compose:

```rust
// Retry up to 5 times
let max_5 = Schedule::exponential(100.ms()).take(5);

// But stop after 30 seconds total
let bounded = Schedule::exponential(100.ms()).until_total_duration(Duration::from_secs(30));

// Combine with &&: both conditions must agree to continue
let safe = Schedule::exponential(100.ms())
    .take(5)
    .until_total_duration(Duration::from_secs(30));
```

## Schedule as a Value

Like effects, schedules are values. You can define them once and reuse them:

```rust
const DEFAULT_RETRY: Schedule = Schedule::exponential(Duration::from_millis(100))
    .take(5)
    .with_jitter(Duration::from_millis(50));

fn call_external_api() -> Effect<Response, ApiError, HttpClient> {
    make_request().retry(DEFAULT_RETRY)
}
```

Jitter (random delay variation) reduces thundering-herd problems when many processes retry simultaneously. `.with_jitter(d)` adds a random delay in `[0, d)` to each wait.
