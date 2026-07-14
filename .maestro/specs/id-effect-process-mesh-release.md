---
title: id_effect v0.5.0 Process Mesh Release
slug: id-effect-process-mesh-release
mode: heavy
work_type: initiative
risk_class: high
version: 1
acceptance_criteria:
  - Process Mesh work is committed and workspace is green (cargo test --workspace, clippy --all-features -D warnings, mdbook build) with roam index rebuilt to cover id_effect_node and id_effect_process
  - Placement is unified in compute/cluster.rs behind a single scorer fed by mesh LoadReport telemetry and consumed by DistributedSupervisor and mesh.remote_spawn instead of naive self-or-prefer selection
  - JobSpec.target_node is honored by a real runner and FabricJobRunner is implemented so ComputeSupervisor scale_out_placed triggers a real mesh remote spawn or durable jobs enqueue
  - DistributedStepJournal and NetworkJournalStub are wired to a real remote backend (id_effect_rpc or mesh transport) with DistributedJournalConfig made functional and covered by a test
  - id_effect_process Supervisor restart-intensity delegates to concurrency Supervisor policy rather than duplicating it, verified by test
  - ClusterSingleton has a production Postgres advisory-lock SingletonLease via id_effect_sql_pg with MemoryLease retained for tests
  - Root Cargo.toml defines workspace.package and every crate uses version.workspace true with all crates and inter-crate version pins bumped 0.4.0 to 0.5.0 in one atomic change that passes publish verify-versions
  - publish.yml and scripts/validate-crate-packaging.sh publish id_effect_process and id_effect_node at correct dependency levels and validate-crate-packaging.sh passes
  - CHANGELOG.md has a 0.5.0 entry folding the prior Unreleased Cap service names notes plus Process Mesh distribution supervision and the completed wiring with a version alignment table
  - Book part6 ch12 doc drift reconciled to real wiring and Part VII ch37 ch38 ch39 deepened to release-adequate depth with guarantees tables and runnable example refs
  - id_effect_node and id_effect_process enforce missing_docs and carry keywords categories readme metadata and the algebra monad unimplemented is confirmed unreachable or gated
  - Full release gate is green and v0.5.0 tag plus cargo publish --dry-run in publish order succeeds for all crates including node and process
non_goals:
  - Hot code loading or cross-version cluster upgrades (ADR 0009 non-goal)
  - Exactly-once messaging or Byzantine fault tolerance (ADR 0009 non-goal)
  - Replacing concurrency Supervisor with the process supervision tree (mesh extends not replaces)
  - Comprehensive rewrite of the entire book beyond release-adequate Part VII depth
  - Renaming the three Supervisor types (ComputeSupervisor, process Supervisor, DistributedSupervisor) in this release
---

# id_effect v0.5.0 Process Mesh Release

Completes the ADR-0009 Process Mesh integration and ships it: wires the placement,
jobs-offload, and distributed-journal stubs the mesh now makes real; adds the CP
Postgres singleton lease; adopts workspace.package version inheritance; and repairs
the release pipeline (publish coverage, changelog, version bump, docs) so the two
headline crates (id_effect_node, id_effect_process) reach crates.io.

## Blockers repaired

- Publish pipeline omits id_effect_node and id_effect_process (never reach crates.io)
- CHANGELOG has no 0.5.0 / Process Mesh entry
- All-or-nothing version bump: no workspace.package; 26 crates hardcode 0.4.0 + inter-crate pins
- Process Mesh work uncommitted (examples 132/133, tests cluster_registry, dist_supervision, ch37-39, dist_supervisor.rs, mesh.rs, singleton.rs)

## Supersession wiring (given Process Mesh)

- compute/cluster.rs placement scorer to DistributedSupervisor + mesh.remote_spawn
- id_effect_jobs FabricJobRunner + JobSpec.target_node to real offload
- id_effect_workflow DistributedStepJournal / NetworkJournalStub to real remote backend
- id_effect_process Supervisor to delegate restart-intensity to concurrency Supervisor
- ClusterSingleton to Postgres advisory-lock SingletonLease (id_effect_sql_pg)

See .cursor/plans/id-effect-process-mesh-release.plan.md for full architecture and per-leaf detail.
