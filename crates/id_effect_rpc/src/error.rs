//! [`RpcError`] — structured JSON body + HTTP status + Axum [`IntoResponse`].

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::envelope::{RpcEnvelope, RpcErrorCode};

/// Structured RPC failure for Axum handlers at the `Effect` boundary.
#[derive(Debug, Clone)]
pub struct RpcError {
  status: StatusCode,
  envelope: RpcEnvelope,
}

impl RpcError {
  /// HTTP status returned to the client.
  #[inline]
  pub fn status(&self) -> StatusCode {
    self.status
  }

  /// JSON envelope (clone for logging or tests).
  #[inline]
  pub fn envelope(&self) -> &RpcEnvelope {
    &self.envelope
  }

  /// 400 — invalid client input.
  #[inline]
  pub fn invalid_argument(message: impl Into<String>) -> Self {
    Self {
      status: StatusCode::BAD_REQUEST,
      envelope: RpcEnvelope {
        code: RpcErrorCode::InvalidArgument,
        message: message.into(),
        correlation_id: None,
        details: None,
      },
    }
  }

  /// 404 — resource missing.
  #[inline]
  pub fn not_found(message: impl Into<String>) -> Self {
    Self {
      status: StatusCode::NOT_FOUND,
      envelope: RpcEnvelope {
        code: RpcErrorCode::NotFound,
        message: message.into(),
        correlation_id: None,
        details: None,
      },
    }
  }

  /// 409 — conflict / duplicate.
  #[inline]
  pub fn already_exists(message: impl Into<String>) -> Self {
    Self {
      status: StatusCode::CONFLICT,
      envelope: RpcEnvelope {
        code: RpcErrorCode::AlreadyExists,
        message: message.into(),
        correlation_id: None,
        details: None,
      },
    }
  }

  /// 401 — missing or bad credentials.
  #[inline]
  pub fn unauthenticated(message: impl Into<String>) -> Self {
    Self {
      status: StatusCode::UNAUTHORIZED,
      envelope: RpcEnvelope {
        code: RpcErrorCode::Unauthenticated,
        message: message.into(),
        correlation_id: None,
        details: None,
      },
    }
  }

  /// 500 — internal server error (do not leak secrets in `message`).
  #[inline]
  pub fn internal(message: impl Into<String>) -> Self {
    Self {
      status: StatusCode::INTERNAL_SERVER_ERROR,
      envelope: RpcEnvelope {
        code: RpcErrorCode::Internal,
        message: message.into(),
        correlation_id: None,
        details: None,
      },
    }
  }

  /// Attach correlation id to the envelope (typically from [`crate::correlation`]).
  #[must_use]
  #[inline]
  pub fn with_correlation_id(mut self, id: impl Into<String>) -> Self {
    self.envelope.correlation_id = Some(id.into());
    self
  }

  /// Attach structured details (e.g. validation errors).
  #[must_use]
  #[inline]
  pub fn with_details(mut self, details: serde_json::Value) -> Self {
    self.envelope.details = Some(details);
    self
  }
}

impl IntoResponse for RpcError {
  fn into_response(self) -> Response {
    (self.status, Json(self.envelope)).into_response()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use http_body_util::BodyExt;
  use serde_json::json;

  mod rpc_error {
    use super::*;

    mod into_response {
      use super::*;

      #[tokio::test]
      async fn not_found_returns_404_json_envelope() {
        let err = RpcError::not_found("nope").with_correlation_id("abc");
        let res = err.into_response();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        let body = res.into_body();
        let bytes = body.collect().await.unwrap().to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["code"], "not_found");
        assert_eq!(v["message"], "nope");
        assert_eq!(v["correlation_id"], "abc");
      }

      #[tokio::test]
      async fn internal_round_trip_envelope_matches_status() {
        let err = RpcError::internal("boom").with_details(json!({"hint": "retry"}));
        let res = err.clone().into_response();
        assert_eq!(res.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = res.into_body();
        let bytes = body.collect().await.unwrap().to_bytes();
        let env: RpcEnvelope = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(env, *err.envelope());
      }
    }

    mod status_mapping {
      use super::*;

      #[test]
      fn invalid_argument_is_bad_request() {
        let e = RpcError::invalid_argument("x");
        assert_eq!(e.status(), StatusCode::BAD_REQUEST);
        assert_eq!(e.envelope().code, RpcErrorCode::InvalidArgument);
      }

      #[test]
      fn unauthenticated_is_unauthorized() {
        let e = RpcError::unauthenticated("no token");
        assert_eq!(e.status(), StatusCode::UNAUTHORIZED);
      }

      #[test]
      fn already_exists_is_conflict() {
        let e = RpcError::already_exists("dup");
        assert_eq!(e.status(), StatusCode::CONFLICT);
      }
    }
  }
}
