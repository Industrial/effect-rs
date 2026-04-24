# `id_effect_platform`

Cross-cutting **platform** traits (HTTP, filesystem, process) for [`id_effect`](../id_effect), aligned with Effect.ts [`@effect/platform`](https://effect.website/docs/platform/introduction).

## Modules

| Module | Description |
|--------|-------------|
| [`error`](src/error.rs) | `HttpError`, `FsError`, `ProcessError`, `PlatformError` |
| [`http`](src/http.rs) | `HttpClient`, `ReqwestHttpClient`, `HttpClientKey`, portable request/response |
| [`fs`](src/fs.rs) | `FileSystem`, `LiveFileSystem`, `TestFileSystem`, `FileSystemKey` |
| [`process`](src/process.rs) | `ProcessRuntime`, `TokioProcessRuntime`, `ProcessRuntimeKey` |
| [`uri`](src/uri.rs) | `http::Uri` parse / build helpers |

## Design

See RFC [0001-id-effect-platform.md](../../docs/effect-ts-parity/rfcs/0001-id-effect-platform.md).

## Testing

Unit and integration tests follow the repository root **[`TESTING.md`](../../TESTING.md)** (BDD-style names, nested `#[cfg(test)]` modules beside implementation, `rstest` where inputs form a table). Integration tests under `tests/` complement wire/process scenarios; run `cargo test -p id_effect_platform`.

## Relation to `id_effect_reqwest`

- **`id_effect_reqwest`** remains the place for **reqwest-specific helpers** (pools, JSON+Schema, etc.).
- Prefer **`id_effect_platform`** for **portable** HTTP calls keyed by [`HttpClientKey`](src/http.rs), so tests can swap [`HttpClient`](src/http.rs) implementations.
- Low-level `send(|c| c.get(...))` in `id_effect_reqwest` can coexist; new code should gravitate toward [`http::execute`](src/http.rs) + layers.

## Runnable example

```bash
cargo run -p id_effect_platform --example 010_platform_http_get
```
