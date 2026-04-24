//! Tracing helpers for RPC-shaped HTTP handlers.
//!
//! Spans use stable field names so an OpenTelemetry layer on the subscriber can map them to
//! semantic conventions without this crate depending on OTEL crates directly.

use tracing::Span;

/// Root span for one RPC-style HTTP request. Record metadata with [`record_request_metadata`].
#[inline]
pub fn rpc_request_span() -> Span {
  tracing::info_span!(
    "rpc.request",
    correlation.id = tracing::field::Empty,
    http.method = tracing::field::Empty,
    http.route = tracing::field::Empty,
    rpc.operation = tracing::field::Empty,
  )
}

/// Fill standard RPC span fields (safe for high-cardinality: avoid raw URLs with query tokens).
#[inline]
pub fn record_request_metadata(
  span: &Span,
  correlation_id: Option<&str>,
  method: &str,
  route: &str,
  operation: &str,
) {
  if let Some(c) = correlation_id {
    span.record("correlation.id", c);
  }
  span.record("http.method", method);
  span.record("http.route", route);
  span.record("rpc.operation", operation);
}

#[cfg(test)]
mod tests {
  use super::*;

  mod rpc_request_span {
    use super::*;

    #[test]
    fn constructs_without_panicking() {
      let span = rpc_request_span();
      let _g = span.enter();
      record_request_metadata(&span, Some("c1"), "POST", "/greet", "greet");
    }

    #[test]
    fn allows_missing_correlation_id() {
      let span = rpc_request_span();
      let _g = span.enter();
      record_request_metadata(&span, None, "GET", "/health", "health");
    }
  }
}
