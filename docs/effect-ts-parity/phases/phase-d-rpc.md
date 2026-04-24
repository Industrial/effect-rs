# Phase D — RPC & typed service contracts (`@effect/rpc` parity)

**Slug prefix:** `iep-d-*`  
**Effect.ts reference:** `@effect/rpc` — schemas, request/response contracts, protocol handlers.  
**Goal:** Document and optionally implement **RPC-style** communication where **`Effect`**, **`R`**, and **errors** cross process boundaries in a disciplined way—starting with **patterns** on top of existing Rust stacks, evolving toward **codegen** if justified.

## Executive summary

Effect.ts RPC ties **Schema**, **Layer**, and **platform HTTP** together. In Rust, the nearest ecosystem is **`tonic` + `prost`**, **`tarpc`**, or custom HTTP+JSON. Phase D is intentionally **staged**:

1. **Stage D1 — Patterns (thin):** Official mdBook guidance for `tonic` / HTTP JSON handlers that preserve `Effect` error types at the edge, propagate tracing (after Phase B), and use **Phase A** HTTP traits where applicable.
2. **Stage D2 — Thin crate:** `id_effect_rpc` helpers: error envelope serialization, correlation ids, middleware hooks—**no** full proc-macro codegen initially.
3. **Stage D3 — Codegen (optional epic):** Proc-macros or build.rs for service definitions—only after D2 proves the operational model.

---

## Three-level Beads task tree

### Level 1 — Epic

```bash
bd create "Phase D — RPC & service contracts (@effect/rpc parity)" -t epic -p 2 --json
# EPIC_D
```

### Level 2 — Workstreams

```bash
bd create "D — Research & thin patterns (book)" -t task -p 2 --parent EPIC_D --json
bd create "D — id_effect_rpc helper crate" -t feature -p 2 --parent EPIC_D --json
bd create "D — Codegen spike (optional)" -t task -p 4 --parent EPIC_D --json
# WS_D1 WS_D2 WS_D3
```

### Level 3 — Leaves

**`WS_D1`**

```bash
bd create "Slug iep-d-010 — Compare tonic / tarpc / HTTP+JSON" -t task -p 2 --parent WS_D1 --json
bd create "Slug iep-d-011 — mdBook: RPC boundaries with id_effect" -t task -p 2 --parent WS_D1 --json
bd create "Slug iep-d-012 — Example Axum JSON + Schema boundary" -t task -p 3 --parent WS_D1 --json
bd dep add <d011> <d010>
bd dep add <d012> <d011>
```

**`WS_D2`**

```bash
bd create "Slug iep-d-020 — id_effect_rpc skeleton" -t task -p 2 --parent WS_D2 --json
bd create "Slug iep-d-021 — RpcError + Axum IntoResponse" -t feature -p 2 --parent WS_D2 --json
bd create "Slug iep-d-022 — Tracing helpers for RPC" -t task -p 2 --parent WS_D2 --json
bd create "Slug iep-d-023 — Tests: error round-trip" -t task -p 2 --parent WS_D2 --json
bd dep add <d020> <d011>
bd dep add <d021> <d020>
bd dep add <d022> <d021>
bd dep add <d023> <d021>
```

**`WS_D3`**

```bash
bd create "Slug iep-d-030 — Spike: proc-macro or build.rs codegen" -t task -p 4 --parent WS_D3 --json
bd create "Slug iep-d-031 — Decision adopt/defer codegen" -t task -p 4 --parent WS_D3 --json
bd dep add <d030> <d023>
bd dep add <d031> <d030>
```

Optional cross-phase: `bd dep add <d022> <b021>` (RPC tracing after span bridge). `bd dep add <d012> <c040>` if SQL example.

---

## Non-goals (until Stage D3)

- Replacing gRPC ecosystem choices.
- Full bidirectional streaming parity on day one—document limitations.

---

## Work breakdown

### D0 — Epic

| Slug | Suggested title | Type | P |
|------|-----------------|------|---|
| `iep-d-000` | Phase D — RPC & service contracts (epic) | epic | 2 |

### D1 — Research & thin patterns

| Slug | Suggested title | Type | P | Blocked by |
|------|-----------------|------|---|------------|
| `iep-d-010` | Compare `tonic`, `tarpc`, and plain HTTP+JSON for Effect-friendly boundaries | task | 2 | — |
| `iep-d-011` | mdBook chapter (thin): “RPC boundaries with id_effect” | task | 2 | `iep-d-010` |
| `iep-d-012` | Example: Axum JSON API with shared `Schema` validation at boundary | task | 3 | `iep-d-011` |

### D2 — Helper crate

| Slug | Suggested title | Type | P | Blocked by |
|------|-----------------|------|---|------------|
| `iep-d-020` | Add `id_effect_rpc` skeleton crate (docs only) | task | 2 | `iep-d-011` |
| `iep-d-021` | Define `RpcError` envelope + `IntoResponse` / Axum bridge | feature | 2 | `iep-d-020` |
| `iep-d-022` | Tracing span helpers for request/response (integrate Phase B types) | task | 2 | `iep-d-021` |
| `iep-d-023` | Tests: round-trip error mapping | task | 2 | `iep-d-021` |

### D3 — Codegen spike (optional)

| Slug | Suggested title | Type | P | Blocked by |
|------|-----------------|------|---|------------|
| `iep-d-030` | Spike: proc-macro or `build.rs` service stub generation | task | 4 | `iep-d-023` |
| `iep-d-031` | Decision: adopt / defer codegen based on spike | task | 4 | `iep-d-030` |

---

## Dependency graph

```text
iep-d-010 → iep-d-011 → iep-d-012
iep-d-011 → iep-d-020 → iep-d-021 → iep-d-022 → iep-d-023 → iep-d-030 → iep-d-031
```

---

## Cross-phase notes

- Add Beads deps: **`iep-d-022` blocked by** Phase B tracing bridge tasks if sharing concrete types.
- **`iep-d-012`** blocked by Phase C **example DB** tasks if the example uses SQL.

---

## Beads import recipe

1. Create epic **`EPIC_D`** with priority `2` (starts after core infra matures).

2. Keep **Stage D3** tasks as **P4** backlog until D2 closes.

3. Use **labels** if supported (`bd create … -l rpc,docs`) for filtering.

4. For each task, paste **Effect.ts reference links** in the description for parity discussions during review.
