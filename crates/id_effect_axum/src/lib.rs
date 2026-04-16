//! Run [`id_effect::Effect`] programs inside [Axum](https://docs.rs/axum) handlers on the **same Tokio**
//! runtime as `#[tokio::main]` / `axum::serve`.
//!
//! ## Mental model
//!
//! - Axum already drives request handling as `async` futures on Tokio.
//! - Your domain stays in `Effect<A, E, R>` with environment `R` (often clone-cheap state with
//!   `Arc` fields, or a [`id_effect::Context`] stack).
//! - This crate **bridges** `State<R>` → `&mut R` for one build step, then runs the effect with
//!   [`id_effect_tokio::run_async`] so pending effect steps compose with Tokio I/O.
//!
//! ## Runtime requirements
//!
//! Workspace [`id_effect::Effect`] futures are intentionally **not** [`Send`], while Axum route
//! handlers must return [`Send`] futures. This crate runs each effect with
//! [`tokio::task::block_in_place`] and [`tokio::runtime::Handle::block_on`] so the `Effect` value never
//! crosses an async [`Send`] boundary. That requires a **multi-threaded** Tokio runtime (the default
//! for `#[tokio::main]`). On a `current_thread` runtime, prefer driving effects outside Axum or use a
//! dedicated integration. Note that `#[tokio::test]` defaults to **current-thread** and will panic
//! here; use `#[tokio::test(flavor = "multi_thread", worker_threads = 2)]` for router tests.
//!
//! ## Relation to `id_effect_tokio`
//!
//! **Yes — this crate is built on `id_effect_tokio`** (workspace crate `crates/id_effect_tokio`).
//! [`crate::routing`] and [`run_with_env`]/[`execute`] all drive effects via
//! [`id_effect_tokio::run_async`], so async steps inside `Effect` run on the **same** Tokio runtime as
//! `#[tokio::main]` / `axum::serve`.
//!
//! ## Quick start
//!
//! ```ignore
//! use axum::{Router, extract::State};
//! use id_effect::{succeed, Effect};
//! use id_effect_axum::routing;
//!
//! #[derive(Clone)]
//! struct AppState { /* … */ }
//!
//! fn hello(env: &mut AppState) -> Effect<String, std::convert::Infallible, AppState> {
//!   let _ = env;
//!   succeed("ok".to_string())
//! }
//!
//! let app = Router::new()
//!   .route("/hello", routing::get(hello))
//!   .with_state(AppState { /* … */ });
//! ```
//!
//! For full control (extra extractors, middleware ordering), use [`execute`].
//!
//! ## JSON + [`id_effect::schema`]
//!
//! Use [`json::decode_json_schema`] with [`Bytes`](axum::body::Bytes) (or any raw body) to validate
//! JSON via [`Schema::decode_unknown`](id_effect::schema::Schema::decode_unknown) (any wire type `I`).
//! Map failures with [`json::JsonSchemaError`] ([`IntoResponse`]) — schema errors become **422** with
//! `path` + `message`.
//!
//! ## Examples
//!
//! See `examples/` (e.g. `cargo run -p id_effect_axum --example 010_routing_hello`).

#![forbid(unsafe_code)]
#![deny(missing_docs)]
// Axum’s `Handler` trait and tower stack are `async fn` at the wire edge; domain logic stays in
// `Effect` via [`crate::routing`] and [`execute`].
#![allow(unknown_lints)]
#![allow(effect_no_async_fn_application)]

pub mod channel_bridge;
pub mod json;
pub mod routing;

pub use channel_bridge::{exchange, exchange_into_response};

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use id_effect::Effect;
use id_effect_tokio::run_async;

/// Run `build(&mut env)` to obtain an effect, then drive it to completion with the **same** `env`.
///
/// Uses the same [`tokio::task::block_in_place`] / [`tokio::runtime::Handle::block_on`]
/// strategy as [`crate::routing`] so the returned future is [`Send`] for Axum. See [crate
/// documentation](crate#runtime-requirements) for runtime requirements.
///
/// Use this when you already have `State(mut R)` or `&mut R` from custom extractors.
#[inline]
pub async fn run_with_env<S, A, E, F>(env: S, build: F) -> Result<A, E>
where
  S: Send + 'static,
  A: 'static,
  E: 'static,
  F: FnOnce(&mut S) -> Effect<A, E, S>,
{
  run_effect_from_state(env, build).await
}

/// Drives `build(&mut env)` to completion on the current Tokio runtime without storing [`Effect`] in
/// a [`Send`] async state machine (see [crate docs](crate#runtime-requirements)).
#[inline]
pub(crate) async fn run_effect_from_state<S, A, E, F>(mut env: S, build: F) -> Result<A, E>
where
  S: Send + 'static,
  A: 'static,
  E: 'static,
  F: FnOnce(&mut S) -> Effect<A, E, S>,
{
  tokio::task::block_in_place(move || {
    tokio::runtime::Handle::current().block_on(async move {
      let eff = build(&mut env);
      run_async(eff, env).await
    })
  })
}

/// Axum handler: extract [`State`]`<S>`, build an effect, map success and
/// failure through [`IntoResponse`].
#[inline]
pub async fn execute<S, A, E, F>(State(env): State<S>, build: F) -> Response
where
  S: Send + 'static,
  A: IntoResponse + Send + 'static,
  E: IntoResponse + Send + 'static,
  F: FnOnce(&mut S) -> Effect<A, E, S> + Send + 'static,
{
  match run_with_env(env, build).await {
    Ok(a) => a.into_response(),
    Err(e) => e.into_response(),
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use axum::body::Body;
  use axum::http::{Request, StatusCode};
  use axum::routing::Router;
  use id_effect::Metric;
  use id_effect::succeed;
  use tower::ServiceExt;

  #[derive(Clone, Default)]
  struct AppState {
    counter: std::sync::Arc<std::sync::atomic::AtomicU32>,
  }

  #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
  async fn routing_get_succeeds() {
    let app = Router::new()
      .route(
        "/",
        crate::routing::get(|s: &mut AppState| {
          s.counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
          succeed::<_, std::convert::Infallible, _>("hi".to_string())
        }),
      )
      .with_state(AppState::default());

    let req = Request::builder().uri("/").body(Body::empty()).unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
  }

  #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
  async fn axum_route_counter_increments_per_request() {
    let ctr = Metric::counter("axum_requests", std::iter::empty());
    let hist = Metric::histogram("axum_latency", std::iter::empty());

    #[derive(Clone, Default)]
    struct St;

    let app = Router::new()
      .route(
        "/",
        crate::routing::get_with_metrics(ctr.clone(), hist.clone(), |_s: &mut St| {
          succeed::<_, std::convert::Infallible, _>("ok".to_string())
        }),
      )
      .with_state(St);

    for _ in 0..2 {
      let req = Request::builder().uri("/").body(Body::empty()).unwrap();
      let res = app.clone().oneshot(req).await.unwrap();
      assert_eq!(res.status(), StatusCode::OK);
    }

    assert_eq!(ctr.snapshot_count(), 2);
    assert_eq!(hist.snapshot_durations().len(), 2);
  }

  #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
  async fn axum_channel_handler_responds_to_request() {
    use axum::body::Bytes;
    use axum::extract::State;
    use axum::http::Response as HttpResponse;
    use id_effect::channel::QueueChannel;
    use id_effect::{Queue, run_blocking};

    let q = run_blocking(Queue::unbounded(), ()).expect("queue");

    #[derive(Clone)]
    struct St {
      q: Queue<HttpResponse<Bytes>>,
    }

    let app = Router::new()
      .route(
        "/",
        axum::routing::get(|State(s): State<St>, req: Request<Body>| async move {
          let ch = QueueChannel::<HttpResponse<Bytes>, Request<Bytes>, ()>::from_queue_and_map(
            s.q.clone(),
            |req| HttpResponse::new(req.into_body()),
          );
          exchange_into_response((), ch, req).await
        }),
      )
      .with_state(St { q });

    let req = Request::builder()
      .uri("/")
      .body(Body::from("ping"))
      .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body();
    let bytes = http_body_util::BodyExt::collect(body)
      .await
      .unwrap()
      .to_bytes();
    assert_eq!(&bytes[..], b"ping");
  }

  #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
  async fn execute_handler() {
    let app = Router::new()
      .route(
        "/x",
        axum::routing::get(|st: State<AppState>| async move {
          execute(st, |_env| {
            succeed::<_, std::convert::Infallible, _>("42".to_string())
          })
          .await
        }),
      )
      .with_state(AppState::default());

    let req = Request::builder().uri("/x").body(Body::empty()).unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
  }
}
