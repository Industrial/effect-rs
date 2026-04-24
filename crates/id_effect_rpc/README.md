# `id_effect_rpc`

Thin helpers for **RPC-shaped HTTP** boundaries when hosting `Effect` programs behind Axum:

- **`RpcError`** — JSON envelope + status code + [`axum::response::IntoResponse`]
- **Correlation ids** — read/propagate `x-correlation-id` (and optional generation)
- **Tracing spans** — stable `rpc.request` span fields that compose with OpenTelemetry layers on the subscriber

See the mdBook chapter [RPC boundaries with id_effect](../id_effect/book/src/part2/ch07-12-rpc-boundaries.md) (built with the `id_effect` book target).

## Examples

```bash
cargo run -p id_effect_rpc --example 010_json_greet
```

## Phase D

This crate implements **Phase D (stages D1–D2)** from `docs/effect-ts-parity/phases/phase-d-rpc.md`. Stage **D3** (codegen) remains optional backlog.
