//! Integration-style wire tests: Axum + [`id_effect_rpc::RpcError`] JSON body.

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use http_body_util::BodyExt;
use id_effect_rpc::RpcError;
use tower::ServiceExt;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rpc_error_unauthenticated_round_trips_json_fields() {
  let app = Router::new().route(
    "/x",
    get(|| async move { RpcError::unauthenticated("missing bearer").with_correlation_id("z9") }),
  );

  let req = Request::builder().uri("/x").body(Body::empty()).unwrap();
  let res = app.oneshot(req).await.unwrap();
  assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
  let bytes = res.into_body().collect().await.unwrap().to_bytes();
  let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
  assert_eq!(v["code"], "unauthenticated");
  assert_eq!(v["message"], "missing bearer");
  assert_eq!(v["correlation_id"], "z9");
}
