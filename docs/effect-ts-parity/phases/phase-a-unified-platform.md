# Phase A — Unified platform layer (`@effect/platform` parity)

**Slug prefix:** `iep-a-*`  
**Effect.ts reference:** [`@effect/platform`](https://effect.website/docs/platform/introduction) and platform-* runtimes (Node, Browser, Bun).  
**Goal:** Provide **trait-based services** in `R` for HTTP, filesystem, process, and related capabilities, with **Tokio-first** implementations—so application code depends on **capabilities**, not on `reqwest` / `std::fs` directly at the type level.

## Executive summary

Today, `id_effect` integrates at the edges via **`id_effect_reqwest`**, **`id_effect_axum`**, **`id_effect_tokio`**, etc. Effect.ts instead centralizes **cross-cutting platform contracts** in `@effect/platform` and swaps implementations per host.

Phase A introduces a **cohesive `id_effect_platform` (name TBD)** story:

1. **Service traits** (or small trait groups) for `HttpClient`, `FileSystem`, `Path`/`FilePath` helpers, `Command`/`Process`, and optionally `Terminal` / `Clipboard` (lower priority).
2. **Tagged services** in `R` consistent with existing `Tag` / `Layer` patterns.
3. **Reference implementations** backed by Tokio + `reqwest` (HTTP), `tokio::fs` + `std::fs` where appropriate, `tokio::process`.
4. **Migration path** for existing crates: `id_effect_reqwest` becomes an implementation detail or thin adapter over the platform HTTP service.
5. **Documentation and examples** in the mdBook showing Axum handlers that only require platform traits in `R`.

## Non-goals (for Phase A)

- Replacing Axum or Tower; this phase defines **contracts** and **default wiring**, not a new web framework.
- Windows-specific parity for every API in the first cut (document gaps; use `cfg` stubs with clear errors if needed).
- Full MIME/multipart modeling on day one—ship a **minimal** HTTP subset, extend in follow-up tasks.

## Baseline inventory (repository)

| Existing crate / module | Role today |
|-------------------------|------------|
| `crates/id_effect_reqwest` | HTTP client calls |
| `crates/id_effect_axum` | HTTP server integration |
| `crates/id_effect_tokio` | Runtime bridge |
| `crates/id_effect_config` | Config + secrets |
| `crates/id_effect/src/observability/` | Tracing/metrics hooks (used by Phase B) |

## Architecture principles

1. **Errors:** Define small sealed or enum-style **platform error** types mappable from `std::io::Error`, HTTP status/body errors, etc. Consumers map into domain `E`.
2. **Streaming:** HTTP request/response bodies should interoperate with **`id_effect::Stream`** / chunks where feasible; fall back to “byte stream handle” trait objects if necessary for the first milestone.
3. **Determinism:** All traits must admit **test doubles** (in-memory FS, stub HTTP) without Tokio.
4. **Send/Sync:** Match existing `Effect` bounds; document `'static` constraints where dynamic dispatch is used.

---

## Three-level Beads task tree

**Shape:** **Level 1** = phase epic → **Level 2** = workstream (`--parent` epic) → **Level 3** = leaf tasks (`--parent` workstream). Use **`bd dep add <blocked> <blocker>`** only where order must be enforced across siblings.

### Level 1 — Phase epic

```bash
bd create "Phase A — Unified platform layer (@effect/platform parity)" -t epic -p 1 --json
# Record id as EPIC_A
```

### Level 2 — Workstreams (children of epic)

```bash
bd create "A — Design & crate skeleton" -t task -p 1 --parent EPIC_A --json
bd create "A — HTTP client service" -t feature -p 1 --parent EPIC_A --json
bd create "A — Filesystem service" -t feature -p 1 --parent EPIC_A --json
bd create "A — Process / command service" -t feature -p 2 --parent EPIC_A --json
bd create "A — Path & URI extras" -t task -p 3 --parent EPIC_A --json
bd create "A — CI, publish, migration" -t task -p 1 --parent EPIC_A --json
# Record ids as WS_A1 … WS_A6
```

### Level 3 — Leaf tasks (children of workstreams)

**Under `WS_A1` — Design & skeleton**

```bash
bd create "Slug iep-a-010 — Platform RFC (traits, errors, crate layout)" -t task -p 1 --parent WS_A1 --json
bd create "Slug iep-a-011 — Add id_effect_platform workspace crate skeleton" -t task -p 1 --parent WS_A1 --json
bd create "Slug iep-a-012 — PlatformError / HttpError / FsError + From bridges" -t feature -p 1 --parent WS_A1 --json
bd create "Slug iep-a-013 — mdBook stub: Platform services + glossary" -t chore -p 2 --parent WS_A1 --json
bd dep add <id-a011> <id-a010>
bd dep add <id-a012> <id-a011>
bd dep add <id-a013> <id-a010>
```

**Under `WS_A2` — HTTP client**

```bash
bd create "Slug iep-a-020 — HttpClient trait (req/res, timeouts, redirects)" -t feature -p 1 --parent WS_A2 --json
bd create "Slug iep-a-021 — Reqwest HttpClient + Layer" -t feature -p 1 --parent WS_A2 --json
bd create "Slug iep-a-022 — Streaming/chunked body → Stream/Chunk MVP" -t task -p 2 --parent WS_A2 --json
bd create "Slug iep-a-023 — HTTP integration tests (mock server)" -t task -p 2 --parent WS_A2 --json
bd create "Slug iep-a-024 — Reqwest→platform migration plan for examples" -t chore -p 3 --parent WS_A2 --json
bd dep add <id-a021> <id-a020>
bd dep add <id-a022> <id-a021>
bd dep add <id-a023> <id-a021>
bd dep add <id-a024> <id-a021>
```

**Under `WS_A2` — blocked by errors:** `bd dep add <id-a020> <id-a012>` (HTTP trait waits on error hierarchy).

**Under `WS_A3` — Filesystem**

```bash
bd create "Slug iep-a-030 — FileSystem trait (read/write/metadata/dir)" -t feature -p 1 --parent WS_A3 --json
bd create "Slug iep-a-031 — Tokio LiveFileSystem + Layer" -t feature -p 1 --parent WS_A3 --json
bd create "Slug iep-a-032 — In-memory TestFileSystem" -t feature -p 2 --parent WS_A3 --json
bd create "Slug iep-a-033 — FS security docs (traversal, symlinks)" -t chore -p 2 --parent WS_A3 --json
bd dep add <id-a031> <id-a030>
bd dep add <id-a032> <id-a030>
bd dep add <id-a033> <id-a030>
bd dep add <id-a030> <id-a012>
```

**Under `WS_A4` — Process**

```bash
bd create "Slug iep-a-040 — Process/Command trait" -t feature -p 2 --parent WS_A4 --json
bd create "Slug iep-a-041 — Tokio process impl + cancellation" -t feature -p 2 --parent WS_A4 --json
bd create "Slug iep-a-042 — Process CI tests (echo/cat)" -t task -p 2 --parent WS_A4 --json
bd dep add <id-a041> <id-a040>
bd dep add <id-a042> <id-a041>
bd dep add <id-a040> <id-a012>
```

**Under `WS_A5` — Path & URI (optional)**

```bash
bd create "Slug iep-a-050 — Normalize Utf8Path vs Path in public API" -t task -p 3 --parent WS_A5 --json
bd create "Slug iep-a-051 — Optional URI builder (feature-gated)" -t chore -p 4 --parent WS_A5 --json
bd dep add <id-a050> <id-a030>
bd dep add <id-a051> <id-a020>
```

**Under `WS_A6` — CI & migration**

```bash
bd create "Slug iep-a-060 — CI matrix for id_effect_platform" -t chore -p 1 --parent WS_A6 --json
bd create "Slug iep-a-061 — crates.io metadata + README badges" -t chore -p 3 --parent WS_A6 --json
bd create "Slug iep-a-062 — Migrate one official example to platform-only R" -t task -p 2 --parent WS_A6 --json
bd dep add <id-a061> <id-a060>
bd dep add <id-a062> <id-a060>
bd dep add <id-a060> <id-a021>
bd dep add <id-a060> <id-a031>
```

Replace `EPIC_A`, `WS_A*`, and `<id-*>` with real `bd-*` ids from `--json` output after each create.

---

## Work breakdown (Beads-ready)

Each subsection is one **Beads issue** candidate. Copy the title line to `bd create "…"`. Put the **Slug** and **Acceptance** into `--description` or a body file.

### Epic umbrella

| Slug | Suggested title | Type | P |
|------|-----------------|------|---|
| `iep-a-000` | Phase A — Unified platform layer (epic) | epic | 1 |

---

### A1 — Design & crate skeleton

| Slug | Suggested title | Type | P | Blocked by |
|------|-----------------|------|---|------------|
| `iep-a-010` | Platform phase — design RFC (traits, error taxonomy, crate layout) | task | 1 | — |
| `iep-a-011` | Add workspace member crate `id_effect_platform` (empty lib, docs) | task | 1 | `iep-a-010` |
| `iep-a-012` | Define `PlatformError` / `HttpError` / `FsError` hierarchy and `From` bridges | feature | 1 | `iep-a-011` |
| `iep-a-013` | Book stub chapter “Platform services” + glossary entries | chore | 2 | `iep-a-010` |

**Acceptance (`iep-a-010`):** RFC merged in repo (`docs/effect-ts-parity/rfcs/` or `crates/id_effect_platform/README.md`) covering: trait list, naming, dependency surface (single build including HTTP/FS/process/URI), MSRV implications, and relation to `id_effect_reqwest`.

**Acceptance (`iep-a-011`):** `cargo check -p id_effect_platform` passes in workspace; README lists intended modules.

---

### A2 — HTTP client service

| Slug | Suggested title | Type | P | Blocked by |
|------|-----------------|------|---|------------|
| `iep-a-020` | `HttpClient` trait: request/response types, timeouts, redirect policy hooks | feature | 1 | `iep-a-012` |
| `iep-a-021` | Reqwest-backed `HttpClient` implementation + `Layer` constructor | feature | 1 | `iep-a-020` |
| `iep-a-022` | Map streaming/chunked responses into `Stream`/`Chunk` (MVP: bounded body buffer) | task | 2 | `iep-a-021` |
| `iep-a-023` | Integration tests: mock server (e.g. `wiremock` or local `hyper`) + effect graph | task | 2 | `iep-a-021` |
| `iep-a-024` | Deprecation/shim plan for direct `id_effect_reqwest` usage in examples | chore | 3 | `iep-a-021` |

**Acceptance (`iep-a-020`):** At least GET/POST with headers + body; explicit error channel; no hidden global client.

---

### A3 — Filesystem service

| Slug | Suggested title | Type | P | Blocked by |
|------|-----------------|------|---|------------|
| `iep-a-030` | `FileSystem` trait: read/write/append, metadata, create_dir, remove | feature | 1 | `iep-a-012` |
| `iep-a-031` | Tokio-backed `LiveFileSystem` + `Layer` | feature | 1 | `iep-a-030` |
| `iep-a-032` | In-memory `TestFileSystem` for deterministic tests | feature | 2 | `iep-a-030` |
| `iep-a-033` | Security guidelines (path traversal, symlink policy) in module docs | chore | 2 | `iep-a-030` |

---

### A4 — Process / command service

| Slug | Suggested title | Type | P | Blocked by |
|------|-----------------|------|---|------------|
| `iep-a-040` | `Process` / `Command` trait: spawn, stdin/stdout/stderr, exit status | feature | 2 | `iep-a-012` |
| `iep-a-041` | Tokio-backed implementation + cancellation integration | feature | 2 | `iep-a-040` |
| `iep-a-042` | Tests: echo binary / cat pattern under CI | task | 2 | `iep-a-041` |

---

### A5 — Path & URI helpers (optional but high leverage)

| Slug | Suggested title | Type | P | Blocked by |
|------|-----------------|------|---|------------|
| `iep-a-050` | Normalize path types (`Utf8Path` vs `Path`) in public API | task | 3 | `iep-a-030` |
| `iep-a-051` | URI builder helper for HTTP client (optional crate feature) | chore | 4 | `iep-a-020` |

---

### A6 — Release & migration

| Slug | Suggested title | Type | P | Blocked by |
|------|-----------------|------|---|------------|
| `iep-a-060` | `moon run :check` / CI matrix includes new crate + features | chore | 1 | `iep-a-021`, `iep-a-031` |
| `iep-a-061` | Publish checklist: crates.io metadata, README badges | chore | 3 | `iep-a-060` |
| `iep-a-062` | Migrate one official example to platform traits only | task | 2 | `iep-a-021`, `iep-a-031` |

---

## Dependency graph (within Phase A)

```text
iep-a-010 → iep-a-011 → iep-a-012 → iep-a-020 → iep-a-021 → iep-a-022 / iep-a-023
                              └──→ iep-a-030 → iep-a-031 → iep-a-032
                              └──→ iep-a-040 → iep-a-041 → iep-a-042
iep-a-060 depends on iep-a-021 + iep-a-031 (task-level)
```

---

## Risks

| Risk | Mitigation |
|------|------------|
| Trait object `Send` pitfalls | Prefer generic associated types or small `impl Trait` constructors per call site where needed. |
| Duplication with `id_effect_reqwest` | Make reqwest a **private** dependency of the HTTP live impl first; public re-export only where necessary. |
| Scope creep (full HTTP server abstraction) | Defer “server platform” to Axum crate; Phase A focuses on **client + OS** primitives. |

---

## Beads import recipe

1. `bd create "Phase A — Unified platform layer (Effect @effect/platform parity)" -t epic -p 1 --json` → save **`EPIC_A`**.

2. Create children (examples — repeat for each row in work breakdown):

```bash
bd create "Platform phase — design RFC (traits, errors, crate layout)" -t task -p 1 --parent EPIC_A --json
bd create "Add crate id_effect_platform (skeleton)" -t task -p 1 --parent EPIC_A --json
# …
```

3. After each create, record `bd-*` ↔ slug in the issue description (`Slug: iep-a-011`).

4. Blocking edges (Beads: **`bd dep add blocked blocker`**):

```bash
# Example: skeleton blocked until RFC task closed
bd dep add EPIC_A.2 EPIC_A.1

# Example: error types blocked until skeleton exists
bd dep add EPIC_A.3 EPIC_A.2
```

Mirror the **graph** in the section “Dependency graph (within Phase A)”.

5. Optional **strict gate** from Phase Dependencies doc:

```bash
bd dep add EPIC_B EPIC_A   # only if you want entire Phase B epic blocked on Phase A epic
```

Prefer **finer-grained** deps in practice.
