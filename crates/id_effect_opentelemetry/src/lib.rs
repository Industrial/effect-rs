//! OpenTelemetry integration for [`id_effect`]: tracing spans, W3C propagation, and metric bridges.
//!
//! This crate is the Phase B (`@effect/opentelemetry` parity) integration layer. It stays **opt-in**
//! at the dependency level: the core `id_effect` crate does not pull OpenTelemetry.
//!
//! ## Areas
//!
//! - **Span bridge** — compose [`id_effect::with_span`] with `tracing` spans exported to OTEL.
//! - **Propagation** — W3C Trace Context (`traceparent` / `tracestate`) on header maps.
//! - **Subscriber helpers** — build [`opentelemetry_sdk::trace::SdkTracerProvider`] for tests and apps.
//! - **Metric bridges** — dual-write [`id_effect::Metric`] instruments to OTEL, with optional
//!   parallel batch helpers (`apply_many_par`) for independent samples.
//!
//! ## Testing
//!
//! Prefer [`trace_subscriber_for_provider`] with [`tracing::subscriber::with_default`] in unit tests
//! so the global tracing dispatcher is not permanently claimed. See unit tests in each module.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod metrics_bridge;
mod propagation;
mod span_bridge;
mod subscriber;

pub use metrics_bridge::{CounterBridge, DurationHistogramBridge};
pub use propagation::{
  extract_trace_context_from_headers, inject_trace_context_into_headers,
  install_w3c_trace_context_propagator,
};
pub use span_bridge::with_span_otel;
pub use subscriber::{
  register_global_tracer_provider, sdk_tracer_provider_with_in_memory_exporter,
  trace_subscriber_for_provider, try_init_global_tracing_with_otel,
};
