//! JSON envelope for typed RPC-style failures at the HTTP boundary.

use serde::{Deserialize, Serialize};

/// Coarse error category for HTTP APIs (paired with an HTTP status in [`crate::RpcError`](crate::error::RpcError)).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RpcErrorCode {
  /// Client sent malformed or semantically invalid input (typically 400).
  InvalidArgument,
  /// Resource does not exist (404).
  NotFound,
  /// Resource already exists (409).
  AlreadyExists,
  /// Missing or invalid credentials (401).
  Unauthenticated,
  /// Server-side failure (500).
  Internal,
}

/// Wire body returned for structured RPC errors.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RpcEnvelope {
  /// Machine-readable category.
  pub code: RpcErrorCode,
  /// Human-readable summary for operators and clients.
  pub message: String,
  /// Echoes `x-correlation-id` when one was present or generated at the edge.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub correlation_id: Option<String>,
  /// Optional structured payload (validation details, etc.).
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub details: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;

  mod rpc_envelope {
    use super::*;

    mod round_trip_json {
      use super::*;

      #[test]
      fn preserves_code_message_correlation_and_details() {
        let env = RpcEnvelope {
          code: RpcErrorCode::InvalidArgument,
          message: "bad".to_owned(),
          correlation_id: Some("cid-1".to_owned()),
          details: Some(json!({"field": "age"})),
        };
        let bytes = serde_json::to_vec(&env).expect("serialize");
        let back: RpcEnvelope = serde_json::from_slice(&bytes).expect("deserialize");
        assert_eq!(back, env);
      }

      #[test]
      fn omits_null_optional_fields_on_wire() {
        let env = RpcEnvelope {
          code: RpcErrorCode::Internal,
          message: "x".to_owned(),
          correlation_id: None,
          details: None,
        };
        let v: serde_json::Value = serde_json::to_value(&env).expect("to_value");
        assert!(v.get("correlation_id").is_none());
        assert!(v.get("details").is_none());
      }
    }

    mod rpc_error_code {
      use super::*;

      #[test]
      fn serde_snake_case_round_trip() {
        let codes = [
          RpcErrorCode::InvalidArgument,
          RpcErrorCode::NotFound,
          RpcErrorCode::AlreadyExists,
          RpcErrorCode::Unauthenticated,
          RpcErrorCode::Internal,
        ];
        for code in codes {
          let s = serde_json::to_string(&code).expect("ser");
          let back: RpcErrorCode = serde_json::from_str(&s).expect("de");
          assert_eq!(back, code);
        }
      }
    }
  }
}
