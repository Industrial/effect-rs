# HTTP via reqwest (`id_effect_reqwest`)

Workspace crate **`id_effect_reqwest`** integrates **`reqwest::Client`** with the effect environment: the client lives in **`R`** behind **`ReqwestClientKey`**, and HTTP work is expressed as **`Effect`** values (`send`, `text`, `bytes`, `json`, …).

## Relation to `id_effect_platform`

| Use | Crate |
|-----|--------|
| Portable **`HttpClient`** trait, swap implementations in tests, minimal request/response model | [`id_effect_platform`](./ch07-06-platform-services.md) (`HttpClientKey`, `execute`, …) |
| Rich **reqwest** surface (`RequestBuilder`, redirects, pools), **JSON + `Schema`** decoding | **`id_effect_reqwest`** |

Prefer **`id_effect_platform`** for new application boundaries that should stay stack-agnostic. Keep **`id_effect_reqwest`** when you already depend on **`RequestBuilder`** pipelines, need **pooled clients** (`layer_reqwest_pool`, `send_pooled`), or want **`json_schema`** so parse failures carry **field paths** (`id_effect::schema::ParseError`).

Both crates’ async steps are ordinary **`Effect::new_async`** bodies; drive them on Tokio with **`id_effect_tokio::run_async`** (or re-export) per [Tokio bridge](./ch07-05-tokio-bridge.md).

## Core pieces

- **`ReqwestClientKey`** — tag for the active `reqwest::Client` in `R`.
- **`layer_reqwest_client` / `layer_reqwest_client_with`** — construct the service at the composition root.
- **`send`**, **`text`**, **`bytes`**, **`json`** — effectful helpers over `RequestBuilder`.
- **Optional pools** — TTL pools of clients for connection churn scenarios.

## Further reading

- Crate-level docs: `cargo doc --open -p id_effect_reqwest`
- [Migrating from async](../appendix-b-migration.md) for the general `async fn` → `Effect` move; combine with this crate at HTTP boundaries when you choose the reqwest adapter.
