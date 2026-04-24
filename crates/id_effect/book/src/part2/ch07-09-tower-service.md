# Tower service (`id_effect_tower`)

Workspace crate **`id_effect_tower`** implements **[`tower::Service`](https://docs.rs/tower)** for **`Effect`**-based handlers. Effects are driven with **`id_effect_tokio::run_async`**, so you can compose **Tower middleware** (timeouts, retries, load balancing, …) around the same domain style as Axum, without tying to Axum’s router first.

## When to use it

- You already have a **Tower stack** (custom servers, gRPC gateways, middleware chains) and want **`poll_ready` / `call`** semantics.
- You need **per-service concurrency limits** or **request metrics** hooks at the Tower boundary.

## `EffectService`

- **`EffectService::new(state, f)`** — `f(&mut state, request)` returns an `Effect`; unlimited concurrency unless configured otherwise.
- **`with_max_in_flight`** — semaphore-gated **`poll_ready`** / **`call`** so at most *n* handler effects run concurrently; exposes an in-flight counter for observability.
- **`with_request_metrics`** — wraps each call with **`Metric::track_duration`** and increments an error counter on typed failure.

## Dependencies

The crate depends on **`id_effect_tokio`** for the async driver; it does not re-export **`TokioRuntime`**—compose runtime and layers at your application root.

## Further reading

- `cargo doc --open -p id_effect_tower`
- `moon run effect-tower:examples` or `cargo run -p id_effect_tower --example 001_effect_service`
- [Axum host](./ch07-08-axum-host.md) if you are on Axum specifically; [Tokio bridge](./ch07-05-tokio-bridge.md) for `Send` and runtime caveats.
