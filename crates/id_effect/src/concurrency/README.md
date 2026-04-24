# `concurrency` — Stratum 7: fibers & cancellation

**Cooperative concurrency**: branded [`FiberId`](fiber_id.rs), joinable [`FiberHandle`](fiber_handle.rs) / [`FiberStatus`](fiber_handle.rs), [`CancellationToken`](cancel.rs), and fiber-local [`FiberRef`](fiber_ref.rs). Fibers are the unit of parallel `Effect` work; cancellation propagates cooperatively via `check_interrupt`.

## What lives here

| Module | Role |
|--------|------|
| `fiber_id` | `FiberId` — stable, monotonic identifiers. |
| `cancel` | `CancellationToken`, `check_interrupt` — cooperative interrupt. |
| `fiber_handle` | `FiberHandle`, `fiber_all`, `interrupt_all`, `fiber_succeed`, … |
| `fiber_ref` | `FiberRef`, `with_fiber_id` — fiber-scoped mutable state (like thread-locals). |
| `supervisor` | `Supervisor`, `SupervisorPolicy`, `supervised` — scope-linked restart and backoff. |
| `async_notify` | Internal wait/notify helpers (not the primary API surface). |

## What it is used for

- **Forking** work with observable completion and typed errors (`FiberHandle`).
- **Propagating** shutdown or timeouts through `CancellationToken`.
- **Attaching** per-fiber diagnostics or context in `FiberRef` (e.g. tracing).
- **Supervising** long-lived workers with [`supervisor`](supervisor.rs) policies and [`Scope`](../resource/README.md) shutdown.

## Best practices

1. **Always check** `CancellationToken` in long loops (`check_interrupt`) so fibers can stop promptly.
2. **Join** or explicitly handle `FiberHandle` leaks — use [`testing`](../testing/README.md) leak assertions in tests.
3. **Avoid blocking** the runtime thread pool indefinitely; pair fibers with [`scheduling`](../scheduling/README.md) and I/O policies.
4. **Prefer structured scopes** ([`resource::Scope`](../resource/README.md)) when lifetimes tie to nested work.

## See also

- [`SPEC.md`](../../SPEC.md) §Stratum 7.
- [`runtime`](../runtime/README.md) — `run_fork`, re-exports of fiber types.
- [`failure`](../failure/README.md) — `Exit` / `Cause` for fiber outcomes.
