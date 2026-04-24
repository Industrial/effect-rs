# `id_effect_opentelemetry`

Phase B integration: **OpenTelemetry** traces and metrics alongside `id_effect`’s built-in
[`with_span`](https://docs.rs/id_effect/latest/id_effect/fn.with_span.html) and
[`Metric`](https://docs.rs/id_effect/latest/id_effect/struct.Metric.html) helpers.

See the mdBook chapter **“OpenTelemetry (`id_effect_opentelemetry`)”** and
[`docs/effect-ts-parity/phases/phase-b-opentelemetry.md`](../../docs/effect-ts-parity/phases/phase-b-opentelemetry.md).

## Highlights

- **`with_span_otel`**: composes `id_effect::with_span` with a `tracing` span exported via OTEL.
- **W3C `traceparent`**: inject/extract helpers for portable HTTP header maps.
- **Metric bridges**: dual-write from `Metric` counters/histograms to OTEL instruments.
- **Test harness**: in-memory span exporter + scoped `tracing` subscriber (no global clashes).

## Usage sketch

```rust
use id_effect_opentelemetry::{
  install_w3c_trace_context_propagator, sdk_tracer_provider_with_in_memory_exporter,
  with_span_otel,
};
use id_effect::{run_blocking, succeed, install_tracing_layer, TracingConfig};

let exporter = opentelemetry_sdk::trace::InMemorySpanExporter::default();
let provider = sdk_tracer_provider_with_in_memory_exporter(&exporter);
let tracer = provider.tracer("my_app");
let layer = tracing_opentelemetry::layer().with_tracer(tracer);
let subscriber = tracing_subscriber::Registry::default().with(layer);

tracing::subscriber::with_default(subscriber, || {
  let _ = run_blocking(install_tracing_layer(TracingConfig::enabled()), ());
  let eff = with_span_otel("work", succeed::<u32, (), ()>(1));
  let _ = run_blocking(eff, ());
});
```

Production setups typically register a global tracer provider, OTLP exporters, and a
`tracing_subscriber` stack at process start; see the mdBook chapter for Axum + Tokio notes.
