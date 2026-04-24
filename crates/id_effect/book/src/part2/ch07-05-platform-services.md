# Platform services (`id_effect_platform`)

This chapter is a **stub** for the workspace crate **`id_effect_platform`**, which mirrors the idea of Effect.ts **`@effect/platform`**: HTTP, filesystem, and process capabilities as **services** in `R` instead of calling `reqwest`, `std::fs`, or `tokio::process` directly throughout your code.

## Why a separate crate?

- **Test doubles:** swap [`TestFileSystem`](https://docs.rs/id_effect_platform/latest/id_effect_platform/fs/struct.TestFileSystem.html) in tests while production uses [`LiveFileSystem`](https://docs.rs/id_effect_platform/latest/id_effect_platform/fs/struct.LiveFileSystem.html).
- **Stable boundaries:** depend on [`HttpClient`](https://docs.rs/id_effect_platform/latest/id_effect_platform/http/trait.HttpClient.html) rather than a concrete HTTP stack.
- **Layering:** install implementations with the same [`Layer`](./ch06-01-what-is-layer.md) patterns you already use for other services.

## Where to read more

- Crate README: `crates/id_effect_platform/README.md`
- RFC: `docs/effect-ts-parity/rfcs/0001-id-effect-platform.md`
- Runnable example: `cargo run -p id_effect_platform --example 010_platform_http_get`

A full mdBook tour (Axum handlers wired only to platform traits, migration from `id_effect_reqwest`, etc.) can extend this stub in a follow-up edit.
