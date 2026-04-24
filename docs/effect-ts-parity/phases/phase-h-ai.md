# Phase H — AI & LLM clients (`@effect/ai` parity)

**Slug prefix:** `iep-h-*`  
**Effect.ts reference:** `@effect/ai` and vendor packages (OpenAI, Anthropic, …).  
**Goal:** Optional **`id_effect_ai`** (or split crates) providing **streaming completions** as **`Stream`**, unified **errors**, **`Schedule`**-based retries, **`Secret`** for API keys via `id_effect_config`, and **tracing spans** per request (ties to Phase B).

## Executive summary

AI APIs move quickly; this phase optimizes for **composition** with existing `id_effect` primitives rather than locking to one vendor SDK forever.

1. **Core traits:** `LanguageModel`, `ChatCompletionRequest`, streaming token chunks.
2. **Vendor modules:** behind feature flags (`openai`, `anthropic`, …).
3. **HTTP:** use **Phase A** `HttpClient` service where available; fallback documented for early releases.
4. **Examples:** small “ask once” CLI using Phase E patterns.

## Non-goals

- Shipping every vendor Effect.ts supports on day one.
- Tool-calling / function-calling **framework**—document manual JSON schema handling first.

---

## Three-level Beads task tree

### Level 1 — Epic

```bash
bd create "Phase H — AI client abstractions (@effect/ai parity)" -t epic -p 4 --json
# EPIC_H
```

### Level 2 — Workstreams

```bash
bd create "H — RFC & security" -t task -p 4 --parent EPIC_H --json
bd create "H — id_effect_ai core traits" -t feature -p 4 --parent EPIC_H --json
bd create "H — OpenAI-compatible vendor (first)" -t feature -p 4 --parent EPIC_H --json
# WS_H1 WS_H2 WS_H3
```

### Level 3 — Leaves

**`WS_H1`**

```bash
bd create "Slug iep-h-010 — RFC: naming, features, MSRV, deps" -t task -p 4 --parent WS_H1 --json
bd create "Slug iep-h-011 — Threat model: secrets, injection, logs" -t task -p 4 --parent WS_H1 --json
bd dep add <h011> <h010>
```

**`WS_H2`**

```bash
bd create "Slug iep-h-020 — id_effect_ai skeleton + traits" -t task -p 4 --parent WS_H2 --json
bd create "Slug iep-h-021 — Streaming completions Stream/Chunk" -t feature -p 4 --parent WS_H2 --json
bd create "Slug iep-h-022 — Schedule retries for transient HTTP" -t task -p 4 --parent WS_H2 --json
bd dep add <h020> <h010>
bd dep add <h021> <h020>
bd dep add <h022> <h021>
```

**`WS_H3`**

```bash
bd create "Slug iep-h-030 — OpenAI-compatible chat (feature)" -t feature -p 4 --parent WS_H3 --json
bd create "Slug iep-h-031 — Mock server tests (no live keys)" -t task -p 4 --parent WS_H3 --json
bd create "Slug iep-h-032 — Example + book appendix" -t chore -p 4 --parent WS_H3 --json
bd dep add <h030> <h022>
bd dep add <h031> <h030>
bd dep add <h032> <h031>
```

Cross-phase: `bd dep add <h030> <id-reqwest-http-impl>` or Phase A `HttpClient` leaf when available.

---

## Work breakdown

### H0 — Epic

| Slug | Suggested title | Type | P |
|------|-----------------|------|---|
| `iep-h-000` | Phase H — AI client abstractions (epic) | epic | 4 |

### H1 — Design

| Slug | Suggested title | Type | P | Blocked by |
|------|-----------------|------|---|------------|
| `iep-h-010` | RFC: crate naming, feature flags, MSRV, dependency budget | task | 4 | — |
| `iep-h-011` | Threat model: secrets, prompt injection logging redaction | task | 4 | `iep-h-010` |

### H2 — Core traits (vendor-agnostic)

| Slug | Suggested title | Type | P | Blocked by |
|------|-----------------|------|---|------------|
| `iep-h-020` | Add `id_effect_ai` skeleton + traits | task | 4 | `iep-h-010` |
| `iep-h-021` | Streaming response type using `Stream` / `Chunk` | feature | 4 | `iep-h-020` |
| `iep-h-022` | Retry policy via `Schedule` for transient HTTP failures | task | 4 | `iep-h-021` |

### H3 — First vendor (OpenAI illustrative)

| Slug | Suggested title | Type | P | Blocked by |
|------|-----------------|------|---|------------|
| `iep-h-030` | OpenAI-compatible chat completion (behind feature) | feature | 4 | `iep-h-022` |
| `iep-h-031` | Mock server tests (no real API keys in CI) | task | 4 | `iep-h-030` |
| `iep-h-032` | Example + book appendix | chore | 4 | `iep-h-031` |

---

## Dependency graph

```text
iep-h-010 → iep-h-011
iep-h-010 → iep-h-020 → iep-h-021 → iep-h-022 → iep-h-030 → iep-h-031 → iep-h-032
```

**Cross-phase:** `iep-h-030` should be **blocked by** Phase A HTTP client tasks if traits are required; `iep-h-031` benefits from Phase B for span assertions in tests.

---

## Beads import recipe

1. Epic **`EPIC_H`** at **P4** until AI becomes a roadmap pillar.

2. Require **`iep-h-011`** (security) before any vendor implementation merges.

3. Use **`discovered-from`** deps when spikes uncover more vendors:

```bash
bd create "Add Anthropic backend" -t feature -p 4 --deps discovered-from:EPIC_H.5 --json
```

4. Keep **network tests** behind explicit CI job to avoid flakes.
