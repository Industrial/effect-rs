# Phase F — Fiber supervision & restart policies

**Slug prefix:** `iep-f-*`  
**Effect.ts reference:** Supervisor patterns in Effect.ts (fiber scopes with restart strategies, retries on failure).  
**Goal:** Add **declarative supervision** for `FiberHandle` trees: policies that react to **`Exit`** / **`Cause`** (including defects and interruption) by **restarting**, **escalating**, or **terminating**—integrated with **`Scope`** and **`CancellationToken`**.

## Executive summary

`id_effect` already provides **fibers**, **interruption**, and **scopes**. Phase F closes the gap to Effect.ts-style **supervision trees**:

1. **`Supervisor` (name TBD)** type holding policy + child handles.
2. **Policies:** `restart`, `restart_with_limit`, `escalate`, `ignore` (exact set to be designed).
3. **Integration:** supervisor installs **finalizers** so child fibers are interrupted when the supervisor scope ends.
4. **Semantics doc:** how supervision interacts with `Cause::Interrupt` vs `Cause::Die` / failure.

## Non-goals (v1)

- Distributed supervision across machines (that is Phase G territory).
- Magic recovery from `panic!` inside `std` (document that unwinding may still abort depending on runtime).

## Implementation (shipped)

Core API lives in `crates/id_effect/src/concurrency/supervisor.rs` (`Supervisor`, `SupervisorPolicy`, `supervised`, `Supervisor::spawn`), with module tests following `TESTING.md` (nested `#[cfg(test)]` trees, BDD-style names, `TestClock` for backoff without flakes). User-facing narrative: mdBook **§9.5 Supervision** (`crates/id_effect/book/src/part3/ch09-05-supervision.md`). Optional lint **iep-f-031** remains a future chore.

---

## Three-level Beads task tree

### Level 1 — Epic

```bash
bd create "Phase F — Fiber supervision & restart policies" -t epic -p 2 --json
# EPIC_F
```

### Level 2 — Workstreams

```bash
bd create "F — Design & semantics (RFC)" -t task -p 2 --parent EPIC_F --json
bd create "F — Core Supervisor implementation" -t feature -p 2 --parent EPIC_F --json
bd create "F — Documentation & optional lint" -t task -p 3 --parent EPIC_F --json
# WS_F1 WS_F2 WS_F3
```

### Level 3 — Leaves

**`WS_F1`**

```bash
bd create "Slug iep-f-010 — Survey Effect.ts supervision vs Exit/Cause" -t task -p 2 --parent WS_F1 --json
bd create "Slug iep-f-011 — RFC: Supervisor + Scope" -t task -p 2 --parent WS_F1 --json
bd create "Slug iep-f-012 — Plan property tests for restarts/backoff" -t task -p 3 --parent WS_F1 --json
bd dep add <f011> <f010>
bd dep add <f012> <f011>
```

**`WS_F2`**

```bash
bd create "Slug iep-f-020 — SupervisorPolicy enum + docs" -t feature -p 2 --parent WS_F2 --json
bd create "Slug iep-f-021 — Supervisor spawn + child attach" -t feature -p 2 --parent WS_F2 --json
bd create "Slug iep-f-022 — Restart loop + Schedule backoff" -t feature -p 2 --parent WS_F2 --json
bd create "Slug iep-f-023 — Escalation aggregated Cause" -t feature -p 2 --parent WS_F2 --json
bd create "Slug iep-f-024 — Stress tests bounded restarts" -t task -p 2 --parent WS_F2 --json
bd dep add <f020> <f011>
bd dep add <f021> <f020>
bd dep add <f022> <f021>
bd dep add <f023> <f021>
bd dep add <f024> <f022>
```

**`WS_F3`**

```bash
bd create "Slug iep-f-030 — mdBook: supervision vs raw fork" -t task -p 3 --parent WS_F3 --json
bd create "Slug iep-f-031 — Optional lint: fork without scope" -t chore -p 4 --parent WS_F3 --json
bd dep add <f030> <f023>
bd dep add <f031> <f030>
```

Cross-phase (Phase G): `bd dep add <g020> <f021>` when filing workflow spike.

---

## Work breakdown

### F0 — Epic

| Slug | Suggested title | Type | P |
|------|-----------------|------|---|
| `iep-f-000` | Phase F — Fiber supervision (epic) | epic | 2 |

### F1 — Design & semantics

| Slug | Suggested title | Type | P | Blocked by |
|------|-----------------|------|---|------------|
| `iep-f-010` | Survey Effect.ts supervision semantics + map to `Exit`/`Cause` | task | 2 | — |
| `iep-f-011` | RFC: API shape (`Supervisor`, `SupervisorPolicy`, interaction with `Scope`) | task | 2 | `iep-f-010` |
| `iep-f-012` | Property/law tests plan for restart counters and backoff | task | 3 | `iep-f-011` |

### F2 — Core implementation (in `id_effect` crate)

| Slug | Suggested title | Type | P | Blocked by |
|------|-----------------|------|---|------------|
| `iep-f-020` | Implement `SupervisorPolicy` enum + docs | feature | 2 | `iep-f-011` |
| `iep-f-021` | Implement `Supervisor::spawn` / attach child fibers | feature | 2 | `iep-f-020` |
| `iep-f-022` | Restart loop with `Schedule` integration for backoff between restarts | feature | 2 | `iep-f-021` |
| `iep-f-023` | Escalation path: supervisor fails with aggregated `Cause` | feature | 2 | `iep-f-021` |
| `iep-f-024` | Stress tests: tight restart loops remain bounded | task | 2 | `iep-f-022` |

### F3 — Book & migration

| Slug | Suggested title | Type | P | Blocked by |
|------|-----------------|------|---|------------|
| `iep-f-030` | mdBook chapter: supervision vs raw `fork` | task | 3 | `iep-f-023` |
| `iep-f-031` | Lint rule (optional): warn on `fork` without scope in long-lived servers | chore | 4 | `iep-f-030` |

---

## Dependency graph

```text
iep-f-010 → iep-f-011 → iep-f-012
iep-f-011 → iep-f-020 → iep-f-021 → iep-f-022 → iep-f-024
                         └──→ iep-f-023 → iep-f-030 → iep-f-031
```

---

## Cross-phase notes

- **Phase G** should add Beads deps from its **workflow runner** tasks onto **`iep-f-021`** or epic **`EPIC_F`** per [PHASE-DEPENDENCIES.md](../PHASE-DEPENDENCIES.md).

---

## Beads import recipe

1. `bd create "Phase F — Fiber supervision" -t epic -p 2 --json` → **`EPIC_F`**.

2. Serialize **design** (`iep-f-010`–`012`) before implementation tasks.

3. **Implementation** can parallelize `iep-f-022` (backoff) vs `iep-f-023` (escalation) **after** `iep-f-021` lands only if interfaces are stable—otherwise keep linear.

4. Acceptance for **`iep-f-024`:** must run under CI without timing flakes (use `TestClock` where applicable).
