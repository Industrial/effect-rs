//! Unified interpreter-style effects, piping, and **capability-first** dependency injection.
//!
//! - **[`mod@kernel`]** — unified [`Effect<A, E, R>`] plus [`into_bind`].
//! - **[`effect!`](macro@effect)** — procedural do-notation (`x ~ expr` bind, `~ expr` discard), tail `Ok(expr)`; see [`macros`].
//! - **[`mod@macros`]** — declarative macros ([`pipe!`](macro@pipe), [`require!`](macro@require), …).
//! - **[`mod@capability`]** — trait-first DI: [`Env`], [`ProviderSpec`], [`CapabilityGraph`], [`run_with`].
//! - **[`mod@piping`]** — [`Pipe`] trait (macro: [`pipe!`](macro@pipe)).
//! - **[`schedule`]** — Effect.ts-style repeat/retry policies.
//! - **[`stream`]** — Effect.ts-inspired stream combinators.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
// CI runs `cargo doc` with `RUSTDOCFLAGS=-D warnings`; allow rustdoc lints until intra-doc links are
// normalized (ambiguous modules, redundant explicit targets, etc.).
#![allow(rustdoc::all)]
// `effect-dylint-rules` targets crates that *compose* `Effect` at the edge; this crate implements
// the runtime (`Effect::new`, internal `run_blocking`, …). Lints are crate-allowed here; see
// `.cursor/skills/effect.rs-fundamentals/SKILL.md` for caller-facing rules.
#![allow(unknown_lints)]
#![allow(
  effect_success_should_be_fn_type_param,
  effect_no_effect_suffix_on_graph_builder,
  effect_run_blocking_outside_boundary,
  effect_run_async_outside_boundary,
  effect_prefer_from_async_over_new_async,
  effect_from_async_single_await,
  effect_effect_generics_need_bounds,
  effect_multiple_top_level_effect_macros,
  effect_returning_effect_should_use_effect_macro,
  effect_no_async_fn_application,
  clippy::doc_overindented_list_items,
  clippy::should_implement_trait,
  clippy::type_complexity,
  clippy::unnecessary_lazy_evaluations
)]
// `#[cfg(test)]` law tests and similar intentionally exercise patterns Clippy flags (`unit` values,
// `clone` on `Copy` `Result`, etc.); keep `cargo clippy -p id_effect --all-targets -- -D warnings` green.
#![cfg_attr(
  test,
  allow(
    clippy::bool_assert_comparison,
    clippy::clone_on_copy,
    clippy::empty_line_after_doc_comments,
    clippy::let_unit_value,
    clippy::map_identity,
    clippy::redundant_closure,
    clippy::unit_arg,
    clippy::unit_cmp,
  )
)]

// Lets `::id_effect::…` in `id_effect_macro` expansions resolve when those macros are used inside this crate.
extern crate self as id_effect;

pub use id_effect_macro::{caps, err, mock_capability, pipe, provide, providers, require};
pub use id_effect_proc_macro::{
  EffectData, Fsm, Optics, ProviderSpec as ProviderSpecDerive, SchemaParser, capability, effect,
  effect_tagged, match_effect,
};

pub mod algebra;
pub mod capability;
pub mod collections;
pub mod compute;
pub mod concurrency;
pub mod coordination;
pub mod failure;
pub mod foundation;
pub mod kernel;
pub mod macros;
pub mod match_;
pub mod observability;
pub mod parallelism;
pub mod resource;
pub mod runtime;
pub mod scheduling;
pub mod schema;
pub mod stm;
pub mod streaming;
pub mod testing;

pub use crate::kernel::{
  BoxFuture, Effect, IntoBind, acquire_release, box_future, fail, flatten_or, from_async,
  into_bind, join_binds2, join_binds3, join_binds4, pure, scope_with, scoped, succeed,
};
pub use crate::match_::{HasTag, Matcher};
pub use capability::{
  Cap, CapBind, CapBindR, CapBindWide, CapKeys, CapList, CapWiden, Capability,
  CapabilityDiagnostic, CapabilityError, CapabilityGraph, CapabilityId, CapabilityKey,
  CapabilityPlannerError, CapabilitySet, Caps, Env, FromEnv, HasCap, Needs, NoCaps, PlannerNode,
  PlannerPlan, Provider, ProviderBox, ProviderError, ProviderNode, ProviderSpec, RunError,
  build_env, cap_into_bind, plan_topological, run, run_with, with_fiber_and_override,
  with_override,
};
pub use collections::{
  ChunkBuilder, EffectHashMap, EffectHashSet, EffectSortedMap, EffectSortedSet, EffectVector,
  MutableHashMap, MutableHashSet, MutableList, MutableQueue, RedBlackTree, Trie,
};
pub use compute::{
  AdaptiveContext, AdmissionController, ClusterResourcePolicy, ComputeFabric, ComputeSupervisor,
  CpuSpreadBucket, FabricJobSpec, FiberPool, LoadReport, MetricMode, MetricPolicy, MockTelemetry,
  PlacementMode, RebalanceStrategy, ResourcePolicy, SysinfoTelemetry, TelemetryEngine,
  TelemetrySnapshot, WorkProfile, configure_rayon_threads, ensure_run_context, install_fabric,
  install_parallel, place, refresh_adaptive_context,
};
pub use concurrency::{
  CancellationToken, FiberHandle, FiberId, FiberRef, FiberStatus, RestartIntensity, Supervisor,
  SupervisorPolicy, check_interrupt, fiber_all, fiber_never, fiber_succeed, interrupt_all,
  supervised, with_fiber_id,
};
pub use coordination::semaphore::Permit;
pub use coordination::{
  Channel, ChannelReadError, Deferred, FnRequestResolver, Latch, MissingKey, PubSub, Queue,
  QueueChannel, QueueError, Ref, RequestEntry, RequestResolver, Semaphore, SubscriptionRef,
  SynchronizedRef, batching, make_request_resolver,
};
pub use failure::{Cause, Exit, Or};
#[doc(inline)]
pub use foundation::either::Either;
pub use foundation::func::{
  always, compose, const_, flip, identity, memoize, pipe1, pipe2, pipe3, tupled, untupled,
};
pub use foundation::mutable_ref::MutableRef;
pub use foundation::piping::Pipe;
pub use foundation::predicate::Predicate;
pub use observability::{
  AnnotateCurrentSpanErr, AnnotateCurrentSpanSuccess, ComputeEvent, EffectEvent, FiberEvent,
  LogSpan, Metric, SpanRecord, TracingConfig, TracingFiberRefs, TracingSnapshot,
  annotate_current_span, emit_compute_event, emit_effect_event, emit_fiber_event,
  install_tracing_layer, metric_make, record_compute_event, snapshot_tracing, with_span,
};
pub use resource::{Cache, CacheStats, Finalizer, KeyedPool, Pool, Scope};
pub use runtime::{
  Never, Runtime, ThreadSleepRuntime, run_async, run_blocking, run_fork, yield_now,
};
pub use scheduling::{
  AnyDateTime, Clock, Duration, DurationParseError, LiveClock, Schedule, ScheduleDecision,
  ScheduleInput, TestClock, TimeUnit, UtcDateTime, ZonedDateTime, forever, repeat, repeat_n,
  repeat_with_clock, repeat_with_clock_and_interrupt, retry, retry_with_clock,
  retry_with_clock_and_interrupt, timezone,
};
pub use schema::brand::Brand;
pub use schema::data::{DataError, DataStruct, DataTuple, EffectData as EffectDataTrait};
pub use schema::equal::{EffectHash, Equal};
pub use schema::order::{DynOrder, Ordering, ordering};
pub use schema::{HasSchema, ParseError, ParseErrors, Redacted, Schema, Unknown};
pub use stm::{Outcome, Stm, TMap, TQueue, TRef, TSemaphore, Txn, atomically, commit};
pub use streaming::stream::{StreamBroadcastFanout, StreamChannelFull, StreamSender, StreamV1};
pub use streaming::{
  BackpressureDecision, BackpressurePolicy, Chunk, Sink, Stream, Transducer, backpressure_decision,
  broadcast_with_replay, combine_latest, end_stream, keyed_join, send_chunk, state_scan,
  stream_from_channel, stream_from_channel_with_policy, transducer_filter, transducer_map,
};
pub use testing::{
  assert_no_leaked_fibers, assert_no_unclosed_scopes, record_leaked_fiber, record_unclosed_scope,
  run_test, run_test_with_clock,
};

// ─── Backward-compatible module re-exports ────────────────────────────────────
// External crates that use `id_effect::channel::Channel` etc. keep working.
pub use coordination::channel;
pub use coordination::ref_;
pub use foundation::either;
pub use foundation::func;
pub use scheduling::clock;
pub use scheduling::duration;
pub use scheduling::schedule;
pub use schema::data;
pub use streaming::stream;
pub use testing::snapshot;

// ─── Re-export the im crate ───────────────────────────────────────────────────
// Callers can use `id_effect::im::Vector`, `id_effect::im::OrdSet`, etc. without
// adding a separate `im` dependency, and they're guaranteed version-compatible
// with all `id_effect::collections` types (which are type aliases of `im` types).
pub use im;

pub use algebra::{Alternative, Foldable, Invariant, sequence_vec, traverse_option, traverse_vec};
