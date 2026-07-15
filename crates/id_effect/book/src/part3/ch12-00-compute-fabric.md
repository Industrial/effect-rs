# Compute Fabric

Every effect in id_effect passes through **Compute Fabric** at the runtime boundary. You write lazy `Effect` descriptions and declare a [`ResourcePolicy`](../../src/compute/policy.rs); Fabric decides where, when, and how concurrently work runs.

## Declarative policy

```rust,no_run
use id_effect::compute::{ResourcePolicy, MetricMode, MetricPolicy, RebalanceStrategy};

let policy = ResourcePolicy {
  memory: MetricPolicy::new(MetricMode::Max { ceiling: 0.85 }),
  cpu: MetricPolicy::new(MetricMode::Max { ceiling: 1.0 }),
  rebalance: RebalanceStrategy::ThrottleAdmission,
};
```

Memory capped at 85% while CPU may use all cores is:

```rust,no_run
let policy = ResourcePolicy::memory_cap_max_cpu(0.85);
```

Unlimited memory with even CPU spread across workers:

```rust,no_run
let policy = ResourcePolicy::unlimited_memory_cpu_spread(0.25);
```

## Supervisor loop

`ComputeSupervisor` polls `TelemetryEngine` (live sysinfo on host; mock in tests) and compares readings to policy:

1. **Monitor** — `TelemetrySnapshot { cpu_pct, mem_pct }`
2. **Admit** — `AdmissionController` adjusts permit count
3. **Place** — fibers on `FiberPool`, collections via Rayon, I/O via Tokio
4. **Rebalance** — throttle, shed, scale-out, or scale-in

Each tick emits a [`ComputeEvent`](../../src/observability/tracing.rs) (`SupervisorTick`) for observability alongside [`FiberEvent`](../../src/observability/tracing.rs).

Example: memory at 61% with an 85% ceiling → headroom → admit many fibers. Memory at 86% → throttle admission.

## Installing Fabric

```rust,no_run
use id_effect::{ThreadSleepRuntime, run_fork, succeed};
use id_effect::compute::ComputeFabric;

let fabric = ComputeFabric::memory_cap_max_cpu(0.85);
let rt = ThreadSleepRuntime::with_fabric(fabric);
// fibers spawned via rt use the shared pool + admission
```

[`run_with`](../../src/capability/run.rs) installs a default fabric for the duration of each application run. [`run_blocking`](../../src/runtime/execute.rs) refreshes the thread-local [`AdaptiveContext`](../../src/compute/adaptive.rs) on every entry.

See example `120_compute_fabric_memory_cap.rs`.

## Adaptive parallelism

[`AdaptiveContext`](../../src/compute/adaptive.rs) holds the current admission budget, Rayon thread count, and auto-parallel element threshold. Bulk dispatch via [`parallel_if_profitable`](../../src/compute/dispatch.rs) consults `should_parallelize_current(len)` instead of a fixed 1024 default when Fabric is installed.

- [`configure_rayon_threads`](../../src/compute/rayon_pool.rs) — supervisor-sized Rayon pool
- [`CpuSpreadBucket`](../../src/compute/spread.rs) — token bucket for `MetricMode::Spread`
- Example: `121_compute_fabric_cpu_spread.rs`

See also [Implicit Parallelism](../part4/ch13-05-parallelism.md) for the full 0.4.0 API surface (`map` / `map_effect` + `*_serial`).

## Effect binds

Independent `~` steps in an `effect!` block may run concurrently when the Effect Dependency Graph finds no data or capability conflict. Dependent steps stay ordered. Use `#[effect(serial)]` to opt out.

Example: `122_compute_fabric_effect_parallel.rs` (`Stream::map_effect` under admission budget).

## Streams and collections

- Element threshold and Rayon pool size follow [`AdaptiveContext`](../../src/compute/adaptive.rs)
- [`Stream::map_effect`](../../src/streaming/stream.rs) caps concurrent effect mappers from the admission budget
- Pure chunk ops (`map`, `filter`, …) use the same Fabric snapshot via `parallel_if_profitable`
- CPU spread mode caps per-worker share via [`CpuSpreadBucket`](../../src/compute/spread.rs)

## Cluster

When local Fabric saturates, `RebalanceStrategy::ScaleOut` builds a [`FabricJobSpec`](../../src/compute/cluster.rs) via [`ComputeSupervisor::scale_out`](../../src/compute/cluster.rs) (or `scale_out_placed`, which records a `target_node`). Convert with `id_effect_jobs::JobSpec::from_fabric` and enqueue on a [`FabricJobRunner`](../../id_effect_jobs/src/runner.rs), which routes each job to its `target_node` via `dequeue_for` / `pending_for`. Durable cross-node steps hook through [`DistributedStepJournal`](../../id_effect_workflow/src/journal.rs), backed by a real `RemoteStepJournal` over a `JournalTransport` (0.5.0).

Example: `123_compute_fabric_cluster.rs` (two-node offload with `MemoryJobRunner`).

[`ClusterResourcePolicy`](../../src/compute/cluster.rs) combines global and per-node caps with [`PlacementMode`](../../src/compute/cluster.rs). All placement decisions flow through one pure entry point, [`place`](../../src/compute/cluster.rs), fed by [`LoadReport`](../../src/compute/cluster.rs) telemetry (adapted to `NodeCandidate`):

| `PlacementMode` | Chooses |
|-----------------|---------|
| `Affinity` | an explicitly preferred node when eligible |
| `LocalFirst` | the least-loaded node (headroom-weighted) |
| `Spread` | the node with the lowest raw load |
| `RoundRobin` | the next node in a deterministic rotation |

As of 0.5.0 `LocalFirst` and `Spread` diverge — previously they scored identically. The distributed supervisor, mesh remote-spawn, and jobs offload all consume `place`.

## Further reading

- [ADR 0007](../../../../docs/adrs/0007-compute-fabric.md)
- [ADR 0008](../../../../docs/adrs/0008-implicit-parallelism.md) — Fabric-only parallelism (0.4.0)
- [Implicit Parallelism](../part4/ch13-05-parallelism.md)
- [Concurrency](./ch09-00-concurrency.md)
- [Scheduling](./ch11-00-scheduling.md) — temporal vs compute scheduling
