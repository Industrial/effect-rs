# Built-in Schedules — exponential, fibonacci, forever

effect-rs provides a library of common schedule types. This section catalogs them with typical use cases.

## exponential

```rust
Schedule::exponential(Duration::from_millis(100))
// Delays: 100ms, 200ms, 400ms, 800ms, 1600ms, ...
```

The standard backoff pattern. Each delay doubles. Use for most network retry scenarios — it backs off quickly and gives the downstream service time to recover.

```rust
// With cap: 100ms, 200ms, 400ms, 800ms, 800ms, 800ms (capped at 800ms)
Schedule::exponential(Duration::from_millis(100))
    .with_max_delay(Duration::from_millis(800))
```

## fibonacci

```rust
Schedule::fibonacci(Duration::from_millis(100))
// Delays: 100ms, 100ms, 200ms, 300ms, 500ms, 800ms, 1300ms, ...
```

Fibonacci backoff grows more gradually than exponential — useful when you want more retries in the early attempts before backing off significantly.

## spaced

```rust
Schedule::spaced(Duration::from_secs(5))
// Delays: 5s, 5s, 5s, 5s, ... (constant)
```

Fixed interval. Use for polling (checking status every N seconds), heartbeats, or scenarios where the delay should be predictable.

## forever

```rust
Schedule::forever()
// No delay between repetitions; runs indefinitely
```

Run an effect as fast as possible, forever. Use with `repeat` for continuous background jobs (e.g., metrics collection, background syncs). Almost always combined with `.take(n)` or `.until_total_duration(d)`.

## Limiters

Limiters constrain any schedule:

```rust
// Stop after N attempts
schedule.take(5)

// Stop after N successes (for repeat)
schedule.take_successes(3)

// Stop after total elapsed time
schedule.until_total_duration(Duration::from_secs(30))

// Stop after N total elapsed retries including initial
schedule.take_with_initial(6)  // 1 initial + 5 retries
```

## Jitter

```rust
// Add random delay in [0, jitter_max) to each wait
schedule.with_jitter(Duration::from_millis(50))

// Alternatively, full jitter (random in [0, delay])
schedule.with_full_jitter()
```

Jitter prevents retry storms. When 1000 clients all hit a failing service and all retry at the same time, they create a "thundering herd." Jitter spreads the retries out.

## Combining with &&

```rust
let bounded_exponential = Schedule::exponential(Duration::from_millis(100))
    .take(5)
    .with_jitter(Duration::from_millis(20))
    .until_total_duration(Duration::from_secs(10));
```

The `&&` (chain) operation means "continue only if both schedules agree to continue." The first schedule that says "done" wins.
