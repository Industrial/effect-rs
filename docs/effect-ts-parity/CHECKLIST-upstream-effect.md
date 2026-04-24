# Upstream Effect.ts parity checklist

**Purpose:** Run this checklist **each release cycle** (see Phase I, `iep-i-010`–`iep-i-011`). Record the **upstream Effect version** reviewed in [`UPSTREAM-VERSION`](./UPSTREAM-VERSION).

**How to use:** For each row, mark **Reviewed**, **Gap noted** (with link to Beads issue), or **N/A** for Rust. Keep links to the canonical Effect docs or source.

| Upstream area | Effect.ts / docs anchor | id_effect location / notes | Status |
|---------------|-------------------------|----------------------------|--------|
| Core `Effect` | `effect` package `Effect` | `crates/id_effect/src/kernel/effect.rs` | |
| `Layer` / `Context` | Services, layers | `crates/id_effect/src/layer/`, `context/` | |
| `Scope` / `Release` | Resource safety | `crates/id_effect/src/resource/scope.rs` | |
| `Schedule` / retry | Scheduling policies | `crates/id_effect/src/scheduling/schedule.rs` | |
| `Stream` / `Sink` | Streaming | `crates/id_effect/src/streaming/` | |
| `STM` | `STM` module | `crates/id_effect/src/stm/` | |
| `Schema` | Parse/encode | `crates/id_effect/src/schema/` | |
| `Fiber` / interruption | Concurrency | `crates/id_effect/src/concurrency/` | |
| `FiberRef` | Fiber-local state | `fiber_ref.rs` | |
| `Cause` / `Exit` | Failure | `crates/id_effect/src/failure/` | |
| `@effect/platform` | HTTP, FS, process | *Planned Phase A* — `docs/effect-ts-parity/phases/phase-a-unified-platform.md` | |
| `@effect/opentelemetry` | OTEL | **Phase B** — `crates/id_effect_opentelemetry` + mdBook `part2/ch07-12-opentelemetry.md` | |
| `@effect/sql` | Database | *Planned Phase C* | |
| `@effect/rpc` | RPC | *Planned Phase D* | |
| `@effect/cli` | CLI | *Planned Phase E* | |
| Supervision | Fiber supervision | `crates/id_effect/src/concurrency/supervisor.rs`, mdBook §9.5 | Reviewed |
| Cluster / workflow | Distributed | *Planned Phase G* | |
| `@effect/ai` | LLM clients | *Planned Phase H* | |

## Review log

| Date | Upstream version | Reviewer | Notes / link |
|------|------------------|----------|--------------|
| | | | |
