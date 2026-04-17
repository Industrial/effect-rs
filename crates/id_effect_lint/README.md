# `id_effect_lint`

Custom [rustc] lint passes for [Effect.rs]–style code: `Effect<A, E, R>`, `effect!`, services, `from_async`, and related patterns.

This crate is a **`cdylib`** that exports `register_lints` for rustc’s plugin interface. **You need a nightly Rust toolchain** and `#![feature(rustc_private)]` when building it.

## Building

From this directory:

```bash
cargo +nightly build --release
```

Install the `rust-src` (and typically `rustc-dev`) components for the same nightly you use to compile the library, matching whatever **rustc driver** loads this plugin.

## Lint inventory

Rules are declared in `src/lib.rs` (categories A–K): function signatures, `effect!` discipline, async/`from_async`, `run_blocking` / `run_test`, services, errors, concurrency, time, tests, logging, and schema boundaries.

## Publishing (crate maintainers)

From `crates/id_effect_lint`:

```bash
cargo publish --no-verify
```

`--no-verify` avoids registry-local `cargo check` when the environment cannot build `rustc_private` plugins. Prefer a full `cargo publish` from a nightly toolchain with `rust-src` / `rustc-dev` when available.

## License

CC-BY-SA-4.0 — see the workspace root in [Industrial/id_effect].

[rustc]: https://doc.rust-lang.org/rustc/
[Effect.rs]: https://github.com/Industrial/id_effect
