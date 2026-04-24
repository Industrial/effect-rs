# API Quick Reference

A condensed reference for the most commonly used types and functions in id_effect. For full documentation, use `cargo doc --open -p id_effect`.

## Core Types

| Type | Description |
|------|-------------|
| `Effect<A, E, R>` | A computation that produces `A`, can fail with `E`, and requires environment `R` |
| `Stream<A, E, R>` | A sequence of `A` values that can fail with `E` and requires environment `R` |
| `Stm<A>` | A transactional computation that produces `A` |
| `Exit<A, E>` | The result of running an effect: `Success(A)` or `Failure(Cause<E>)` |
| `Cause<E>` | `Fail(E)`, `Die(Box<dyn Any>)`, or `Interrupt` |
| `Context<R>` | A heterogeneous map of services, the `R` at runtime |
| `Layer<Out, In, E>` | A recipe for building `Out` from `In`, can fail with `E` |
| `Chunk<A>` | A contiguous, reference-counted batch of `A` values |
| `Unknown` | Unvalidated wire data; input type for schemas |
| `ParseErrors` | Accumulated parse failures with paths |

## Constructors

| Function | Type | Notes |
|----------|------|-------|
| `succeed(a)` | `Effect<A, E, R>` | Always succeeds with `a` |
| `fail(e)` | `Effect<A, E, R>` | Always fails with typed error `e` |
| `pure(a)` | `Effect<A, Never, ()>` | Alias for `succeed`; `E = Never` |
| `from_async(f)` | `Effect<A, E, R>` | Lift an async closure |
| `effect!(…)` | `Effect<A, E, R>` | Do-notation macro |
| `commit(stm)` | `Effect<A, Never, ()>` | Run an STM transaction |
| `Stream::from_iter(i)` | `Stream<A, Never, ()>` | Stream from an iterator |
| `Stream::from_effect(e)` | `Stream<A, E, R>` | Single-element stream |
| `Stream::unfold_effect(s, f)` | `Stream<A, E, R>` | Generate stream from state |

## Effect Combinators

| Method | Notes |
|--------|-------|
| `.map(f)` | Transform success value |
| `.flat_map(f)` | Chain effects |
| `.map_err(f)` | Transform error |
| `.catch(f)` | Handle typed failure |
| `.catch_all(f)` | Handle any `Cause` |
| `.fold(on_e, on_a)` | Both paths to success |
| `.or_else(f)` | Try alternative on failure |
| `.ignore_error()` | Convert failure to `Option` |
| `.zip(other)` | Run two effects, tuple result |
| `.zip_left(other)` | Run two effects, keep left |
| `.zip_right(other)` | Run two effects, keep right |
| `.retry(schedule)` | Retry on failure |
| `.repeat(schedule)` | Repeat on success |
| `.timeout(dur)` | Fail with `Timeout` if too slow |

## Concurrency

| Function/Method | Notes |
|----------------|-------|
| `run_fork(rt, f)` | Spawn a fiber |
| `handle.join()` | `Effect` that waits for the fiber |
| `handle.interrupt()` | Cancel a fiber |
| `FiberRef::new(initial)` | Fiber-scoped dynamic variable |
| `fiber_ref.get()` | Read current fiber's value |
| `fiber_ref.set(v)` | Set current fiber's value |
| `with_fiber_id(id, f)` | Run `f` with a specific fiber id |
| `Supervisor::attach(scope)` | Fork child scope; cancel token when child closes |
| `supervised(&sup, policy, clock, make)` | Run `make` under restart / limit / ignore policy |
| `Supervisor::spawn(rt, …)` | `run_fork` + `supervised` on a worker |

## STM

| Function | Notes |
|----------|-------|
| `TRef::new(v)` | Create a transactional cell |
| `tref.read_stm()` | Read inside `stm!` |
| `tref.write_stm(v)` | Write inside `stm!` |
| `tref.modify_stm(f)` | Modify inside `stm!` |
| `commit(stm)` | Lift `Stm<A>` into `Effect<A, Never, ()>` |
| `atomically(stm)` | Execute `Stm` synchronously |
| `stm::retry()` | Block until any read `TRef` changes |
| `stm::fail(e)` | Abort transaction with error |
| `TQueue::bounded(n)` | Transactional FIFO queue |
| `TMap::new()` | Transactional hash map |
| `TSemaphore::new(n)` | Transactional semaphore |

## Resources

| Function | Notes |
|----------|-------|
| `scope.acquire(res, f)` | Use a resource, run finalizer on exit |
| `acquire_release(acq, rel)` | Bracket-style resource management |
| `Pool::new(size, factory)` | Reusable resource pool |
| `pool.get()` | `Effect` that borrows one resource |
| `Cache::new(loader)` | Cache backed by an effect |

## Scheduling

| Function | Notes |
|----------|-------|
| `Schedule::fixed(d)` | Repeat every `d` |
| `Schedule::exponential(base)` | Exponential backoff |
| `Schedule::linear(step)` | Linear backoff |
| `Schedule::immediate()` | No delay |
| `.take(n)` | At most `n` repetitions |
| `.until(pred)` | Stop when predicate holds |
| `eff.retry(sched)` | Retry with a schedule |
| `eff.repeat(sched)` | Repeat with a schedule |

## Running Effects

| Function | Notes |
|----------|-------|
| `run_blocking(eff, env)` | Synchronous runner (main/binaries) |
| `run_async(eff, env)` | Async runner (tokio integration) |
| `run_test(eff)` | Test harness; detects leaks |
| `run_test_and_unwrap(eff)` | Test harness; panics on failure |
| `run_test_with_env(eff, env)` | Test with custom environment |
| `run_test_with_clock(f)` | Test with controlled `TestClock` |

## Schema

| Function | Notes |
|----------|-------|
| `string()` | `Schema<String>` |
| `i64()` | `Schema<i64>` |
| `f64()` | `Schema<f64>` |
| `boolean()` | `Schema<bool>` |
| `optional(s)` | `Schema<Option<T>>` |
| `array(s)` | `Schema<Vec<T>>` |
| `struct_!(Type { … })` | Struct schema via macro |
| `refine(s, pred, msg)` | Add a predicate constraint |
| `parse(schema, unknown)` | Run schema; returns `Result<T, ParseErrors>` |
| `Unknown::from_json_str(s)` | Parse JSON into `Unknown` |
| `Unknown::from_serde_json(v)` | Convert `serde_json::Value` |

## Macros

| Macro | Notes |
|-------|-------|
| `effect!(…)` | Do-notation for effects; use `~expr` to bind |
| `ctx!(Key => value, …)` | Build a `Context` from key-value pairs |
| `service_key!(Name: Type)` | Declare a service key |
| `pipe!(v, f, g, …)` | Pipeline for pure values |

## Workspace crates (beyond `id_effect`)

| Crate | Role | Book | Docs |
|-------|------|------|------|
| `id_effect` | Core `Effect`, `Stream`, `Stm`, schema, fibers, … | Parts I–IV | `cargo doc -p id_effect` |
| `id_effect_tokio` | Tokio `Runtime`, `run_async` wiring, `spawn_blocking_run_async` | [Tokio bridge](./part2/ch07-05-tokio-bridge.md) | `cargo doc -p id_effect_tokio` |
| `id_effect_platform` | HTTP / FS / process **ports** + live + test impls | [Platform I/O](./part2/ch07-06-platform-services.md) | `cargo doc -p id_effect_platform` |
| `id_effect_reqwest` | `reqwest::Client` as a service; pools; JSON + schema | [HTTP via reqwest](./part2/ch07-07-reqwest-http.md) | `cargo doc -p id_effect_reqwest` |
| `id_effect_axum` | Axum handlers + `State<R>` bridge | [Axum host](./part2/ch07-08-axum-host.md) | `cargo doc -p id_effect_axum` |
| `id_effect_rpc` | RPC-style JSON errors, correlation ids, tracing spans | [RPC boundaries](./part2/ch07-12-rpc-boundaries.md) | `cargo doc -p id_effect_rpc` |
| `id_effect_tower` | `tower::Service` over effects | [Tower service](./part2/ch07-09-tower-service.md) | `cargo doc -p id_effect_tower` |
| `id_effect_config` | Config descriptors, Figment, provider in `R` | [Configuration](./part2/ch07-10-config.md) | `cargo doc -p id_effect_config` |
| `id_effect_logger` | Injectable `EffectLogger` | [Logging](./part2/ch07-11-logger.md) | `cargo doc -p id_effect_logger` |
| `id_effect_macro` / `id_effect_proc_macro` | `effect!` implementation split | [Workspace tooling](./appendix-d-workspace-tooling.md) | `cargo doc -p id_effect_macro` |
| `id_effect_lint` | Custom rustc lint (excluded from default workspace) | [Workspace tooling](./appendix-d-workspace-tooling.md) | build crate explicitly |

For a single local index, run `cargo doc --workspace --no-deps` from the repository root (see each crate’s `README` for optional examples).
