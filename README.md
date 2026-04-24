# id_effect

**id_effect** brings [`Effect<A, E, R>`](https://docs.rs/id_effect): structured effects, typed errors, and composable services in Rust. The design is heavily inspired by [Effect](https://effect.website) (Effect-TS): programs describe work as lazy values, wire dependencies through the type system, and run under an explicit runtime.

If you have written async Rust—`Future`s, `Pin`, long `.await?` chains—and want clearer dependency boundaries, recoverable errors, and tests that do not rely on global mocks, this workspace is for you.

<!-- Badge row 1: CI & quality -->
[![CI](https://github.com/Industrial/id_effect/actions/workflows/ci.yml/badge.svg)](https://github.com/Industrial/id_effect/actions/workflows/ci.yml)
[![Docs & Pages](https://github.com/Industrial/id_effect/actions/workflows/docs.yml/badge.svg)](https://github.com/Industrial/id_effect/actions/workflows/docs.yml)
[![Security Audit](https://github.com/Industrial/id_effect/actions/workflows/audit.yml/badge.svg)](https://github.com/Industrial/id_effect/actions/workflows/audit.yml)
[![codecov](https://codecov.io/gh/Industrial/id_effect/branch/main/graph/badge.svg)](https://codecov.io/gh/Industrial/id_effect)

<!-- Badge row 2: crates.io -->
[![crates.io](https://img.shields.io/crates/v/id_effect.svg)](https://crates.io/crates/id_effect)
[![docs.rs](https://docs.rs/id_effect/badge.svg)](https://docs.rs/id_effect)
[![downloads](https://img.shields.io/crates/d/id_effect.svg)](https://crates.io/crates/id_effect)

<!-- Badge row 3: repository health -->
[![License](https://img.shields.io/badge/license-CC--BY--SA--4.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org)
[![Edition](https://img.shields.io/badge/edition-2024-orange.svg)](https://doc.rust-lang.org/edition-guide/rust-2024/)
[![GitHub Stars](https://img.shields.io/github/stars/Industrial/id_effect?style=social)](https://github.com/Industrial/id_effect/stargazers)

### Start with the book

**[Typed Effects in Rust](https://industrial.github.io/id_effect/)** is the main way to learn **id_effect**: it walks the same story as the library—from `Effect<A, E, R>` and the `effect!` macro, through context, layers, and services, to concurrency, resources, STM, streams, schema, and testing. The API docs answer “what does this type do?”; the book answers “how do I think in effects?” and ties the pieces together.

- **Read online:** [Typed Effects in Rust](https://industrial.github.io/id_effect/) (GitHub Pages)
- **Terminology:** [Glossary](https://industrial.github.io/id_effect/appendix-c-glossary.html) — quick definitions for `Effect`, `Cause`, `Layer`, `Fiber`, `Stm`, and the rest of the vocabulary
- **Source:** [`crates/id_effect/book/`](crates/id_effect/book/) (build locally with `moon run :book`)

---

## Overview

**id_effect** models programs as **descriptions** of work (`Effect` values) rather than immediate side effects. You compose them, attach requirements to the type signature, and run them when you choose. That buys:

- **`Effect<A, E, R>`** — success type `A`, error type `E`, environment/requirements `R`.
- **Context and layers** — typed dependency injection: declare what an effect needs, provide it once at the edge.
- **`pipe!` and `effect!`** — ergonomic composition and do-notation-style blocks without hiding what the types mean.
- **Streams, STM, schema** — pull-based streams, software transactional memory, and structural validation for larger systems.
- **No bundled async executor** — the core stays portable; Tokio and other runtimes live in separate integration crates (see below).

For depth beyond this README, use the mdBook [**Typed Effects in Rust**](https://industrial.github.io/id_effect/) (see **Start with the book** above). It follows the same arc as the library: foundations, environment and dependencies, production concerns (errors, concurrency, resources, scheduling), then advanced topics (STM, streams, schema, testing).

---

## Crates in this workspace

| Crate | Version | Description |
|-------|---------|-------------|
| [`id_effect`](crates/id_effect) | [![crates.io](https://img.shields.io/crates/v/id_effect.svg)](https://crates.io/crates/id_effect) | Core: `Effect`, `pipe!`, `effect!`, context, schema, STM, … |
| [`id_effect_macro`](crates/id_effect_macro) | [![crates.io](https://img.shields.io/crates/v/id_effect_macro.svg)](https://crates.io/crates/id_effect_macro) | Declarative macros (`ctx!`, `pipe!`, …) |
| [`id_effect_proc_macro`](crates/id_effect_proc_macro) | [![crates.io](https://img.shields.io/crates/v/id_effect_proc_macro.svg)](https://crates.io/crates/id_effect_proc_macro) | Procedural `effect!` macro |
| [`id_effect_tokio`](crates/id_effect_tokio) | [![crates.io](https://img.shields.io/crates/v/id_effect_tokio.svg)](https://crates.io/crates/id_effect_tokio) | Tokio runtime adapter |
| [`id_effect_axum`](crates/id_effect_axum) | [![crates.io](https://img.shields.io/crates/v/id_effect_axum.svg)](https://crates.io/crates/id_effect_axum) | Axum integration |
| [`id_effect_logger`](crates/id_effect_logger) | [![crates.io](https://img.shields.io/crates/v/id_effect_logger.svg)](https://crates.io/crates/id_effect_logger) | Logging service (tracing backend) |
| [`id_effect_config`](crates/id_effect_config) | [![crates.io](https://img.shields.io/crates/v/id_effect_config.svg)](https://crates.io/crates/id_effect_config) | `ConfigProvider` + Figment/serde layers |
| [`id_effect_cli`](crates/id_effect_cli) | [![crates.io](https://img.shields.io/crates/v/id_effect_cli.svg)](https://crates.io/crates/id_effect_cli) | CLI edge: `run_main`, `Exit` / `Cause` → `ExitCode`, optional `clap` ([book](https://industrial.github.io/id_effect/part3/ch16-00-cli-with-clap.html)) |
| [`id_effect_platform`](crates/id_effect_platform) | (publish with core) | Platform traits: HTTP, FS, process (`@effect/platform` parity) |
| [`id_effect_reqwest`](crates/id_effect_reqwest) | [![crates.io](https://img.shields.io/crates/v/id_effect_reqwest.svg)](https://crates.io/crates/id_effect_reqwest) | HTTP via reqwest |
| [`id_effect_tower`](crates/id_effect_tower) | [![crates.io](https://img.shields.io/crates/v/id_effect_tower.svg)](https://crates.io/crates/id_effect_tower) | Tower `Service` bridge |

---

## Minimal example

Add the crate:

```toml
[dependencies]
id_effect = "0.1"
```

```rust
use id_effect::Effect;

fn greet(name: &str) -> Effect<String, (), ()> {
    Effect::succeed(format!("Hello, {name}!"))
}

fn main() {
    let result = id_effect::run_blocking(greet("world"), ());
    println!("{result:?}");
}
```

For a guided path through the API, read [**Typed Effects in Rust**](https://industrial.github.io/id_effect/) first, then use the numbered examples under [`crates/id_effect/examples/`](crates/id_effect/examples/) and [docs.rs](https://docs.rs/id_effect).

**CLI template:** [`examples/cli-minimal/`](examples/cli-minimal/) shows `clap` + [`id_effect_cli`](crates/id_effect_cli) + [`id_effect_config`](crates/id_effect_config) (`Secret`). Run:

```bash
devenv shell -- cargo run -p cli_minimal -- --token dummy
```

---

## Documentation

| Resource | Link |
|----------|------|
| **Book (primary learning path)** | [**Typed Effects in Rust**](https://industrial.github.io/id_effect/) — [glossary](https://industrial.github.io/id_effect/appendix-c-glossary.html) |
| **CLI with `clap` + `ExitCode`** | [CLI with clap (`id_effect_cli`)](https://industrial.github.io/id_effect/part3/ch16-00-cli-with-clap.html) |
| API reference | [docs.rs/id_effect](https://docs.rs/id_effect) |
| Examples | [`crates/id_effect/examples/`](crates/id_effect/examples/) |

---

## Development

This repository uses [devenv](https://devenv.sh). Run commands inside the dev shell:

```bash
devenv shell -- <command>
```

Common [Moon](https://moonrepo.dev) tasks:

```bash
# Format
devenv shell -- moon run :format

# Check + clippy
devenv shell -- moon run :clippy

# Tests (nextest)
devenv shell -- moon run :test

# Coverage (95% threshold)
devenv shell -- moon run :coverage

# Build
devenv shell -- moon run :build

# Run examples for a crate
devenv shell -- moon run id_effect_lib:examples

# Security audit
devenv shell -- moon run :audit

# API docs + mdBook
devenv shell -- moon run :docs :book

# Pre-push checks
devenv shell -- moon run :format :check :build :test :coverage :audit :check-docs
```

### Continuous integration

| Workflow | Triggers | What it does |
|----------|----------|--------------|
| [CI](https://github.com/Industrial/id_effect/actions/workflows/ci.yml) | push/PR → `main` | Format, check, clippy, test, build, coverage, doc-check, matrix (stable+beta × linux+mac+win) |
| [Docs & Pages](https://github.com/Industrial/id_effect/actions/workflows/docs.yml) | push/PR → `main` | API docs + mdBook → GitHub Pages |
| [Security Audit](https://github.com/Industrial/id_effect/actions/workflows/audit.yml) | daily + `Cargo.lock` changes | `cargo audit` |
| [Publish](https://github.com/Industrial/id_effect/actions/workflows/publish.yml) | `v*.*.*` tag | Test, then publish crates in dependency order |

### Releases

```bash
# 1. Bump versions in the workspace Cargo.toml files
# 2. Commit and push
# 3. Tag and push — CI publishes
git tag v0.2.0
git push --tags
```

Publishing expects `CARGO_REGISTRY_TOKEN` on crates.io; `CODECOV_TOKEN` is optional for coverage uploads. The publish workflow can use a `crates-io` GitHub environment for approval gates.

---

## Coverage

[![codecov](https://codecov.io/gh/Industrial/id_effect/branch/main/graph/badge.svg)](https://codecov.io/gh/Industrial/id_effect)

CI enforces **≥ 95%** lines, regions, and functions via `cargo llvm-cov nextest`.

![Coverage sunburst](https://codecov.io/gh/Industrial/id_effect/branch/main/graphs/sunburst.svg)

---

## Star history

[![Star History Chart](https://api.star-history.com/svg?repos=Industrial/id_effect&type=Date)](https://star-history.com/#Industrial/id_effect&Date)

---

## Contributors

Thanks to everyone who has contributed patches, reported issues, or improved the types.

[![Contributors](https://contrib.rocks/image?repo=Industrial/id_effect)](https://github.com/Industrial/id_effect/graphs/contributors)

---

## License

This project is licensed under [CC-BY-SA-4.0](LICENSE).
