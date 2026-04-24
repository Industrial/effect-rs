# RFC 0001 — `id_effect_platform` (Phase A)

## Summary

Introduce **`id_effect_platform`**: capability traits and Tokio-backed implementations for **HTTP**, **filesystem**, and **process** I/O, aligned with Effect.ts **`@effect/platform`** so application `R` depends on **services** instead of concrete `reqwest` / `std::fs` / `tokio::process` at call sites.

## Crate layout

| Module | Contents |
|--------|----------|
| `error` | `HttpError`, `FsError`, `ProcessError`, `PlatformError`, `From` bridges |
| `http` | `HttpRequest`, `HttpResponse`, `HttpClient` trait, `ReqwestHttpClient`, `HttpClientKey`, layers |
| `fs` | `FileSystem` trait, `LiveFileSystem`, `TestFileSystem`, `FileSystemKey`, layers |
| `process` | `CommandSpec`, `ProcessRuntime` trait, `TokioProcessRuntime`, `ProcessRuntimeKey`, layers |
| `uri` | Helpers to build `http::Uri` / validate URLs for HTTP |

## Dependency surface

The crate is a **single** build: it always depends on `reqwest`, `http`, `bytes`, and `tokio` (fs, process, io-util, etc.). Optional feature flags were intentionally omitted for now so every consumer gets the full platform API without `Cargo.toml` feature wiring.

## MSRV

Matches workspace **Rust 2024** / stable as used by `id_effect`.

## Relation to `id_effect_reqwest`

- **`id_effect_reqwest`** remains the **low-level** reqwest + schema helpers crate.
- **`id_effect_platform`** provides the **portable** `HttpClient` abstraction; the reqwest-backed type is one implementation.
- Migration: new code prefers `id_effect_platform` + `HttpClientKey`; existing code can adopt incrementally (see `id_effect_reqwest` README “Platform migration”).

## Security (filesystem)

Document in `fs` module: reject `..` components where appropriate, avoid following symlinks for sensitive reads unless explicitly requested (policy TBD per call site).
