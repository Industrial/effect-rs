//! W3C Trace Context propagation on simple `(String, String)` header maps (MVP for HTTP clients).

use opentelemetry::Context;
use opentelemetry::propagation::{Extractor, Injector};
use opentelemetry_sdk::propagation::TraceContextPropagator;

/// Installs the W3C Trace Context propagator as the global text-map propagator.
///
/// Safe to call once per process; subsequent calls replace the previous propagator.
pub fn install_w3c_trace_context_propagator() {
  opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new());
}

struct VecHeadersInjector<'a>(&'a mut Vec<(String, String)>);

impl Injector for VecHeadersInjector<'_> {
  fn set(&mut self, key: &str, value: String) {
    self.0.retain(|(k, _)| !k.eq_ignore_ascii_case(key));
    self.0.push((key.to_string(), value));
  }
}

/// Carrier for [`Extractor`] over immutable header slices.
pub struct VecHeadersExtractor<'a> {
  headers: &'a [(String, String)],
}

impl<'a> VecHeadersExtractor<'a> {
  /// Wraps a borrowed header list for extraction.
  pub fn new(headers: &'a [(String, String)]) -> Self {
    Self { headers }
  }
}

impl Extractor for VecHeadersExtractor<'_> {
  fn get(&self, key: &str) -> Option<&str> {
    self.headers.iter().find_map(|(k, v)| {
      if k.eq_ignore_ascii_case(key) {
        Some(v.as_str())
      } else {
        None
      }
    })
  }

  fn keys(&self) -> Vec<&str> {
    self.headers.iter().map(|(k, _)| k.as_str()).collect()
  }
}

/// Injects the current [`Context`] trace state into `headers` using the global text-map propagator.
pub fn inject_trace_context_into_headers(cx: &Context, headers: &mut Vec<(String, String)>) {
  let mut inj = VecHeadersInjector(headers);
  opentelemetry::global::get_text_map_propagator(|prop| prop.inject_context(cx, &mut inj));
}

/// Extracts a [`Context`] from `headers` using the global text-map propagator, starting from `base`.
pub fn extract_trace_context_from_headers(base: &Context, headers: &[(String, String)]) -> Context {
  let ext = VecHeadersExtractor::new(headers);
  opentelemetry::global::get_text_map_propagator(|prop| prop.extract_with_context(base, &ext))
}

#[cfg(test)]
mod tests {
  use super::*;
  use opentelemetry::trace::TracerProvider as _;
  use opentelemetry::trace::{TraceContextExt, Tracer};
  use opentelemetry_sdk::trace::{InMemorySpanExporter, SdkTracerProvider};

  mod round_trip {
    use super::*;

    #[test]
    fn inject_then_extract_preserves_remote_span_id() {
      install_w3c_trace_context_propagator();
      let exporter = InMemorySpanExporter::default();
      let provider = SdkTracerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();
      let tracer = provider.tracer("propagation_test");
      let span = tracer.start("remote");
      let cx = opentelemetry::Context::current_with_span(span);
      let mut headers = Vec::new();
      inject_trace_context_into_headers(&cx, &mut headers);
      assert!(
        headers
          .iter()
          .any(|(k, _)| k.eq_ignore_ascii_case("traceparent")),
        "expected traceparent header, got {headers:?}"
      );
      let extracted = extract_trace_context_from_headers(&Context::default(), &headers);
      let span_ctx = extracted.span().span_context().clone();
      assert!(span_ctx.is_valid(), "expected valid span context");
      let _ = provider.shutdown();
    }
  }

  mod vec_headers_extractor {
    use super::*;

    #[test]
    fn get_is_case_insensitive_for_header_name() {
      let headers = vec![(
        "TraceParent".to_string(),
        "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01".to_string(),
      )];
      let ext = VecHeadersExtractor::new(&headers);
      assert!(ext.get("traceparent").is_some());
    }
  }
}
