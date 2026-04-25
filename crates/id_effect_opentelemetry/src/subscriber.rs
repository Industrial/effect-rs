//! Tracer provider helpers and [`tracing_subscriber`] wiring for OpenTelemetry.

use opentelemetry::global;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_sdk::trace::{InMemorySpanExporter, SdkTracerProvider};
use tracing::Subscriber;
use tracing_subscriber::Registry;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// Builds an [`SdkTracerProvider`] that exports spans to an in-memory buffer (tests and spikes).
pub fn sdk_tracer_provider_with_in_memory_exporter(
  exporter: &InMemorySpanExporter,
) -> SdkTracerProvider {
  SdkTracerProvider::builder()
    .with_simple_exporter(exporter.clone())
    .build()
}

/// Registers `provider` as the process-wide OpenTelemetry tracer provider.
///
/// Call [`SdkTracerProvider::shutdown`] (or drop the last clone after shutdown) before replacing
/// the global provider in long-lived binaries.
pub fn register_global_tracer_provider(provider: &SdkTracerProvider) {
  global::set_tracer_provider(provider.clone());
}

/// Returns a boxed [`tracing`] [`Subscriber`]: registry + OpenTelemetry layer, optionally with `fmt`.
///
/// Use with [`tracing::subscriber::with_default`] in tests, or [`SubscriberInitExt::try_init`] once at
/// startup in binaries.
pub fn trace_subscriber_for_provider(
  provider: &SdkTracerProvider,
  with_fmt_layer: bool,
) -> Box<dyn Subscriber + Send + Sync + 'static> {
  let tracer = provider.tracer("id_effect_opentelemetry");
  let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);
  if with_fmt_layer {
    Box::new(
      Registry::default()
        .with(otel_layer)
        .with(tracing_subscriber::fmt::layer()),
    )
  } else {
    Box::new(Registry::default().with(otel_layer))
  }
}

/// Installs a global subscriber built from [`trace_subscriber_for_provider`].
///
/// Returns `Err` if a global default was already installed (mirrors [`tracing_subscriber`] rules).
pub fn try_init_global_tracing_with_otel(
  provider: &SdkTracerProvider,
  with_fmt_layer: bool,
) -> Result<(), tracing_subscriber::util::TryInitError> {
  trace_subscriber_for_provider(provider, with_fmt_layer).try_init()
}

#[cfg(test)]
mod tests {
  use super::*;
  use opentelemetry::trace::Tracer;

  mod sdk_tracer_provider_with_in_memory_exporter {
    use super::*;

    #[test]
    fn exports_finished_span_after_flush() {
      let exporter = InMemorySpanExporter::default();
      let provider = sdk_tracer_provider_with_in_memory_exporter(&exporter);
      let tracer = provider.tracer("unit");
      {
        let span = tracer.start("hello");
        drop(span);
      }
      let _ = provider.force_flush();
      let spans = exporter.get_finished_spans().expect("spans");
      assert!(
        spans.iter().any(|s| s.name == "hello"),
        "expected span named hello, got {spans:?}"
      );
      let _ = provider.shutdown();
    }
  }

  mod trace_subscriber_for_provider {
    use super::*;

    #[test]
    fn records_tracing_span_without_fmt_layer() {
      let exporter = InMemorySpanExporter::default();
      let provider = sdk_tracer_provider_with_in_memory_exporter(&exporter);
      let sub = trace_subscriber_for_provider(&provider, false);
      tracing::subscriber::with_default(sub, || {
        let root = tracing::info_span!("root_op");
        let _g = root.enter();
        tracing::info!(target: "id_effect_opentelemetry", "event");
      });
      let _ = provider.force_flush();
      let spans = exporter.get_finished_spans().expect("spans");
      assert!(
        spans.iter().any(|s| s.name == "root_op"),
        "expected tracing span, got {spans:?}"
      );
      let _ = provider.shutdown();
    }
  }
}
