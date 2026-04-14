# Clock Injection — Testable Time

`Schedule::exponential(100.ms()).take(3)` with real retries takes ~700ms to test. Multiply by hundreds of tests and your test suite takes minutes. `Clock` injection solves this.

## The Clock Trait

```rust
use effect::{Clock};

// The Clock trait abstracts time
trait Clock: Send + Sync {
    fn now(&self) -> UtcDateTime;
    fn sleep(&self, duration: Duration) -> Effect<(), Never, ()>;
}
```

All time-related operations in effect-rs go through the current fiber's `Clock`. Replace the clock, and "time" moves as fast as you drive it.

## Production: LiveClock

```rust
use effect::LiveClock;

// Uses real system time and tokio::time::sleep
let live_clock = LiveClock::new();
```

In production, inject `LiveClock` through the environment. Effect code never calls `std::time::SystemTime::now()` directly — it uses the injected clock.

## Testing: TestClock

```rust
use effect::TestClock;

let clock = TestClock::new();

// The clock starts at epoch and doesn't advance on its own
assert_eq!(clock.now(), EPOCH);

// Advance time instantly
clock.advance(Duration::from_secs(60));
assert_eq!(clock.now(), EPOCH + 60s);
```

`TestClock` is deterministic. It advances only when you tell it to. Sleep effects don't wait — they check the clock, and if the clock is past their wake time, they return immediately.

## Test Example

```rust
#[test]
fn exponential_retry_makes_three_attempts() {
    let clock = TestClock::new();
    let attempts = Arc::new(AtomicU32::new(0));

    let effect = {
        let attempts = attempts.clone();
        failing_operation(attempts.clone())
            .retry(Schedule::exponential(Duration::from_secs(1)).take(3))
    };

    // Fork the effect with the test clock
    let handle = effect.fork_with_clock(&clock);

    // Advance time to trigger each retry
    clock.advance(Duration::from_secs(1));   // retry 1
    clock.advance(Duration::from_secs(2));   // retry 2
    clock.advance(Duration::from_secs(4));   // retry 3 (exhausted)

    let exit = handle.join_blocking();
    assert!(matches!(exit, Exit::Failure(_)));
    assert_eq!(attempts.load(Ordering::Relaxed), 4);  // initial + 3 retries
}
```

The test runs in microseconds despite testing multi-second retry behaviour. No `tokio::time::pause()` hacks. No `sleep(Duration::ZERO)` workarounds.

## Clock in the Environment

Like all services, `Clock` lives in the effect environment:

```rust
service_key!(ClockKey: Arc<dyn Clock>);

fn now() -> Effect<UtcDateTime, Never, impl NeedsClock> {
    effect! {
        let clock = ~ ClockKey;
        clock.now()
    }
}
```

The production Layer provides `LiveClock`; test code provides `TestClock`. Business logic is identical in both contexts.
