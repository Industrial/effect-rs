# Axum host (`id_effect_axum`)

Workspace crate **`id_effect_axum`** runs **`Effect<A, E, R>`** programs inside **[Axum](https://docs.rs/axum)** handlers on the **same Tokio runtime** as `#[tokio::main]` / `axum::serve`.

## Mental model

- Axum stays **`async fn`** at the **wire edge**; your **domain** stays in **`Effect`** with environment **`R`** (often `Clone` state or an `id_effect::Context`).
- The bridge takes **`State`** (or `&mut R` from custom extractors), **builds** an effect from `&mut R`, then drives it to completion with **`id_effect_tokio::run_async`** using **`tokio::task::block_in_place`** + **`Handle::block_on`** so the **`Effect` value never crosses a `Send` async boundary** incorrectly.

## Runtime requirements

Workspace effects are intentionally **not `Send`** in the general case. Axum handlers must return **`Send`** futures; the **`id_effect_axum`** adapter satisfies that contract by running the interpreter on the **multi-thread** runtime’s blocking integration path. Use a **multi-thread** Tokio runtime (default for `#[tokio::main]`). On **`current_thread`**, prefer driving effects outside this adapter or supply a dedicated integration.

**Tests:** `#[tokio::test]` defaults to **current-thread** and can **panic** with this bridge—use e.g. `#[tokio::test(flavor = "multi_thread", worker_threads = 2)]` for router tests (see crate tests for the pattern).

## API surface

- **`routing::get` / `post` / …** — ergonomic wrappers when the handler is `Fn(&mut R) -> Effect<…>`.
- **`run_with_env`** — you already have `&mut R` or `State(mut R)`; build and run one effect.
- **`execute`** — Axum handler: `State` + `IntoResponse` for success and failure.
- **`json::decode_json_schema`** — validate JSON bodies with **`Schema::decode_unknown`** and map errors to HTTP responses (e.g. **422** with paths).

## Further reading

- [RPC boundaries](./ch07-12-rpc-boundaries.md) — `id_effect_rpc` envelopes, correlation ids, tracing
- `cargo doc --open -p id_effect_axum`
- Examples: `cargo run -p id_effect_axum --example 010_routing_hello`
- [Tokio bridge](./ch07-05-tokio-bridge.md) for interpreter semantics; [Tower](./ch07-09-tower-service.md) for generic `Service` composition without Axum.
