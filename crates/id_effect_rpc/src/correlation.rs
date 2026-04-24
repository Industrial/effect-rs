//! Correlation id helpers for RPC-style HTTP (`x-correlation-id`).

use axum::http::HeaderMap;
use axum::http::header::HeaderName;

/// Stable header name for request/response correlation (lowercase for intermediaries).
pub static CORRELATION_ID_HEADER: HeaderName = HeaderName::from_static("x-correlation-id");

/// Read correlation id from incoming headers when present and non-empty.
#[inline]
pub fn read_correlation_id(headers: &HeaderMap) -> Option<String> {
  headers
    .get(&CORRELATION_ID_HEADER)
    .and_then(|v| v.to_str().ok())
    .map(str::trim)
    .filter(|s| !s.is_empty())
    .map(str::to_owned)
}

/// Return existing correlation id or generate a new UUID v4 string.
#[inline]
pub fn ensure_correlation_id(headers: &HeaderMap) -> String {
  read_correlation_id(headers).unwrap_or_else(|| uuid::Uuid::new_v4().to_string())
}

/// Append `x-correlation-id` to outgoing response headers (idempotent append).
#[inline]
pub fn append_correlation_header(headers: &mut HeaderMap, id: &str) {
  if let Ok(v) = http::HeaderValue::from_str(id) {
    headers.append(CORRELATION_ID_HEADER.clone(), v);
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use axum::http::header::HeaderValue;

  mod read_correlation_id {
    use super::*;

    #[test]
    fn returns_none_when_header_absent() {
      let h = HeaderMap::new();
      assert!(read_correlation_id(&h).is_none());
    }

    #[test]
    fn returns_trimmed_value_when_present() {
      let mut h = HeaderMap::new();
      h.insert(
        CORRELATION_ID_HEADER.clone(),
        HeaderValue::from_static("  abc-123  "),
      );
      assert_eq!(read_correlation_id(&h).as_deref(), Some("abc-123"));
    }

    #[test]
    fn returns_none_when_empty_after_trim() {
      let mut h = HeaderMap::new();
      h.insert(
        CORRELATION_ID_HEADER.clone(),
        HeaderValue::from_static("   "),
      );
      assert!(read_correlation_id(&h).is_none());
    }
  }

  mod ensure_correlation_id {
    use super::*;

    #[test]
    fn preserves_existing_id() {
      let mut h = HeaderMap::new();
      h.insert(
        CORRELATION_ID_HEADER.clone(),
        HeaderValue::from_static("fixed"),
      );
      assert_eq!(ensure_correlation_id(&h), "fixed");
    }

    #[test]
    fn generates_uuid_when_missing() {
      let h = HeaderMap::new();
      let id = ensure_correlation_id(&h);
      assert_eq!(id.len(), 36);
      assert!(uuid::Uuid::parse_str(&id).is_ok());
    }
  }

  mod append_correlation_header {
    use super::*;

    #[test]
    fn appends_valid_value() {
      let mut h = HeaderMap::new();
      append_correlation_header(&mut h, "out-1");
      assert_eq!(
        h.get(&CORRELATION_ID_HEADER).and_then(|v| v.to_str().ok()),
        Some("out-1")
      );
    }
  }

  mod read_correlation_id_parameterized {
    use super::*;
    use axum::http::header::HeaderValue;
    use rstest::rstest;

    #[rstest]
    #[case::plain_token("token-1", Some("token-1"))]
    #[case::trimmed_inner("  spaced  ", Some("spaced"))]
    fn returns_expected_correlation(
      #[case] raw: &'static str,
      #[case] expect: Option<&'static str>,
    ) {
      let mut h = HeaderMap::new();
      let hv = HeaderValue::from_str(raw).expect("header value");
      h.insert(CORRELATION_ID_HEADER.clone(), hv);
      assert_eq!(read_correlation_id(&h).as_deref(), expect);
    }
  }
}
