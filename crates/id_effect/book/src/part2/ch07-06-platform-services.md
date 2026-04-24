# Platform I/O (`id_effect_platform`)

Workspace crate **`id_effect_platform`** mirrors Effect.ts **`@effect/platform`**: **HTTP**, **filesystem**, and **process** capabilities as **typed services in `R`**, not as ad hoc `reqwest` / `std::fs` / `tokio::process` calls scattered through domain code.

The crate is a **single build**: HTTP (`reqwest` + `http` + `bytes`), Tokio fs/process, and URI helpers ship together—no Cargo feature matrix to toggle when learning or publishing docs.

## Why separate from `id_effect`?

- **Ports, not drivers** — traits like [`HttpClient`](https://docs.rs/id_effect_platform/latest/id_effect_platform/http/trait.HttpClient.html), [`FileSystem`](https://docs.rs/id_effect_platform/latest/id_effect_platform/fs/trait.FileSystem.html), and [`ProcessRuntime`](https://docs.rs/id_effect_platform/latest/id_effect_platform/process/trait.ProcessRuntime.html) describe *what* your program needs; production stacks install concrete implementations via **layers** (same pattern as [Layers](./ch06-00-layers.md)).
- **Test doubles** — [`TestFileSystem`](https://docs.rs/id_effect_platform/latest/id_effect_platform/fs/struct.TestFileSystem.html) is an in-memory `FileSystem` for fast, deterministic tests; production uses [`LiveFileSystem`](https://docs.rs/id_effect_platform/latest/id_effect_platform/fs/struct.LiveFileSystem.html) backed by Tokio.
- **Stable HTTP boundary** — domain code depends on **`HttpClientKey`** + `execute`, not on `reqwest::RequestBuilder`. The default implementation is **`ReqwestHttpClient`**, swappable in tests.

## Modules at a glance

| Module | Responsibility |
|--------|------------------|
| `error` | `HttpError`, `FsError`, `ProcessError`, unified [`PlatformError`](https://docs.rs/id_effect_platform/latest/id_effect_platform/error/enum.PlatformError.html) |
| `http` | `HttpRequest` / `HttpResponse`, `HttpClient`, `ReqwestHttpClient`, `layer_http_client`, `execute` |
| `fs` | `FileSystem`, live + test impls, `FileSystemKey`, `read` helper |
| `process` | `CommandSpec`, `ProcessRuntime`, `TokioProcessRuntime`, `spawn_wait` |
| `uri` | Small helpers around `http::Uri` |

## Wiring pattern

1. Declare **`R`** that includes `Service<HttpClientKey, …>` (or fs/process keys) via `Context` / `Cons` as in [A Complete DI Example](./ch07-04-complete-example.md).
2. Build a **`Layer`** from `layer_http_client`, `layer_file_system`, or `layer_process_runtime`.
3. Call **`execute`**, **`read`**, or **`spawn_wait`** from handlers; drive with **`id_effect_tokio::run_async`** (see [Tokio bridge](./ch07-05-tokio-bridge.md)).

## Security note (filesystem)

[`TestFileSystem`](https://docs.rs/id_effect_platform/latest/id_effect_platform/fs/struct.TestFileSystem.html) rejects path keys containing `..`. Live I/O follows OS semantics—**sandbox or canonicalize** untrusted paths at a higher layer when exposing file access.

## Runnable example

```bash
cargo run -p id_effect_platform --example 010_platform_http_get
```

## Spec and parity

- RFC: `docs/effect-ts-parity/rfcs/0001-id-effect-platform.md`
- Crate README: `crates/id_effect_platform/README.md`

## Next

For **reqwest-shaped** APIs (pools, `RequestBuilder`, schema-decoded JSON helpers), see [HTTP via reqwest](./ch07-07-reqwest-http.md) and decide when to prefer platform **ports** vs **reqwest** adapters.
