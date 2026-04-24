# Phase I — Ongoing API parity & release hygiene

**Slug prefix:** `iep-i-*`  
**Effect.ts reference:** Core `effect` package release notes, `Stream`/`Schedule`/`Schema` churn.  
**Goal:** Make **parity maintenance** a **first-class, recurring** process: tracked issues, checklists, and automation so `id_effect` does not silently drift from Effect.ts idioms users expect.

## Executive summary

Phase I is not a one-shot project—it is a **program** with:

1. **Version cadence:** after each upstream Effect major/minor, run a structured diff checklist (API names, semantics, docs).
2. **Lint alignment:** extend `id_effect_lint` where Effect.ts ergonomics should be enforced in Rust.
3. **Book sync:** ensure mdBook chapters mention new Rust-side equivalents when phases A–H add crates.
4. **Changelog discipline:** user-visible parity called out in release notes.

---

## Three-level Beads task tree

### Level 1 — Epic

```bash
bd create "Phase I — Effect.ts parity maintenance (ongoing program)" -t epic -p 2 --json
# EPIC_I
```

### Level 2 — Workstreams

```bash
bd create "I — Checklist & release ritual" -t task -p 2 --parent EPIC_I --json
bd create "I — id_effect_lint parity" -t task -p 3 --parent EPIC_I --json
bd create "I — Book & glossary continuity" -t chore -p 3 --parent EPIC_I --json
bd create "I — Automation (CI, pins)" -t chore -p 4 --parent EPIC_I --json
# WS_I1 WS_I2 WS_I3 WS_I4
```

### Level 3 — Leaves

**`WS_I1`**

```bash
bd create "Slug iep-i-010 — CHECKLIST-upstream-effect.md rows" -t task -p 2 --parent WS_I1 --json
bd create "Slug iep-i-011 — Release ritual + changelog links" -t task -p 2 --parent WS_I1 --json
bd create "Slug iep-i-012 — justfile/script: checklist + upstream URLs" -t chore -p 3 --parent WS_I1 --json
bd dep add <i011> <i010>
bd dep add <i012> <i011>
```

**`WS_I2`**

```bash
bd create "Slug iep-i-020 — Audit id_effect_lint vs Effect idioms" -t task -p 3 --parent WS_I2 --json
bd create "Slug iep-i-021 — Extend lints for platform/OTEL crates" -t task -p 3 --parent WS_I2 --json
bd create "Slug iep-i-022 — Book: false positives + allows" -t chore -p 3 --parent WS_I2 --json
bd dep add <i021> <i020>
bd dep add <i022> <i021>
```

**`WS_I3`**

```bash
bd create "Slug iep-i-030 — Glossary updates (A–H terms)" -t chore -p 3 --parent WS_I3 --json
bd create "Slug iep-i-031 — Book ↔ docs/effect-ts-parity links" -t chore -p 3 --parent WS_I3 --json
bd dep add <i031> <i010>
```

**`WS_I4`**

```bash
bd create "Slug iep-i-040 — CI link check for Effect docs" -t chore -p 4 --parent WS_I4 --json
bd create "Slug iep-i-041 — UPSTREAM-VERSION file discipline" -t chore -p 4 --parent WS_I4 --json
bd dep add <i040> <i010>
bd dep add <i041> <i011>
```

---

## Work breakdown

### I0 — Epic

| Slug | Suggested title | Type | P |
|------|-----------------|------|---|
| `iep-i-000` | Phase I — Effect.ts parity maintenance (epic) | epic | 2 |

### I1 — Process & templates

| Slug | Suggested title | Type | P | Blocked by |
|------|-----------------|------|---|------------|
| `iep-i-010` | Add `docs/effect-ts-parity/CHECKLIST-upstream-effect.md` with module-by-module rows | task | 2 | — |
| `iep-i-011` | Define release ritual: owner, timebox, links to Effect changelog | task | 2 | `iep-i-010` |
| `iep-i-012` | Script or `just` recipe: print checklist + open upstream diff URLs | chore | 3 | `iep-i-011` |

### I2 — Lint & static guidance

| Slug | Suggested title | Type | P | Blocked by |
|------|-----------------|------|---|------------|
| `iep-i-020` | Audit `id_effect_lint` rules vs Effect.ts anti-patterns doc | task | 3 | — |
| `iep-i-021` | Add/adjust lints for new platform/OTEL crates when they land | task | 3 | `iep-i-020` |
| `iep-i-022` | Document false positives + allow patterns in book | chore | 3 | `iep-i-021` |

### I3 — Documentation continuity

| Slug | Suggested title | Type | P | Blocked by |
|------|-----------------|------|---|------------|
| `iep-i-030` | Glossary updates when phases A–H introduce new terms | chore | 3 | — |
| `iep-i-031` | Cross-links from book to `docs/effect-ts-parity/` | chore | 3 | `iep-i-010` |

### I4 — Automation (optional)

| Slug | Suggested title | Type | P | Blocked by |
|------|-----------------|------|---|------------|
| `iep-i-040` | CI job: nightly or weekly link check for external Effect docs | chore | 4 | `iep-i-010` |
| `iep-i-041` | Track upstream version under `docs/effect-ts-parity/UPSTREAM-VERSION` | chore | 4 | `iep-i-011` |

---

## Dependency graph

```text
iep-i-010 → iep-i-011 → iep-i-012
iep-i-020 → iep-i-021 → iep-i-022
iep-i-010 → iep-i-031
iep-i-010 → iep-i-040
iep-i-011 → iep-i-041
```

---

## Beads import recipe

1. Create **`EPIC_I`** early (priority **2**) — this epic **never “finishes”**; instead, close **child milestones** and open new ones each release cycle.

2. Use **recurring tasks:** duplicate a closed “Release ritual” issue template each cycle (Beads may support templates; otherwise copy from `iep-i-011` description).

3. Prefer **labels** `parity`, `docs`, `lint`, `release`.

4. Link **Phase A–H epics** as **related** (not necessarily blocking) using Beads graph links if available in your `bd` version (`relates_to` style — consult `bd graph --help`).

---

## Success metrics (program-level)

| Metric | Target |
|--------|--------|
| Checklist completion | 100% rows reviewed before each **minor** `id_effect` release |
| Lint regressions | Zero new Clippy/`id_effect_lint` failures on `main` |
| Doc drift | No broken links in parity docs (CI `iep-i-040`) |
