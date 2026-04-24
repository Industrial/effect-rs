//! **Platform capabilities** for `id_effect` — HTTP, filesystem, and process abstractions
//! inspired by Effect.ts [`@effect/platform`](https://effect.website/docs/platform/introduction).
//!
//! The crate ships **one** build: [`http`], [`fs`], [`process`], and [`uri`] are always available.
//!
//! ## Example (HTTP, with Tokio)
//!
//! ```ignore
//! use id_effect::{ctx, run_async, Context, Cons, Nil};
//! use id_effect_platform::http::{HttpRequest, ReqwestHttpClient, execute, layer_http_client};
//!
//! type Env = Context<Cons<
//!   id_effect::Service<id_effect_platform::http::HttpClientKey, ReqwestHttpClient>,
//!   Nil,
//! >>;
//!
//! # async fn demo() -> Result<(), id_effect_platform::error::HttpError> {
//! let env = Context::new(Cons(
//!   id_effect::service(id_effect_platform::http::HttpClientKey, ReqwestHttpClient::default_client()),
//!   Nil,
//! ));
//! let eff = execute::<Env, _>(HttpRequest::get("https://example.com"));
//! let res = run_async(eff, env).await?;
//! assert_eq!(res.status, 200);
//! # Ok(())
//! # }
//! ```
//!
//! See the crate [`README.md`](https://github.com/Industrial/id_effect/blob/main/crates/id_effect_platform/README.md)
//! and RFC [`0001-id-effect-platform.md`](../../docs/effect-ts-parity/rfcs/0001-id-effect-platform.md).

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod error;
pub mod fs;
pub mod http;
pub mod process;
pub mod uri;

#[cfg(test)]
mod tests {
  use super::error::PlatformError;

  #[test]
  fn platform_error_unsupported_round_trips_display() {
    let e = PlatformError::Unsupported("sanity");
    assert!(e.to_string().contains("sanity"));
  }
}
