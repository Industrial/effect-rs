# Tokio bridge (`id_effect_tokio`)

The core **`id_effect`** crate defines interpreters (`run_blocking`, `run_async`, `run_fork`, …) and the **`Runtime`** trait. Workspace crate **`id_effect_tokio`** supplies the **Tokio-backed** implementation you use in binaries and services that already run on **`#[tokio::main]`**.

Read this section **before** [Platform I/O](./ch07-06-platform-services.md), [HTTP via reqwest](./ch07-07-reqwest-http.md), and [Axum](./ch07-08-axum-host.md): every adapter below ultimately drives effects with the same Tokio integration rules.

## What `id_effect_tokio` provides

- **`TokioRuntime`** — implements [`id_effect::Runtime`]: cooperative **sleep** and **yield**, and schedules **forked** fibers on Tokio’s **blocking thread pool** via `spawn_blocking` + an internal `run_blocking` driver (the effect graph itself is **not** assumed `Send` for `tokio::spawn`).
- **Re-exports** — `run_async`, `run_blocking`, `run_fork`, and `yield_now` from `id_effect` for convenient use at the async boundary next to `TokioRuntime`.
- **`spawn_blocking_run_async`** — when an async effect graph is **still not `Send`** (scopes, pool checkouts, …) but must be driven by the **real async** interpreter (`run_async`) rather than the blocking-only fiber driver, this pattern runs construction + `run_async` **on Tokio’s blocking pool** with `Handle::block_on` inside. **`id_effect_axum`** uses the same idea so Axum handlers return **`Send`** futures.

## Mental model

| Concern | Where it lives |
|---------|----------------|
| Describing work | `Effect<A, E, R>` in your domain |
| Blocking / tests | `run_blocking(eff, env)` |
| Async I/O on Tokio | `id_effect_tokio::run_async(eff, env)` with a live runtime |
| Fibers vs OS threads | `run_fork` + `join` / `interrupt` (see [Concurrency](../part3/ch09-00-concurrency.md)) |

`id_effect_tokio` does **not** replace the effect system; it **wires** the interpreter to Tokio timers and thread pools so `Effect::new_async` steps compose with `await` from the host runtime.

## Sharp edges

1. **`Send`**: futures produced by `run_async` are often **not** `Send`. Do not stash them in a `tokio::spawn` task without an adapter (Axum/Tower helpers handle this explicitly).
2. **Router tests**: `#[tokio::test]` defaults to **current-thread** runtime; **`id_effect_axum`** documents that **`multi_thread`** flavor is required for its bridge. Prefer `#[tokio::test(flavor = "multi_thread", worker_threads = 2)]` when exercising Axum + effects together.

## Further reading

- Crate docs: `cargo doc --open -p id_effect_tokio`
- Examples under `crates/id_effect_tokio/examples/` (e.g. end-to-end Tokio wiring)
