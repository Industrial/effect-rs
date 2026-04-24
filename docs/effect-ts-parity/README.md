# Effect.ts parity roadmap — documentation for Beads

This directory holds **implementation plans** for closing gaps between **id_effect** (Rust) and the broader **Effect.ts** ecosystem (`effect`, `@effect/platform`, `@effect/opentelemetry`, `@effect/sql`, etc.). The plans are written so you can **mirror them into [Beads](https://github.com/gastownhall/beads)** (`bd`): epics, parent/child trees, and blocking dependencies.

## Documents

| File | Purpose |
|------|---------|
| [CONVENTIONS.md](./CONVENTIONS.md) | Slug scheme, priority scale, how `bd create` / `--parent` / `bd dep add` map to this tree |
| [PHASE-DEPENDENCIES.md](./PHASE-DEPENDENCIES.md) | Cross-phase dependency graph and suggested sequencing |
| [beads-manifest.yaml](./beads-manifest.yaml) | **Machine-readable** task list + `depends_on` edges for scripting `bd create` / `bd dep add` |
| [CHECKLIST-upstream-effect.md](./CHECKLIST-upstream-effect.md) | Per-release upstream parity checklist (Phase I) |
| [UPSTREAM-VERSION](./UPSTREAM-VERSION) | Pin of last reviewed upstream `effect` / Effect.ts version |
| [phases/phase-a-unified-platform.md](./phases/phase-a-unified-platform.md) | Phase A — unified platform services (Effect `@effect/platform`) |
| [phases/phase-b-opentelemetry.md](./phases/phase-b-opentelemetry.md) | Phase B — OpenTelemetry integration |
| [phases/phase-c-sql.md](./phases/phase-c-sql.md) | Phase C — SQL / database access layer |
| [phases/phase-d-rpc.md](./phases/phase-d-rpc.md) | Phase D — RPC & service contracts |
| [phases/phase-e-cli.md](./phases/phase-e-cli.md) | Phase E — CLI ergonomics |
| [phases/phase-f-supervision.md](./phases/phase-f-supervision.md) | Phase F — fiber supervision & restart policies |
| [phases/phase-g-cluster-workflow.md](./phases/phase-g-cluster-workflow.md) | Phase G — cluster / durable workflow (spike → product) |
| [phases/phase-h-ai.md](./phases/phase-h-ai.md) | Phase H — AI / LLM client abstractions |
| [phases/phase-i-parity-maintenance.md](./phases/phase-i-parity-maintenance.md) | Phase I — ongoing API parity & release hygiene |

## Quick start with Beads

1. Install and init Beads in this repo: see [CONVENTIONS.md](./CONVENTIONS.md#prerequisites).
2. Read [PHASE-DEPENDENCIES.md](./PHASE-DEPENDENCIES.md) for **which phase epics block others**.
3. For each phase you are importing, open the phase file and follow the section **“Beads import recipe”** at the bottom:
   - Create the **phase epic** first; record its `bd-*` id.
   - Create **workstream** tasks with `--parent <phase_epic>` (Beads child ids look like `bd-xxxx.1`, `bd-xxxx.2`, …).
   - Add **blocking edges** with `bd dep add <blocked> <blocker>` (first id is blocked until second completes — see [CONVENTIONS.md](./CONVENTIONS.md#blocking-dependencies-bd-dep-add)).

Stable **slugs** (e.g. `iep-a-010`) in each phase doc are for **traceability** in commit messages and design docs (`git commit -m "… (iep-a-010)"`). After `bd create`, you can paste the assigned `bd-*` id next to the slug in your notes or in Beads descriptions.

### Three-level Beads trees (per phase)

Each file under [`phases/`](./phases/) includes a section **“Three-level Beads task tree”**:

1. **Level 1** — one **epic** per phase (`-t epic`).
2. **Level 2** — **workstreams** (`bd create "…" --parent EPIC`) grouping related deliverables.
3. **Level 3** — **leaf tasks** (`bd create "…" --parent WS_*`) with `bd dep add <blocked> <blocker>` chains inside each stream and occasional **cross-workstream** deps.

That maps cleanly to Beads hierarchies (`bd dep tree EPIC`) while keeping sibling work parallel where possible.

## Relationship to the core library

The **`id_effect`** crate already covers most of the **core** `effect` package: `Effect`, layers, fibers, resources, schedules, STM, streams, schema, testing helpers, etc. These phases focus on **satellite capabilities** and **production integration** that Effect.ts spreads across `@effect/*` packages.

## License

These planning documents follow the repository’s documentation licensing where applicable; code referenced in plans is governed by the repo’s code license.
