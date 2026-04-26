//! Decode JSON request bodies with [`id_effect::schema::Schema`] and map [`ParseError`] to **422**
//! [`UNPROCESSABLE_ENTITY`](axum::http::StatusCode::UNPROCESSABLE_ENTITY) with a structured body.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use id_effect::data::EffectData;
use id_effect::schema::serde_bridge::unknown_from_serde_json;
use id_effect::schema::{ParseError, Schema, Unknown};
use rayon::prelude::*;
use serde::Serialize;
use serde_json::Value;

/// JSON-shaped view of a schema or syntax failure (for clients).
#[derive(Debug, Clone, Serialize)]
pub struct SchemaJsonErrorBody {
  /// JSON pointer–style path to the invalid value (empty for syntax errors).
  pub path: String,
  /// Human-readable error message for the client.
  pub message: String,
}

/// Failure while decoding a JSON body with [`decode_json_schema`].
#[derive(Debug)]
pub enum JsonSchemaError {
  /// `serde_json` could not parse the bytes as JSON ([`BAD_REQUEST`](StatusCode::BAD_REQUEST)).
  Syntax(String),
  /// [`Schema::decode_unknown`] failed ([`UNPROCESSABLE_ENTITY`](StatusCode::UNPROCESSABLE_ENTITY)).
  Schema(ParseError),
}

impl JsonSchemaError {
  fn status(&self) -> StatusCode {
    match self {
      JsonSchemaError::Syntax(_) => StatusCode::BAD_REQUEST,
      JsonSchemaError::Schema(_) => StatusCode::UNPROCESSABLE_ENTITY,
    }
  }

  fn body(&self) -> SchemaJsonErrorBody {
    match self {
      JsonSchemaError::Syntax(msg) => SchemaJsonErrorBody {
        path: String::new(),
        message: msg.clone(),
      },
      JsonSchemaError::Schema(p) => SchemaJsonErrorBody {
        path: p.path.clone(),
        message: p.message.clone(),
      },
    }
  }
}

impl IntoResponse for JsonSchemaError {
  fn into_response(self) -> Response {
    let status = self.status();
    let body = self.body();
    (status, Json(body)).into_response()
  }
}

/// Turn [`serde_json::Value`] into [`Unknown`] for [`Schema::decode_unknown`].
///
/// Delegates to [`id_effect::schema::serde_bridge::unknown_from_serde_json`] so JSON mapping stays
/// consistent with `effect-reqwest` and the core crate.
pub fn unknown_from_json(value: Value) -> Unknown {
  unknown_from_serde_json(value)
}

/// Parse `bytes` as JSON, convert to [`Unknown`], then run [`Schema::decode_unknown`].
///
/// The schema's wire type `I` is unused for this path (JSON is always parsed to [`Unknown`] first).
pub fn decode_json_schema<A, I, E>(
  schema: &Schema<A, I, E>,
  bytes: &[u8],
) -> Result<A, JsonSchemaError>
where
  E: EffectData + 'static,
  A: 'static,
  I: 'static,
{
  let value: Value =
    serde_json::from_slice(bytes).map_err(|e| JsonSchemaError::Syntax(e.to_string()))?;
  let unknown = unknown_from_json(value);
  schema
    .decode_unknown(&unknown)
    .map_err(JsonSchemaError::Schema)
}

/// Parse and validate many JSON payloads with the same [`Schema`] in parallel.
///
/// Results are in the same order as `bodies`. Use this for batch import or bulk validation where
/// each body is independent; for a single request body, use [`decode_json_schema`] instead.
pub fn decode_json_schema_batch_par<A, I, E>(
  schema: &Schema<A, I, E>,
  bodies: &[&[u8]],
) -> Vec<Result<A, JsonSchemaError>>
where
  E: EffectData + Send + Sync + 'static,
  A: Send + 'static,
  I: Send + Sync + 'static,
{
  bodies
    .par_iter()
    .map(|bytes| decode_json_schema(schema, bytes))
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;
  use axum::Router;
  use axum::body::Body;
  use axum::http::Request;
  use axum::routing::post;
  use http_body_util::BodyExt;
  use id_effect::schema;
  use std::sync::Arc;
  use tower::ServiceExt;

  #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
  async fn axum_handler_returns_422_with_field_path_on_bad_body() {
    let person = Arc::new(schema::struct_(
      "name",
      schema::string::<()>(),
      "age",
      schema::i64::<()>(),
    ));
    let schema = person.clone();
    let app = Router::new().route(
      "/person",
      post(move |body: axum::body::Bytes| async move {
        match decode_json_schema(schema.as_ref(), &body) {
          Ok((name, age)) => (StatusCode::OK, format!("{name}:{age}")).into_response(),
          Err(e) => e.into_response(),
        }
      }),
    );

    let bad = br#"{"name":"ada","age":"nope"}"#;
    let req = Request::builder()
      .method("POST")
      .uri("/person")
      .header(axum::http::header::CONTENT_TYPE, "application/json")
      .body(Body::from(bad.as_slice()))
      .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json body");
    let path = v.get("path").and_then(|x| x.as_str()).unwrap_or("");
    assert!(
      path.contains("age"),
      "expected path to mention age, got {path:?} full={v:?}"
    );
  }

  #[test]
  fn decode_json_schema_syntax_error_is_bad_request() {
    let schema = Arc::new(schema::string::<()>());
    let err = decode_json_schema(schema.as_ref(), b"not-json-at-all").expect_err("syntax");
    assert!(matches!(err, JsonSchemaError::Syntax(_)));
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
  }

  #[test]
  fn decode_json_schema_batch_par_preserves_order_and_mixed_results() {
    let schema = Arc::new(schema::struct_(
      "name",
      schema::string::<()>(),
      "age",
      schema::i64::<()>(),
    ));
    let b0 = br#"{"name":"ada","age":10}"#.as_slice();
    let b1 = br#"{"name":"bob","age":"x"}"#.as_slice();
    let b2 = br#"{"name":"carl","age":20}"#.as_slice();
    let bodies: &[&[u8]] = &[b0, b1, b2];
    let out = decode_json_schema_batch_par(schema.as_ref(), bodies);
    assert_eq!(out.len(), 3);
    let (n0, a0) = out[0].as_ref().expect("ok");
    assert_eq!(n0.as_str(), "ada");
    assert_eq!(*a0, 10);
    assert!(matches!(&out[1], Err(JsonSchemaError::Schema(_))));
    let (n2, a2) = out[2].as_ref().expect("ok");
    assert_eq!(n2.as_str(), "carl");
    assert_eq!(*a2, 20);
  }
}
