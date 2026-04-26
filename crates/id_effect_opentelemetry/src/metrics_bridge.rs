//! Bridges [`id_effect::Metric`] instruments to OpenTelemetry metrics (MVP: counter + duration histogram).

use id_effect::Metric;
use id_effect::kernel::Effect;
use id_effect::runtime::{Never, run_blocking};
use id_effect::scheduling::Duration;
use opentelemetry::KeyValue;
use opentelemetry::metrics::{Histogram, Meter};
use opentelemetry_sdk::metrics::{InMemoryMetricExporter, PeriodicReader, SdkMeterProvider};
use rayon::prelude::*;

fn tags_to_kv(tags: &[(String, String)]) -> Vec<KeyValue> {
  tags
    .iter()
    .map(|(k, v)| KeyValue::new(k.clone(), v.clone()))
    .collect()
}

/// Dual-writes [`Metric::counter`] updates and an OpenTelemetry `u64` counter.
#[derive(Clone)]
pub struct CounterBridge {
  local: Metric<u64, ()>,
  otel: opentelemetry::metrics::Counter<u64>,
}

impl CounterBridge {
  /// Builds a bridge from an existing `id_effect` counter and an OTEL instrument on `meter`.
  pub fn new(local: Metric<u64, ()>, meter: &Meter, otel_name: &'static str) -> Self {
    let otel = meter.u64_counter(otel_name).build();
    Self { local, otel }
  }

  /// Increments both the in-process counter and the OTEL counter by `delta`.
  pub fn apply(&self, delta: u64) -> Effect<(), Never, ()> {
    let local = self.local.clone();
    let otel = self.otel.clone();
    let attrs = tags_to_kv(self.local.tags());
    Effect::new(move |_env| {
      run_blocking(local.apply(delta), ())?;
      otel.add(delta, attrs.as_slice());
      Ok(())
    })
  }

  /// Applies many counter deltas in parallel (same local+OTEL dual-write semantics as [`apply`]).
  ///
  /// Results preserve batch semantics by waiting for all work and then folding completion in slice
  /// order; use [`apply`] when strict per-update ordering is required.
  pub fn apply_many_par(&self, deltas: &[u64]) -> Effect<(), Never, ()> {
    let local = self.local.clone();
    let otel = self.otel.clone();
    let attrs = tags_to_kv(self.local.tags());
    let deltas = deltas.to_vec();
    Effect::new(move |_env| {
      deltas
        .par_iter()
        .map(|delta| {
          run_blocking(local.apply(*delta), ())?;
          otel.add(*delta, attrs.as_slice());
          Ok::<(), Never>(())
        })
        .collect::<Result<Vec<_>, Never>>()?;
      Ok(())
    })
  }

  /// Snapshot of the `id_effect` counter (OTEL side is observed via exporters).
  #[inline]
  pub fn snapshot_local(&self) -> u64 {
    self.local.snapshot_count()
  }
}

/// Dual-writes [`Metric`] duration histogram observations and an OTEL `f64` histogram (milliseconds).
#[derive(Clone)]
pub struct DurationHistogramBridge {
  local: Metric<Duration, ()>,
  otel: Histogram<f64>,
}

impl DurationHistogramBridge {
  /// Builds a bridge from an `id_effect` histogram and an OTEL histogram on `meter`.
  pub fn new(local: Metric<Duration, ()>, meter: &Meter, otel_name: &'static str) -> Self {
    let otel = meter.f64_histogram(otel_name).build();
    Self { local, otel }
  }

  /// Records the same duration sample on both sides (`id_effect` + OTEL, as milliseconds).
  pub fn apply(&self, sample: Duration) -> Effect<(), Never, ()> {
    let local = self.local.clone();
    let otel = self.otel.clone();
    let attrs = tags_to_kv(self.local.tags());
    Effect::new(move |_env| {
      run_blocking(local.apply(sample), ())?;
      let ms = sample.as_secs_f64() * 1_000.0;
      otel.record(ms, attrs.as_slice());
      Ok(())
    })
  }

  /// Records many duration samples in parallel (dual-writing `id_effect` + OTEL).
  ///
  /// As with [`CounterBridge::apply_many_par`], this favors throughput for independent samples over
  /// strict emission order.
  pub fn apply_many_par(&self, samples: &[Duration]) -> Effect<(), Never, ()> {
    let local = self.local.clone();
    let otel = self.otel.clone();
    let attrs = tags_to_kv(self.local.tags());
    let samples = samples.to_vec();
    Effect::new(move |_env| {
      samples
        .par_iter()
        .map(|sample| {
          run_blocking(local.apply(*sample), ())?;
          let ms = sample.as_secs_f64() * 1_000.0;
          otel.record(ms, attrs.as_slice());
          Ok::<(), Never>(())
        })
        .collect::<Result<Vec<_>, Never>>()?;
      Ok(())
    })
  }

  /// Snapshot of recorded durations on the `id_effect` side.
  #[inline]
  pub fn snapshot_local_durations(&self) -> Vec<Duration> {
    self.local.snapshot_durations()
  }
}

/// Test helper: meter provider with periodic export to memory.
///
/// This is primarily for tests and local spikes; it is not referenced from non-test library code,
/// so it may trigger `dead_code` in `cargo check` of the library target alone.
#[allow(dead_code)]
pub fn test_meter_provider_with_in_memory_exporter(
  exporter: &InMemoryMetricExporter,
) -> SdkMeterProvider {
  let reader = PeriodicReader::builder(exporter.clone()).build();
  SdkMeterProvider::builder().with_reader(reader).build()
}

#[cfg(test)]
mod tests {
  use super::*;
  use id_effect::run_blocking;
  use opentelemetry::metrics::MeterProvider;

  mod counter_bridge {
    use super::*;

    #[test]
    fn apply_updates_local_and_emits_otel_metric_after_flush() {
      let exporter = InMemoryMetricExporter::default();
      let mp = test_meter_provider_with_in_memory_exporter(&exporter);
      let meter = mp.meter("test");
      let local = Metric::counter("requests", Vec::<(String, String)>::new());
      let bridge = CounterBridge::new(local.clone(), &meter, "requests_otel");
      let _ = run_blocking(bridge.apply(3), ());
      let _ = mp.force_flush();
      assert_eq!(bridge.snapshot_local(), 3);
      let finished = exporter.get_finished_metrics().expect("metrics");
      assert!(
        !finished.is_empty(),
        "expected at least one ResourceMetrics after flush, got {finished:?}"
      );
      let _ = mp.shutdown();
    }

    #[test]
    fn apply_many_par_accumulates_local_counter() {
      let exporter = InMemoryMetricExporter::default();
      let mp = test_meter_provider_with_in_memory_exporter(&exporter);
      let meter = mp.meter("test");
      let local = Metric::counter("requests_batch", Vec::<(String, String)>::new());
      let bridge = CounterBridge::new(local.clone(), &meter, "requests_batch_otel");
      let _ = run_blocking(bridge.apply_many_par(&[1, 2, 3]), ());
      let _ = mp.force_flush();
      assert_eq!(bridge.snapshot_local(), 6);
      let _ = mp.shutdown();
    }
  }

  mod duration_histogram_bridge {
    use super::*;

    #[test]
    fn apply_records_on_both_sides() {
      let exporter = InMemoryMetricExporter::default();
      let mp = test_meter_provider_with_in_memory_exporter(&exporter);
      let meter = mp.meter("test");
      let local = Metric::histogram("latency", Vec::<(String, String)>::new());
      let bridge = DurationHistogramBridge::new(local.clone(), &meter, "latency_ms");
      let d = Duration::from_millis(12);
      let _ = run_blocking(bridge.apply(d), ());
      let _ = mp.force_flush();
      assert_eq!(bridge.snapshot_local_durations().len(), 1);
      let finished = exporter.get_finished_metrics().expect("metrics");
      assert!(!finished.is_empty());
      let _ = mp.shutdown();
    }

    #[test]
    fn apply_many_par_records_all_local_samples() {
      let exporter = InMemoryMetricExporter::default();
      let mp = test_meter_provider_with_in_memory_exporter(&exporter);
      let meter = mp.meter("test");
      let local = Metric::histogram("latency_batch", Vec::<(String, String)>::new());
      let bridge = DurationHistogramBridge::new(local.clone(), &meter, "latency_batch_ms");
      let samples = [Duration::from_millis(5), Duration::from_millis(7)];
      let _ = run_blocking(bridge.apply_many_par(&samples), ());
      let _ = mp.force_flush();
      assert_eq!(bridge.snapshot_local_durations().len(), 2);
      let _ = mp.shutdown();
    }
  }

  mod counter_bridge_with_tags {
    use super::*;

    #[test]
    fn forwards_tag_pairs_as_otel_attributes() {
      let exporter = InMemoryMetricExporter::default();
      let mp = test_meter_provider_with_in_memory_exporter(&exporter);
      let meter = mp.meter("test");
      let pairs = vec![("svc".to_string(), "api".to_string())];
      let local = Metric::counter("c", pairs);
      let bridge = CounterBridge::new(local, &meter, "c_otel");
      let _ = run_blocking(bridge.apply(1), ());
      let _ = mp.force_flush();
      let _ = mp.shutdown();
    }
  }
}
