//! Bridge [`id_effect::with_span`] with `tracing` spans so OTEL exporters see effect-scoped work.

use id_effect::{Effect, box_future};
use tracing::Instrument;

/// Runs `effect` under both [`id_effect::with_span`] (fiber-local stack + effect events) and a
/// `tracing` span (exported to OpenTelemetry when a [`tracing_subscriber`] with
/// [`tracing_opentelemetry`] is installed).
///
/// The `name` must match the `id_effect` API (`&'static str`) and is also attached as tracing
/// metadata under `otel.span_name`.
pub fn with_span_otel<A, E, R>(name: &'static str, effect: Effect<A, E, R>) -> Effect<A, E, R>
where
  A: 'static,
  E: 'static,
  R: 'static,
{
  let inner = id_effect::with_span(effect, name);
  Effect::new_async(move |env: &mut R| {
    let span = tracing::trace_span!("id_effect.effect", otel.span_name = name);
    box_future(async move { inner.run(env).instrument(span).await })
  })
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::subscriber::{
    sdk_tracer_provider_with_in_memory_exporter, trace_subscriber_for_provider,
  };
  use id_effect::{TracingConfig, install_tracing_layer, run_blocking, succeed};
  use opentelemetry_sdk::trace::InMemorySpanExporter;

  mod with_span_otel_when_tracing_stack_configured {
    use super::*;

    #[test]
    fn emits_tracing_span_exported_to_otel() {
      let exporter = InMemorySpanExporter::default();
      let provider = sdk_tracer_provider_with_in_memory_exporter(&exporter);
      let subscriber = trace_subscriber_for_provider(&provider, false);
      tracing::subscriber::with_default(subscriber, || {
        let _ = run_blocking(install_tracing_layer(TracingConfig::enabled()), ());
        let eff = with_span_otel("otel.inner", succeed::<(), (), ()>(()));
        let _ = run_blocking(eff, ());
      });
      let _ = provider.force_flush();
      let spans = exporter.get_finished_spans().expect("spans");
      assert!(
        spans.iter().any(|s| s.name == "id_effect.effect"),
        "expected id_effect.effect span, got {spans:?}"
      );
      let _ = provider.shutdown();
    }

    #[test]
    fn nests_inner_and_outer_spans() {
      let exporter = InMemorySpanExporter::default();
      let provider = sdk_tracer_provider_with_in_memory_exporter(&exporter);
      let subscriber = trace_subscriber_for_provider(&provider, false);
      tracing::subscriber::with_default(subscriber, || {
        let _ = run_blocking(install_tracing_layer(TracingConfig::enabled()), ());
        let inner = with_span_otel("inner", succeed::<(), (), ()>(()));
        let outer = with_span_otel("outer", inner);
        let _ = run_blocking(outer, ());
      });
      let _ = provider.force_flush();
      let spans = exporter.get_finished_spans().expect("spans");
      let effect_spans: Vec<_> = spans
        .iter()
        .filter(|s| s.name == "id_effect.effect")
        .collect();
      assert!(
        effect_spans.len() >= 2,
        "expected nested effect spans, got {effect_spans:?}"
      );
      let _ = provider.shutdown();
    }
  }
}
