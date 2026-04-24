# Phase B — OpenTelemetry integration (`@effect/opentelemetry` parity)

**Slug prefix:** `iep-b-*`  
**Effect.ts reference:** `@effect/opentelemetry` — traces, metrics, logs aligned with OTEL semantics and context propagation.  
**Goal:** First-class **OpenTelemetry** export and **context propagation** integrated with `id_effect` fibers, spans, and existing `observability` hooks—without forcing every user to depend on OTEL in the core crate.

**Implementation (workspace):** `crates/id_effect_opentelemetry` — tracing bridge (`with_span_otel`), W3C header propagation, metric bridges, and mdBook `part2/ch07-12-opentelemetry.md`.

## Executive summary

`id_effect` already exposes **tracing-oriented** APIs (`with_span`, fiber events, metrics primitives). Phase B adds an **`id_effect_opentelemetry`** (name TBD) integration crate that:

1. Wires **trace** and **metric** exporters to the OpenTelemetry SDK for Rust (`opentelemetry`, `opentelemetry_sdk`, `tracing-opentelemetry` or OTLP exporters as appropriate).
2. Maps **`FiberId`** / internal span stack to OTEL span context where possible.
3. Provides **Layers** (in the DI sense) to install subscribers/propagators in `run_async` / Axum stacks.
4. Documents **Axum + Tokio + OTEL** as the canonical production setup.

## Non-goals (initial milestones)

- Implementing a full **logs** bridge if the ecosystem split is unstable—start with traces + metrics; add logs when clear.
- Supporting every exporter (Jaeger native, etc.)—**OTLP** first; document others as extensions.
- Changing the **default** behavior of `id_effect` core for users who do not opt in.

## Baseline inventory

| Location | Relevant APIs |
|----------|---------------|
| `crates/id_effect/src/observability/tracing.rs` | Span stack, fiber refs, events |
| `crates/id_effect/src/observability/metric.rs` | Metric combinator |
| `crates/id_effect_logger` | Logging pipeline — coordinate naming with OTEL log bridge |

## Design constraints

1. **Optional dependency:** New workspace member behind features (`otlp`, `metrics`, …).
2. **No panics** in export paths; degrade gracefully if exporter misconfigured.
3. **Testability:** Unit tests use in-memory `SpanExporter` / `MetricReader` patterns from `opentelemetry_sdk` testing docs.
4. **Shutdown:** `Scope` / runtime hooks flush exporters on graceful shutdown (document pattern for Axum).

---

## Three-level Beads task tree

**Level 1** = epic → **Level 2** = workstream → **Level 3** = leaf tasks (`--parent` workstream).

### Level 1 — Epic

```bash
bd create "Phase B — OpenTelemetry (@effect/opentelemetry parity)" -t epic -p 1 --json
# EPIC_B
```

### Level 2 — Workstreams

```bash
bd create "B — Audit & design" -t task -p 1 --parent EPIC_B --json
bd create "B — Crate skeleton & trace MVP" -t feature -p 1 --parent EPIC_B --json
bd create "B — Metrics bridge" -t feature -p 2 --parent EPIC_B --json
bd create "B — Runtime integration (Layer, run_async, tests)" -t feature -p 1 --parent EPIC_B --json
bd create "B — Docs, CI, release" -t task -p 2 --parent EPIC_B --json
# WS_B1 … WS_B5
```

### Level 3 — Leaves

**`WS_B1` — Audit & design**

```bash
bd create "Slug iep-b-010 — Audit observability vs OTEL" -t task -p 1 --parent WS_B1 --json
bd create "Slug iep-b-011 — RFC crate boundaries + features + MSRV" -t task -p 1 --parent WS_B1 --json
bd create "Slug iep-b-012 — Spike: with_span → in-memory OTEL exporter" -t task -p 2 --parent WS_B1 --json
bd dep add <b011> <b010>
bd dep add <b012> <b011>
```

**`WS_B2` — Trace MVP**

```bash
bd create "Slug iep-b-020 — id_effect_opentelemetry crate skeleton" -t task -p 1 --parent WS_B2 --json
bd create "Slug iep-b-021 — Span bridge with_span ↔ OTEL" -t feature -p 1 --parent WS_B2 --json
bd create "Slug iep-b-022 — W3C traceparent HTTP propagation" -t feature -p 1 --parent WS_B2 --json
bd create "Slug iep-b-023 — Fiber child inherits span context" -t task -p 1 --parent WS_B2 --json
bd create "Slug iep-b-024 — Tests: nested spans + fork/join" -t task -p 2 --parent WS_B2 --json
bd dep add <b020> <b011>
bd dep add <b021> <b020>
bd dep add <b022> <b021>
bd dep add <b023> <b021>
bd dep add <b024> <b023>
```

**`WS_B3` — Metrics**

```bash
bd create "Slug iep-b-030 — Map Metric to OTEL counter MVP" -t feature -p 2 --parent WS_B3 --json
bd create "Slug iep-b-031 — Histograms for effect latency" -t feature -p 3 --parent WS_B3 --json
bd create "Slug iep-b-032 — Docs: cardinality & attribute limits" -t chore -p 3 --parent WS_B3 --json
bd dep add <b030> <b020>
bd dep add <b031> <b030>
bd dep add <b032> <b030>
```

**`WS_B4` — Runtime**

```bash
bd create "Slug iep-b-040 — Layer builder + tracing_subscriber" -t feature -p 1 --parent WS_B4 --json
bd create "Slug iep-b-041 — run_async + Axum init/teardown/flush" -t task -p 1 --parent WS_B4 --json
bd create "Slug iep-b-042 — run_test isolation for OTEL" -t task -p 2 --parent WS_B4 --json
bd dep add <b040> <b021>
bd dep add <b041> <b040>
bd dep add <b042> <b040>
```

**`WS_B5` — Docs & release**

```bash
bd create "Slug iep-b-050 — mdBook: production OTEL" -t task -p 2 --parent WS_B5 --json
bd create "Slug iep-b-051 — CI optional all-features OTEL job" -t chore -p 2 --parent WS_B5 --json
bd create "Slug iep-b-052 — crates.io metadata" -t chore -p 3 --parent WS_B5 --json
bd dep add <b050> <b041>
bd dep add <b051> <b020>
bd dep add <b052> <b050>
```

Cross-workstream: `bd dep add <b020> <b011>` (skeleton after RFC). Optional: `bd dep add <b022> <id-http-platform>` if Phase A HTTP propagation task exists.

---

## Work breakdown

### B0 — Epic

| Slug | Suggested title | Type | P |
|------|-----------------|------|---|
| `iep-b-000` | Phase B — OpenTelemetry integration (epic) | epic | 1 |

### B1 — Audit & design

| Slug | Suggested title | Type | P | Blocked by |
|------|-----------------|------|---|------------|
| `iep-b-010` | Audit current observability module vs OTEL concepts (trace, span, baggage) | task | 1 | — |
| `iep-b-011` | RFC: crate boundaries (`id_effect` vs `id_effect_opentelemetry`), feature flags, MSRV | task | 1 | `iep-b-010` |
| `iep-b-012` | Spike: minimal trace from `with_span` to in-memory exporter | task | 2 | `iep-b-011` |

**Acceptance (`iep-b-010`):** Written gap analysis (doc or ticket body): which fields map 1:1, which need new APIs, what stays custom.

### B2 — Crate skeleton & trace MVP

| Slug | Suggested title | Type | P | Blocked by |
|------|-----------------|------|---|------------|
| `iep-b-020` | Add `id_effect_opentelemetry` crate with `tracing` + `opentelemetry` deps (gated) | task | 1 | `iep-b-011` |
| `iep-b-021` | Implement span bridge: `with_span` ↔ OTEL spans + parent context | feature | 1 | `iep-b-020` |
| `iep-b-022` | Propagation: W3C `traceparent` extractor/injector helpers for HTTP | feature | 1 | `iep-b-021` |
| `iep-b-023` | Fiber boundary: ensure child fibers inherit span context correctly | task | 1 | `iep-b-021` |
| `iep-b-024` | Regression tests for nested spans + fork/join | task | 2 | `iep-b-023` |

### B3 — Metrics bridge

| Slug | Suggested title | Type | P | Blocked by |
|------|-----------------|------|---|------------|
| `iep-b-030` | Map `Metric` / counters to OTEL metrics instruments (MVP: u64 counter) | feature | 2 | `iep-b-020` |
| `iep-b-031` | Histogram support (latency of effect regions) | feature | 3 | `iep-b-030` |
| `iep-b-032` | Document cardinality / attribute limits | chore | 3 | `iep-b-030` |

### B4 — Runtime integration

| Slug | Suggested title | Type | P | Blocked by |
|------|-----------------|------|---|------------|
| `iep-b-040` | `Layer` / builder: install global tracer provider + `tracing_subscriber` | feature | 1 | `iep-b-021` |
| `iep-b-041` | `run_async` / Axum example: init + teardown (flush) | task | 1 | `iep-b-040` |
| `iep-b-042` | `run_test` guidance: isolate OTEL state between tests | task | 2 | `iep-b-040` |

### B5 — Docs & release

| Slug | Suggested title | Type | P | Blocked by |
|------|-----------------|------|---|------------|
| `iep-b-050` | mdBook chapter: production observability with OTEL | task | 2 | `iep-b-041` |
| `iep-b-051` | CI: optional job building `--all-features` subset for OTEL | chore | 2 | `iep-b-020` |
| `iep-b-052` | crates.io publish metadata | chore | 3 | `iep-b-050` |

---

## Dependency graph (within Phase B)

```text
iep-b-010 → iep-b-011 → iep-b-012
iep-b-011 → iep-b-020 → iep-b-021 → iep-b-022
                              └──→ iep-b-023 → iep-b-024
iep-b-020 → iep-b-030 → iep-b-031
iep-b-021 → iep-b-040 → iep-b-041 → iep-b-050
```

---

## Cross-phase notes

- See [PHASE-DEPENDENCIES.md](../PHASE-DEPENDENCIES.md): **B may soft-depend on A** for HTTP propagation helpers. File **task-level** `bd dep add` edges (e.g. `iep-b-022` blocked by HTTP platform trait task) rather than blocking the entire epic unless intentional.

---

## Beads import recipe

1. `bd create "Phase B — OpenTelemetry (Effect @effect/opentelemetry parity)" -t epic -p 1 --json` → **`EPIC_B`**.

2. Create design chain under epic:

```bash
bd create "Audit id_effect observability vs OTEL" -t task -p 1 --parent EPIC_B --json
bd create "RFC: id_effect_opentelemetry crate boundaries" -t task -p 1 --parent EPIC_B --json
# use bd dep add so RFC is blocked by audit child id
```

3. Parallel track: after `iep-b-020`, **metrics** (`iep-b-030`) can proceed in parallel with **span bridge** (`iep-b-021`) if staffing allows—use Beads deps only where code truly serializes.

4. Tag each issue body with `Slug: iep-b-0xx` for cross-reference to this file.
