# Testing — Effects Are Easy to Test

Testing async code often means standing up infrastructure, dealing with timing, and wide mocks—then chasing occasional flakes in CI.

Effect programs can be tested differently. Because an `Effect` is a description of what to do — not the doing itself — you control everything about how it runs. Swap in a test clock. Provide fake services through the `Layer` system. Detect fiber leaks automatically. Run in microseconds instead of seconds.

This chapter covers the testing tools id_effect provides.

## What Makes Effects Testable

Three properties make effect programs easy to test:

**1. Services are injected, not ambient.**
Your code doesn't call `DatabaseClient::global()`. It declares `R: NeedsDb` and gets its database from the environment. In tests, you provide a different environment — one with a fake database.

**2. Time is injectable.**
Code that uses `Clock` instead of `std::time::SystemTime::now()` can be tested with `TestClock`, which advances only when you tell it to.

**3. Effects don't run until you run them.**
An `Effect` is inert. You can inspect, compose, and modify it before running. `run_test` runs it in a harness that adds leak detection and deterministic scheduling.

## What This Chapter Covers

- **`run_test`** — the test harness that replaces `run_blocking` in tests ([next section](./ch15-01-run-test.md))
- **`TestClock`** — deterministic time control in tests ([ch15-02](./ch15-02-test-clock.md))
- **Mocking services** — injecting test doubles via layers ([ch15-03](./ch15-03-mocking.md))
- **Property testing** — generating inputs and checking invariants ([ch15-04](./ch15-04-property-testing.md))
