//! Minimal [`Router`] + [`id_effect_axum::routing::get`]: state → `Effect` → HTTP response.
//!
//! Run: `cargo run -p id_effect_axum --example 010_routing_hello`

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::Router;
use http_body_util::BodyExt;
use id_effect::Effect;
use id_effect::succeed;
use id_effect_axum::routing;
use std::convert::Infallible;
use tower::ServiceExt;

#[derive(Clone)]
struct AppState {
  greeting: &'static str,
}

fn hello(env: &mut AppState) -> Effect<String, Infallible, AppState> {
  succeed(env.greeting.to_string())
}

#[tokio::main]
async fn main() {
  let app = Router::new()
    .route("/", routing::get(hello))
    .with_state(AppState {
      greeting: "hello from effect-axum",
    });

  let res = app
    .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
    .await
    .unwrap();

  assert_eq!(res.status(), StatusCode::OK);
  let body = res.into_body().collect().await.unwrap().to_bytes();
  assert_eq!(body.as_ref(), b"hello from effect-axum");
  println!("010_routing_hello ok");
}
