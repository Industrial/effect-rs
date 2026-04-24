# Phase C — SQL / database access (`@effect/sql` parity)

**Slug prefix:** `iep-c-*`  
**Effect.ts reference:** `@effect/sql` and driver packages (`@effect/sql-pg`, …).  
**Goal:** A composable **database client** abstraction that runs as **`Effect`**, uses **`Scope`** for connections/transactions, maps errors into typed `E`, and supports at least **one** production driver (PostgreSQL recommended).

## Executive summary

Applications today can use `sqlx` / `tokio-postgres` inside `from_async`. Phase C elevates DB access to a **first-class platform concern**:

1. **`id_effect_sql`** trait module: `Client`, `Transaction`, `Executor` (names illustrative), streaming rows.
2. **Driver crate** `id_effect_sql_pg` (or feature-gated module) using a single chosen stack.
3. **Layer** constructors: pool acquisition, health checks, graceful shutdown.
4. **Examples:** Axum handler + transaction + `Stream` of rows.

## Non-goals (first ship)

- Every database under the sun—**one** driver to prove the model.
- ORM (Drizzle/Kysely-style) bridges—document “manual mapping” pattern; revisit later.
- Migration runner inside the library (users may keep `refinery` / `sqlx migrate` at the edge).

## Design principles

1. **Resource safety:** Connections and transactions must finalize via `Scope` / `acquire_release` patterns already in `id_effect`.
2. **Error channel:** Map driver errors into `SqlError` and let callers lift into domain `E` via `map_err` / `Effect::map_err`.
3. **Cancellation:** Long-running queries respect `CancellationToken` where the driver API allows (document limitations).
4. **Testing:** In-memory or testcontainers-backed integration tests in CI (feature-gated if heavy).

---

## Three-level Beads task tree

### Level 1 — Epic

```bash
bd create "Phase C — SQL platform (@effect/sql parity)" -t epic -p 1 --json
# EPIC_C
```

### Level 2 — Workstreams

```bash
bd create "C — Design & ADR" -t task -p 1 --parent EPIC_C --json
bd create "C — id_effect_sql core traits" -t feature -p 1 --parent EPIC_C --json
bd create "C — PostgreSQL driver (id_effect_sql_pg)" -t feature -p 1 --parent EPIC_C --json
bd create "C — Documentation & examples" -t task -p 2 --parent EPIC_C --json
# WS_C1 … WS_C4
```

### Level 3 — Leaves

**`WS_C1` — Design**

```bash
bd create "Slug iep-c-010 — ADR: sqlx vs tokio-postgres vs deadpool" -t task -p 1 --parent WS_C1 --json
bd create "Slug iep-c-011 — RFC: Client/Transaction/Stream rows" -t task -p 1 --parent WS_C1 --json
bd create "Slug iep-c-012 — Transaction nesting / savepoint semantics" -t task -p 2 --parent WS_C1 --json
bd dep add <c011> <c010>
bd dep add <c012> <c011>
```

**`WS_C2` — Core traits**

```bash
bd create "Slug iep-c-020 — Workspace crate id_effect_sql (traits)" -t task -p 1 --parent WS_C2 --json
bd create "Slug iep-c-021 — Client trait" -t feature -p 1 --parent WS_C2 --json
bd create "Slug iep-c-022 — Transaction trait" -t feature -p 1 --parent WS_C2 --json
bd create "Slug iep-c-023 — Row streaming → Stream MVP" -t feature -p 2 --parent WS_C2 --json
bd dep add <c020> <c011>
bd dep add <c021> <c020>
bd dep add <c022> <c021>
bd dep add <c023> <c021>
```

**`WS_C3` — Postgres**

```bash
bd create "Slug iep-c-030 — id_effect_sql_pg pool + Layer" -t feature -p 1 --parent WS_C3 --json
bd create "Slug iep-c-031 — Config + Secret redaction for DSN" -t task -p 2 --parent WS_C3 --json
bd create "Slug iep-c-032 — Integration tests (docker/testcontainers)" -t task -p 2 --parent WS_C3 --json
bd create "Slug iep-c-033 — Pool acquire timeout observability" -t task -p 3 --parent WS_C3 --json
bd dep add <c030> <c022>
bd dep add <c031> <c030>
bd dep add <c032> <c030>
bd dep add <c033> <c030>
```

**`WS_C4` — Docs**

```bash
bd create "Slug iep-c-040 — mdBook: database access with effects" -t task -p 2 --parent WS_C4 --json
bd create "Slug iep-c-041 — Example/snippet source for book" -t chore -p 3 --parent WS_C4 --json
bd dep add <c040> <c032>
bd dep add <c041> <c040>
```

---

## Work breakdown

### C0 — Epic

| Slug | Suggested title | Type | P |
|------|-----------------|------|---|
| `iep-c-000` | Phase C — SQL platform layer (epic) | epic | 1 |

### C1 — Design

| Slug | Suggested title | Type | P | Blocked by |
|------|-----------------|------|---|------------|
| `iep-c-010` | Survey Rust ecosystem: `sqlx` vs `tokio-postgres` vs `deadpool` — decision record | task | 1 | — |
| `iep-c-011` | RFC: `id_effect_sql` traits, error types, streaming row API | task | 1 | `iep-c-010` |
| `iep-c-012` | Define transaction nesting / savepoint semantics (or explicit non-support) | task | 2 | `iep-c-011` |

### C2 — Core crate (driver-agnostic)

| Slug | Suggested title | Type | P | Blocked by |
|------|-----------------|------|---|------------|
| `iep-c-020` | Add `id_effect_sql` workspace member (traits only + docs) | task | 1 | `iep-c-011` |
| `iep-c-021` | `Client` trait: query, execute, parameterized API | feature | 1 | `iep-c-020` |
| `iep-c-022` | `Transaction` trait: commit/rollback as `Effect` | feature | 1 | `iep-c-021` |
| `iep-c-023` | Row streaming: map to `Stream` of typed rows (MVP: `Vec<u8>` / `String` cells) | feature | 2 | `iep-c-021` |

### C3 — PostgreSQL implementation

| Slug | Suggested title | Type | P | Blocked by |
|------|-----------------|------|---|------------|
| `iep-c-030` | `id_effect_sql_pg`: connect + pool + `Layer` | feature | 1 | `iep-c-022` |
| `iep-c-031` | Connection string from `id_effect_config` + `Secret` redaction in logs | task | 2 | `iep-c-030` |
| `iep-c-032` | Integration tests (docker or `testcontainers`) | task | 2 | `iep-c-030` |
| `iep-c-033` | Pool sizing / acquire timeout observability hooks | task | 3 | `iep-c-030` |

### C4 — Documentation & examples

| Slug | Suggested title | Type | P | Blocked by |
|------|-----------------|------|---|------------|
| `iep-c-040` | mdBook chapter: database access with effects | task | 2 | `iep-c-032` |
| `iep-c-041` | Example binary or integration test used as snippet source | chore | 3 | `iep-c-040` |

---

## Dependency graph (within Phase C)

```text
iep-c-010 → iep-c-011 → iep-c-012
iep-c-011 → iep-c-020 → iep-c-021 → iep-c-022 → iep-c-030 → iep-c-031 / iep-c-032 / iep-c-033
iep-c-032 → iep-c-040 → iep-c-041
```

---

## Cross-phase notes

- **Phase D (RPC)** book examples should depend on **C** for realistic demos — add Beads `bd dep add` from selected D tasks to `iep-c-040` or `iep-c-030`.

---

## Beads import recipe

1. `bd create "Phase C — SQL platform (@effect/sql parity)" -t epic -p 1 --json` → **`EPIC_C`**.

2. Linearize **design** tasks first (`iep-c-010` → `iep-c-011` → `iep-c-012`) as children with `bd dep add` chain.

3. **Parallel:** after `iep-c-020`, trait work (`iep-c-021`–`023`) can split across contributors—avoid epic-level blocking unless necessary.

4. Mark **driver** milestone `iep-c-030` as **feature** with acceptance: CI green on default features; optional job for integration tests.

5. For each issue, include **connection string handling** security checklist in acceptance criteria.
