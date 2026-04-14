//! Minimal JSON Schema (draft-07 style) fragments for **primitives** — not a full exporter for
//! composed [`crate::schema::parse::Schema`] values (those are closure-backed and not introspectable).
//!
//! Use for docs, OpenAPI-ish hints, and tests. Requires **`schema-serde`**.
//!
//! See [`TESTING.md`](../../../../TESTING.md).

use serde_json::{Value, json};

/// `"type": "string"`.
pub fn type_string() -> Value {
  json!({ "type": "string" })
}

/// `"type": "integer"` without `format`.
pub fn type_integer() -> Value {
  json!({ "type": "integer" })
}

/// `"type": "number"`.
pub fn type_number() -> Value {
  json!({ "type": "number" })
}

/// `"type": "boolean"`.
pub fn type_boolean() -> Value {
  json!({ "type": "boolean" })
}

/// `{ "type": "array", "items": items }`.
pub fn type_array(items: Value) -> Value {
  json!({ "type": "array", "items": items })
}

/// `{ "type": "object", "additionalProperties": value_schema }` for string-keyed records.
pub fn type_record(value_schema: Value) -> Value {
  json!({
    "type": "object",
    "additionalProperties": value_schema
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn string_fragment_is_stable() {
    assert_eq!(type_string(), json!({"type": "string"}));
  }

  #[test]
  fn primitive_fragments_match_draft_style() {
    assert_eq!(type_integer(), json!({"type": "integer"}));
    assert_eq!(type_number(), json!({"type": "number"}));
    assert_eq!(type_boolean(), json!({"type": "boolean"}));
  }

  #[test]
  fn array_and_record_wrap_items() {
    let items = type_string();
    assert_eq!(
      type_array(items.clone()),
      json!({ "type": "array", "items": items })
    );
    assert_eq!(
      type_record(type_boolean()),
      json!({
        "type": "object",
        "additionalProperties": type_boolean()
      })
    );
  }
}
