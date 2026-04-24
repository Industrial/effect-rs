# Phase E — CLI ergonomics (`@effect/cli` parity)

**Slug prefix:** `iep-e-*`  
**Effect.ts reference:** `@effect/cli` — declarative CLI argument models composed into `Effect` programs.  
**Goal:** Give users a **recommended** way to build CLIs whose main bodies are `Effect` values: parsing, config loading, logging init, and exit codes—without necessarily reimplementing `clap`.

## Executive summary

Rust’s **de facto** CLI stack is **`clap`**. Phase E should **embrace** it rather than reinvent parsing:

1. **Official pattern:** `clap` derive or builder → parse into a struct → `Effect::from_async` / `run_blocking` entrypoint with `R` assembled from layers.
2. **Optional `id_effect_cli` crate:** Thin wrappers: `run_cli(effect, env)`, stderr/stdout `Sink` adapters (future), consistent **exit code mapping** from `Exit` / `Cause`.
3. **Templates:** `examples/cli-template/` or cargo-generate style layout documented in the book.

## Non-goals

- Full parser-combinator CLI library mirroring every `@effect/cli` API.
- TUI widgets (defer to dedicated crates).

---

## Three-level Beads task tree

### Level 1 — Epic

```bash
bd create "Phase E — CLI ergonomics (@effect/cli parity)" -t epic -p 3 --json
# EPIC_E
```

### Level 2 — Workstreams

```bash
bd create "E — Documentation (mdBook + snippets)" -t task -p 3 --parent EPIC_E --json
bd create "E — id_effect_cli helper crate" -t feature -p 3 --parent EPIC_E --json
bd create "E — Examples & README links" -t chore -p 4 --parent EPIC_E --json
# WS_E1 WS_E2 WS_E3
```

### Level 3 — Leaves

**`WS_E1`**

```bash
bd create "Slug iep-e-010 — mdBook: CLI entrypoints with clap + Effect" -t task -p 3 --parent WS_E1 --json
bd create "Slug iep-e-011 — Doc: Exit → ExitCode mapping" -t task -p 3 --parent WS_E1 --json
bd create "Slug iep-e-012 — Snippet: Config + Secret flags" -t task -p 3 --parent WS_E1 --json
bd dep add <e011> <e010>
bd dep add <e012> <e010>
```

**`WS_E2`**

```bash
bd create "Slug iep-e-020 — id_effect_cli skeleton (clap feature)" -t task -p 3 --parent WS_E2 --json
bd create "Slug iep-e-021 — run_main helper" -t feature -p 3 --parent WS_E2 --json
bd create "Slug iep-e-022 — Integration test exit codes 0/1" -t task -p 3 --parent WS_E2 --json
bd dep add <e020> <e011>
bd dep add <e021> <e020>
bd dep add <e022> <e021>
```

**`WS_E3`**

```bash
bd create "Slug iep-e-030 — examples/cli-minimal" -t chore -p 4 --parent WS_E3 --json
bd create "Slug iep-e-031 — README cross-links" -t chore -p 4 --parent WS_E3 --json
bd dep add <e031> <e030>
bd dep add <e030> <e010>
```

---

## Work breakdown

### E0 — Epic

| Slug | Suggested title | Type | P |
|------|-----------------|------|---|
| `iep-e-000` | Phase E — CLI ergonomics (epic) | epic | 3 |

### E1 — Documentation-first

| Slug | Suggested title | Type | P | Blocked by |
|------|-----------------|------|---|------------|
| `iep-e-010` | mdBook chapter: “CLI entrypoints with Effect” using `clap` | task | 3 | — |
| `iep-e-011` | Document exit codes: `Exit` → `std::process::ExitCode` mapping table | task | 3 | `iep-e-010` |
| `iep-e-012` | Snippet: load `Config` + `Secret` flags via `id_effect_config` | task | 3 | `iep-e-010` |

### E2 — Optional helper crate

| Slug | Suggested title | Type | P | Blocked by |
|------|-----------------|------|---|------------|
| `iep-e-020` | Add `id_effect_cli` skeleton (feature: `clap`) | task | 3 | `iep-e-011` |
| `iep-e-021` | `run_main(effect)` helper: init tracing, run, map errors | feature | 3 | `iep-e-020` |
| `iep-e-022` | Integration test: binary exits 0/1 deterministically | task | 3 | `iep-e-021` |

### E3 — Developer templates

| Slug | Suggested title | Type | P | Blocked by |
|------|-----------------|------|---|------------|
| `iep-e-030` | Add `examples/cli-minimal/` mirroring book chapter | chore | 4 | `iep-e-010` |
| `iep-e-031` | README section linking example + book | chore | 4 | `iep-e-030` |

---

## Dependency graph

```text
iep-e-010 → iep-e-011 → iep-e-020 → iep-e-021 → iep-e-022
iep-e-010 → iep-e-012
iep-e-010 → iep-e-030 → iep-e-031
```

---

## Beads import recipe

1. Epic **`EPIC_E`** at priority `3` or `4` unless CLI is a product priority.

2. Most tasks are **independent**—minimize `bd dep add` to avoid starving the ready queue.

3. Label suggestions: `cli`, `docs`, `dx`.
