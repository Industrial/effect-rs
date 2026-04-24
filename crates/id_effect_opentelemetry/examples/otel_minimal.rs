//! Minimal example: in-memory OTEL trace export + `with_span_otel`.
//!
//! Run: `cargo run -p id_effect_opentelemetry --example otel_minimal`

use id_effect::{TracingConfig, install_tracing_layer, run_blocking, succeed};
use id_effect_opentelemetry::{
  sdk_tracer_provider_with_in_memory_exporter, trace_subscriber_for_provider, with_span_otel,
};
use opentelemetry_sdk::trace::InMemorySpanExporter;

fn main() {
  let exporter = InMemorySpanExporter::default();
  let provider = sdk_tracer_provider_with_in_memory_exporter(&exporter);
  let subscriber = trace_subscriber_for_provider(&provider, true);
  let _guard = tracing::subscriber::set_default(subscriber);

  let _ = run_blocking(install_tracing_layer(TracingConfig::enabled()), ());
  let eff = with_span_otel("example.work", succeed::<(), (), ()>(()));
  let _ = run_blocking(eff, ());

  let _ = provider.force_flush();
  let spans = exporter.get_finished_spans().expect("read spans");
  println!("exported spans: {}", spans.len());
  let _ = provider.shutdown();
}
