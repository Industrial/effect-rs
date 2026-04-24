# Supervision — Restart Policies and Scope

Raw [`run_fork`](../appendix-a-api-reference.md) gives you a [`FiberHandle`](../appendix-a-api-reference.md) and full control. Long-lived servers usually need **policies**: when a child effect fails, should you **retry**, **back off**, **give up**, or **substitute a default**? Effect.ts encodes these ideas in **supervisors**. `id_effect` provides the same vocabulary wired to [`Scope`](./ch10-02-scopes-finalizers.md), [`CancellationToken`](./ch09-03-cancellation.md), and [`Schedule`](../ch11-01-schedule.md).

## Why not only `retry`?

[`retry`](../ch11-03-retry-repeat.md) re-runs an `Effect<A, E, R>` until success or the schedule stops. Supervision adds:

- A **stable shutdown channel** ([`CancellationToken`]) installed when a **child [`Scope`]** closes, so cooperative loops exit with [`Cause::Interrupt`](./ch08-02-exit.md) instead of spinning forever.
- **Declarative policies** (`Terminate`, `Restart`, `RestartWithLimit`, `Escalate`, `Ignore`) that compose with **virtual time** ([`TestClock`](../ch15-02-test-clock.md)) for deterministic tests.

## `Supervisor` and `Scope`

`Supervisor::attach(parent_scope)` forks a child [`Scope`]. When the parent closes, the child closes; a **finalizer** on the child cancels the supervisor token so any `supervised` loop observes cancellation on the next iteration header.

Use `Supervisor::detached()` for examples and unit tests that do not need a parent tree.

## Policies in one table

| Policy | On child `Ok(a)` | On child `Err(e)` |
|--------|------------------|-------------------|
| `Terminate` / `Escalate` | Return `a` | Return [`Cause::Fail(e)]` (no retry) |
| `Restart { schedule }` | Return `a` | Sleep per `schedule`, run factory again |
| `RestartWithLimit { limit, schedule }` | Return `a` | Retry while under `limit`; then fail with [`Cause::Then`](./ch08-04-accumulation.md) aggregating prior failures |
| `Ignore { recover }` | Return `a` | Return `recover` |

`RestartWithLimit` counts **retries after failure**: `limit == 0` means “no retries” (fail on the first `Err` without sleeping). A positive limit allows that many **retry attempts** after the initial failing run.

## Typed failures vs interrupts vs defects

- A supervised child that returns [`Err`] becomes [`Cause::Fail`].
- Token cancellation (scope teardown or explicit `cancel`) surfaces as [`Cause::Interrupt`].
- Panics inside the interpreter are still **defects** at the runtime boundary; supervision does not “recover” unwinding `std` panics—document and test the happy paths your runtime actually exposes.

## `supervised` vs `FiberHandle::scoped`

[`FiberHandle::scoped`](./ch09-02-spawning-joining.md) ties **one handle** to **one scope** via a finalizer that **interrupts** the fiber. `supervised` runs the factory **inline** on your environment `R`, so it suits **retry loops** without an extra `FiberHandle` until you opt into `Supervisor::spawn`.

## Example sketch

```rust
use id_effect::{
  Supervisor, SupervisorPolicy, supervised,
  Schedule, TestClock, succeed,
};
use std::time::Instant;

let parent = id_effect::Scope::make();
let sup = Supervisor::attach(&parent);
let clock = TestClock::new(Instant::now());
let body = supervised(
  &sup,
  SupervisorPolicy::Restart {
    schedule: Schedule::spaced(std::time::Duration::ZERO),
  },
  clock,
  || succeed::<u32, &str, ()>(42),
);
// run `body` with your environment; close `parent` to cancel via token.
```

For production delays, prefer a non-zero [`Schedule`] and a real [`Clock`](../ch11-04-clock-injection.md) (for example the Tokio bridge clock in server code).

## See also

- [Phase F parity doc](../../../../../docs/effect-ts-parity/phases/phase-f-supervision.md) — epic breakdown and acceptance notes.
- [`supervisor.rs`](../../../src/concurrency/supervisor.rs) — full API and unit tests.
