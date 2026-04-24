# Challenges in Large Async Codebases

Async Rust gives you non-blocking I/O and structured concurrency primitives. In production, the same strengths can become painful when **composition** and **boundaries** are not planned: errors, dependencies, and spawned work all tend to accumulate complexity.

This section is not a claim that “async is broken.” It is a concise picture of problems **id_effect** is meant to help with—so the rest of the book has a shared vocabulary.

## Challenge 1: Error mapping and noise

A typical async workflow chains several operations. Each step may fail in its own way, so you map errors into a domain type and propagate:

```rust
async fn process_order(order: Order) -> Result<Receipt, ProcessError> {
    let config = get_config()
        .await
        .map_err(|e| ProcessError::Config(e))?;

    let user = fetch_user(&config, order.user_id)
        .await
        .map_err(|e| ProcessError::User(e))?;

    let inventory = check_inventory(&config, &order.items)
        .await
        .map_err(|e| ProcessError::Inventory(e))?;

    let payment = charge_payment(&config, &user, order.total)
        .await
        .map_err(|e| ProcessError::Payment(e))?;

    let shipment = create_shipment(&config, &order, &user)
        .await
        .map_err(|e| ProcessError::Shipment(e))?;

    Ok(Receipt::new(order, payment, shipment))
}
```

The business steps are clear, but the `.map_err` noise is repetitive. The domain `ProcessError` enum often grows with every new integration. Policy (retries, fallbacks) may live in callers or ad hoc helpers, which makes behavior harder to see in one place.

**What effects add:** failure and recovery can be expressed as **transformations on a description** (for example `retry`, `map_error`, structured `Exit` types), so policies are easier to reuse and test without rewriting the core flow.

## Challenge 2: Explicit dependency parameters

Another common shape is the handler that needs many clients and cross-cutting services:

```rust
async fn handle_request(
    db: &DatabasePool,
    cache: &RedisClient,
    logger: &Logger,
    config: &AppConfig,
    metrics: &MetricsClient,
    tracer: &Tracer,
    request: Request,
) -> Response {
    // ...
}
```

Dependencies are explicit, which is good for honesty, but every layer between here and `main` must repeat or forward them. Tests must build or mock the same bundle repeatedly. Alternatives (globals, implicit context) trade one problem for another.

**What effects add:** required capabilities can be expressed in **`R`** (the environment type) and satisfied in one place at the edge, while inner functions stay focused on logic.

## Challenge 3: Background work and lifetimes

Fire-and-forget background tasks are easy to start and harder to reason about:

```rust
fn start_background_worker(db: DatabasePool) {
    tokio::spawn(async move {
        loop {
            match process_queue(&db).await {
                Ok(_) => {}
                Err(e) => eprintln!("Worker error: {}", e),
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });
}
```

Questions that matter in production—shutdown, cancellation, panic behavior, and resource cleanup—need explicit design. That is true in any async system; the goal is to make ownership and intent **visible** in the program structure.

**What effects add:** structured concurrency patterns (fibers, scopes, handles) integrate with the same `Effect` abstraction so “what runs” and “how it ends” can be expressed consistently.

## How this relates to `Future` and `async`

Remember: **`async fn` bodies compile to `Future`s; nothing runs until a future is polled** (for example via `.await` on an async caller). The difficulties above are usually about **how we organize** async code—signatures, error types, and where side effects are allowed—not about rejecting the `Future` model.

In practice, hand-written async often **reads** like a straight-line script: await one step, then the next. That is appropriate for many functions. It becomes harder when you want the **same logical workflow** to be inspected, wrapped (retries, timeouts), or tested with a **substituted environment** without threading mocks through every layer.

**Effects** push the “script” into a value: `Effect<A, E, R>` is a description that you **run** with `run_async`, `run_blocking`, or test harnesses—after you have composed and configured it.

That does not replace understanding executors or `Future`. It adds a **layer** for domain structure: answer type `A`, error type `E`, requirements `R`, and explicit execution.

Next we define what an `Effect` is in this library and how that description differs from calling `async fn` directly—without exaggerating either side.
