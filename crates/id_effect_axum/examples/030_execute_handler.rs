//! Use [`id_effect_axum::execute`] when the handler is already an `async` closure and you want to
//! compose extra Axum extractors or middleware around [`State`].
//!
//! Run: `cargo run -p id_effect_axum --example 030_execute_handler`

use axum::body::Body;
use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::routing::Router;
use http_body_util::BodyExt;
use id_effect::succeed;
use id_effect_axum::execute;
use std::convert::Infallible;
use tower::ServiceExt;

#[derive(Clone, Default)]
struct AppState {
  n: u32,
}

#[tokio::main]
async fn main() {
  let app = Router::new()
    .route(
      "/n",
      axum::routing::get(|st: State<AppState>| async move {
        execute(st, |env| {
          succeed::<String, Infallible, _>(format!("n={}", env.n))
        })
        .await
      }),
    )
    .with_state(AppState { n: 7 });

  let res = app
    .oneshot(Request::builder().uri("/n").body(Body::empty()).unwrap())
    .await
    .unwrap();

  assert_eq!(res.status(), StatusCode::OK);
  let body = res.into_body().collect().await.unwrap().to_bytes();
  assert_eq!(body.as_ref(), b"n=7");
  println!("030_execute_handler ok");
}
