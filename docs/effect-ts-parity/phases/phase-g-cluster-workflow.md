# Phase G — Cluster & durable workflow (research → product)

**Slug prefix:** `iep-g-*`  
**Effect.ts reference:** `@effect/cluster`, `@effect/workflow` — distributed execution, durable steps, sagas.  
**Goal:** Time-box **research** and optionally deliver a **minimal durable execution** slice appropriate for Rust backends—**without** committing prematurely to full distributed cluster semantics.

## Executive summary

This phase is **high risk / high scope**. Structure it as:

1. **G0 — Research epic:** decision record comparing external systems (Temporal, custom saga store, message queues) vs in-process replay.
2. **G1 — Spike:** smallest “durable step log” prototype using `Effect` + persistent storage (SQLite/Postgres).
3. **G2 — Product (optional):** harden spike into a crate **or** document **recommended external integration** and close the epic.

## Non-goals (default)

- Building a full competitor to Temporal/Cadence inside `id_effect`.
- Peer-to-peer cluster membership protocol.

---

## Three-level Beads task tree

### Level 1 — Epic

```bash
bd create "Phase G — Cluster / durable workflow (research → product)" -t epic -p 4 --json
# EPIC_G
```

### Level 2 — Workstreams

```bash
bd create "G — Charter & ADR (research)" -t task -p 4 --parent EPIC_G --json
bd create "G — Spike: durable step log" -t feature -p 4 --parent EPIC_G --json
bd create "G — Hardening (product track, optional)" -t feature -p 3 --parent EPIC_G --json
# WS_G1 WS_G2 WS_G3
```

### Level 3 — Leaves

**`WS_G1` — Research**

```bash
bd create "Slug iep-g-010 — Charter: success criteria for workflow" -t task -p 4 --parent WS_G1 --json
bd create "Slug iep-g-011 — ADR: Temporal vs saga vs out-of-scope" -t task -p 4 --parent WS_G1 --json
bd create "Slug iep-g-012 — Security: idempotency, PII, replay" -t task -p 4 --parent WS_G1 --json
bd dep add <g011> <g010>
bd dep add <g012> <g011>
```

**`WS_G2` — Spike**

```bash
bd create "Slug iep-g-020 — Spike crate: append-only log + resume" -t feature -p 4 --parent WS_G2 --json
bd create "Slug iep-g-021 — Demo: restart resumes workflow" -t task -p 4 --parent WS_G2 --json
bd create "Slug iep-g-022 — Retrospective keep/delete/externalize" -t task -p 4 --parent WS_G2 --json
bd dep add <g020> <g011>
bd dep add <g021> <g020>
bd dep add <g022> <g021>
```

**`WS_G3` — Hardening**

```bash
bd create "Slug iep-g-030 — Stable API + semver from spike" -t feature -p 3 --parent WS_G3 --json
bd create "Slug iep-g-031 — Load tests + crash injection" -t task -p 3 --parent WS_G3 --json
bd create "Slug iep-g-032 — mdBook: vs external orchestrator" -t task -p 3 --parent WS_G3 --json
bd dep add <g030> <g022>
bd dep add <g031> <g030>
bd dep add <g032> <g030>
```

Cross-phase: `bd dep add <g020> <f021>` (Supervisor spawn ready).

---

## Work breakdown

### G0 — Epic & charter

| Slug | Suggested title | Type | P |
|------|-----------------|------|---|
| `iep-g-000` | Phase G — Cluster / durable workflow (epic) | epic | 4 |

### G1 — Research

| Slug | Suggested title | Type | P | Blocked by |
|------|-----------------|------|---|------------|
| `iep-g-010` | Charter: success criteria for “workflow” in this repo | task | 4 | — |
| `iep-g-011` | ADR: compare Temporal SDK, custom saga table, and “out of scope” | task | 4 | `iep-g-010` |
| `iep-g-012` | Security review: idempotency keys, PII in logs, replay safety | task | 4 | `iep-g-011` |

### G2 — Spike

| Slug | Suggested title | Type | P | Blocked by |
|------|-----------------|------|---|------------|
| `iep-g-020` | Spike crate (experimental): append-only step log + resume | feature | 4 | `iep-g-011` |
| `iep-g-021` | Demo: single-process restart resumes incomplete workflow | task | 4 | `iep-g-020` |
| `iep-g-022` | Spike retrospective: keep / delete / externalize | task | 4 | `iep-g-021` |

### G3 — Hardening (only if G2 says “keep”)

| Slug | Suggested title | Type | P | Blocked by |
|------|-----------------|------|---|------------|
| `iep-g-030` | Extract stable API from spike + semver policy | feature | 3 | `iep-g-022` |
| `iep-g-031` | Load tests + failure injection (crash mid-workflow) | task | 3 | `iep-g-030` |
| `iep-g-032` | mdBook chapter: when to use this vs external orchestrator | task | 3 | `iep-g-030` |

---

## Dependency graph

```text
iep-g-010 → iep-g-011 → iep-g-012
iep-g-011 → iep-g-020 → iep-g-021 → iep-g-022 → (optional) iep-g-030 → iep-g-031 / iep-g-032
```

**Cross-phase:** `iep-g-020` should be **blocked by** supervision milestone `iep-f-021` (or epic **F**) if restart semantics are required for the demo.

---

## Beads import recipe

1. Keep **`EPIC_G`** at **P4** until leadership promotes workflow work.

2. Use **Beads labels** `research`, `spike`, `workflow`.

3. **Close or split** the epic after `iep-g-022` if outcome is “external only” — do not leave an open-ended epic.

4. If promoting to product, re-prioritize tasks to **P2** and add CI ownership tasks.
