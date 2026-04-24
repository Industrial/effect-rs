//! Axum JSON greet: validate body with [`id_effect::schema`] and return [`id_effect_rpc::RpcError`] on failure.
//!
//! Run (from repo root):
//!
//! ```bash
//! cargo run -p id_effect_rpc --features examples --example 010_json_greet
//! ```

use std::sync::Arc;

use axum::Router;
use axum::body::Bytes;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::post;
use id_effect::schema;
use id_effect_axum::json::decode_json_schema;
use id_effect_rpc::RpcError;
use id_effect_rpc::correlation::{append_correlation_header, ensure_correlation_id};
use id_effect_rpc::span::{record_request_metadata, rpc_request_span};

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() {
  let greet_schema = Arc::new(schema::struct_(
    "name",
    schema::string::<()>(),
    "enthusiasm",
    schema::i64::<()>(),
  ));

  let schema = greet_schema.clone();
  let app = Router::new().route(
    "/greet",
    post(
      move |headers: axum::http::HeaderMap, body: Bytes| async move {
        let cid = ensure_correlation_id(&headers);
        let span = rpc_request_span();
        let _enter = span.enter();
        record_request_metadata(&span, Some(cid.as_str()), "POST", "/greet", "greet");

        let (name, enthusiasm) = match decode_json_schema(schema.as_ref(), &body) {
          Ok(v) => v,
          Err(e) => return e.into_response(),
        };

        if enthusiasm < 0 {
          let err = RpcError::invalid_argument("enthusiasm must be non-negative")
            .with_correlation_id(cid.clone());
          return err.into_response();
        }

        let msg = format!("Hello, {name}! ({enthusiasm}x)");
        let mut res = (StatusCode::OK, msg).into_response();
        append_correlation_header(res.headers_mut(), &cid);
        res
      },
    ),
  );

  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let addr = listener.local_addr().unwrap();
  println!("Listening on http://{addr}/greet (POST JSON {{\"name\":\"Ada\",\"enthusiasm\":3}})");

  axum::serve(listener, app).await.unwrap();
}
