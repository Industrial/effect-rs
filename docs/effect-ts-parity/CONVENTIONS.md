# Conventions — slugs, priorities, Beads mapping

This file defines how **phase documents** under `docs/effect-ts-parity/` relate to **[Beads](https://github.com/gastownhall/beads)** (`bd`) issues.

## Prerequisites

- Install the `bd` CLI ([installation](https://github.com/gastownhall/beads/blob/main/docs/INSTALLING.md)).
- From the repository root:

```bash
bd init --quiet
# optional: bd hooks install
```

> **Note:** Beads stores state under `.beads/` (see repo `.gitignore`). Issue ids are **hash-based** (`bd-a1b2`, …) and are only known **after** creation. Phase docs therefore use **stable slugs** for design traceability; you map slugs ↔ `bd-*` in your tracker notes or in each issue’s description.

## Slug format

Pattern:

```text
iep-<phase>-<nnn>[-short-name]
```

| Part | Meaning |
|------|---------|
| `iep` | **i**d_**e**ffect **p**arity (namespace prefix) |
| `<phase>` | `a` … `i` matching phase files (`phase-a-…` through `phase-i-…`) |
| `<nnn>` | Three-digit sequence within the phase (sortable) |
| `-short-name` | Optional kebab-case hint (omit if redundant) |

Examples: `iep-a-010`, `iep-b-020`, `iep-c-015`.

## Issue types (Beads `-t`)

| Type | Use for |
|------|---------|
| `epic` | Whole phase or major multi-month program |
| `feature` | User-visible capability (new crate surface, major API) |
| `task` | Concrete engineering unit (module, integration, tests) |
| `chore` | Tooling, CI, docs-only, refactors without behavior change |
| `bug` | Incorrect behavior to fix (parity bugs vs Effect.ts semantics) |

## Priorities (Beads `-p`)

| Priority | Meaning |
|----------|---------|
| `0` | Critical — security, data loss, broken builds |
| `1` | High — primary roadmap deliverables |
| `2` | Medium — default depth work |
| `3` | Low — polish, optional stretch |
| `4` | Backlog — research, future ideas |

Phase epics are usually **`-p 1`**. Research spikes inside a phase are often **`-p 2`** or **`-p 3`**.

## Parent hierarchy (`--parent`)

Beads supports **child issues** attached to an epic:

```bash
bd create "Phase A: Unified platform layer" -t epic -p 1 --json   # note id EPIC_A
bd create "A — HTTP client service trait" -t feature -p 1 --parent EPIC_A --json
```

Children receive hierarchical ids (`EPIC_A.1`, …) and show under `bd dep tree EPIC_A`.

**Recipe:** Create the **phase epic** first, then create each **top-level deliverable** in the phase doc with `--parent <phase_epic>`.

## Blocking dependencies (`bd dep add`)

From the [Beads quickstart](https://github.com/gastownhall/beads/blob/main/website/docs/getting-started/quickstart.md):

```bash
bd dep add bd-2 bd-1
```

means **`bd-2` is blocked by `bd-1`** (bd-2 cannot be “ready” until bd-1 is closed).

**Mnemonic:** `bd dep add <blocked> <blocker>`.

Phase docs express edges as:

```text
blocked_slug  →  blocker_slug
```

When importing, substitute **actual** Beads ids after each `bd create`.

## Long descriptions

Avoid shell-escaping issues for multi-line acceptance criteria:

```bash
bd create "Title" -t task -p 2 --description=- --json <<'EOF'
Paste acceptance criteria here.
EOF
```

Or use `--body-file=path/to-snippet.md`.

## Machine-readable manifest

[`beads-manifest.yaml`](./beads-manifest.yaml) lists **all** phase tasks with:

- `slug`, `title`, `type`, `priority`, `phase`
- `depends_on`: list of **blocker slugs** (Beads: `bd dep add <this_task_bd_id> <blocker_bd_id>` **after** you map slugs to `bd-*` ids)

Epics (`type: epic`) are created first per phase; other tasks use `--parent <that_epic_bd_id>`.

**Multi-blocker tasks:** If `depends_on` has multiple entries, run **`bd dep add` once per blocker** for the same blocked issue.

**Scripting:** Any small script (Python + PyYAML, `yq`, etc.) can read the YAML and emit `bd` commands in **topological order**; keep slug ↔ `bd-*` mapping in a sidecar JSON as the script runs.

## Linking to repository paths

When filing Beads issues, include pointers to:

- Crate paths: `crates/id_effect/…`, `crates/id_effect_reqwest/…`, etc.
- Phase markdown: `docs/effect-ts-parity/phases/phase-*.md`
- Stable slug in the first line of the description: `Slug: iep-a-010`

This keeps **git history**, **design docs**, and **bd issues** aligned.

## What not to do

- Do **not** hand-edit Beads/Dolt storage; use `bd` commands.
- Do **not** duplicate the same work as both a markdown checkbox army **and** unrelated tracker entries — pick **bd** as source of execution state; keep these docs as the **spec** for what each issue means.
