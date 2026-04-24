# Migrating from `async fn` to effects

This appendix is a practical guide for converting existing async Rust code to id_effect. It covers common patterns and their id_effect equivalents, with migration steps for each.

## The Mental Model Shift

In typical async Rust, a function returns a `Future`; when that future is awaited, the work runs:

```rust
async fn get_user(id: u64, db: &DbClient) -> Result<User, DbError> {
    db.query_one("SELECT * FROM users WHERE id = $1", &[&id]).await
}
```

In id_effect, many domain functions return an **`Effect`**—a description you run later with an environment:

```rust
fn get_user<A, E, R>(id: u64) -> Effect<A, E, R>
where
    A: From<User> + 'static,
    E: From<DbError> + 'static,
    R: NeedsDb + 'static,
{
    effect!(|r: &mut R| {
        let db = ~ DbKey;
        let user = ~ db.get_user(id);
        A::from(user)
    })
}
```

The database client is no longer a function parameter. It's declared in `R` and retrieved by the runtime. The business logic is identical; what changes is how dependencies are supplied.

## Pattern 1: async fn → fn returning Effect

**Before**

```rust
pub async fn process_order(
    order_id: OrderId,
    db: &DbClient,
    mailer: &MailClient,
) -> Result<Receipt, AppError> {
    let order = db.get_order(order_id).await?;
    let receipt = db.complete_order(order).await?;
    mailer.send_receipt(&receipt).await?;
    Ok(receipt)
}
```

**After**

```rust
pub fn process_order<A, E, R>(order_id: OrderId) -> Effect<A, E, R>
where
    A: From<Receipt> + 'static,
    E: From<AppError> + 'static,
    R: NeedsDb + NeedsMailer + 'static,
{
    effect!(|r: &mut R| {
        let db     = ~ DbKey;
        let mailer = ~ MailerKey;
        let order   = ~ db.get_order(order_id);
        let receipt = ~ db.complete_order(order);
        ~ mailer.send_receipt(&receipt);
        A::from(receipt)
    })
}
```

**Migration steps:**

1. Remove the dependency parameters (`db`, `mailer`)
2. Add `<A, E, R>` generic parameters
3. Add `where` bounds for each removed dependency
4. Replace `async move { … }` with `effect!(|r: &mut R| { … })`
5. Replace `.await?` with `~ ` prefix
6. Wrap the return value with `A::from(…)`

## Pattern 2: Wrapping Third-Party Async

Third-party libraries return `Future`s, not `Effect`s. Use `from_async` to wrap them:

**Before**

```rust
async fn fetch_price(symbol: &str) -> Result<f64, reqwest::Error> {
    reqwest::get(format!("https://api.example.com/price/{symbol}"))
        .await?
        .json::<PriceResponse>()
        .await
        .map(|r| r.price)
}
```

**After**

```rust
fn fetch_price<A, E, R>(symbol: String) -> Effect<A, E, R>
where
    A: From<f64> + 'static,
    E: From<reqwest::Error> + 'static,
    R: 'static,
{
    from_async(move |_r| async move {
        let price = reqwest::get(format!("https://api.example.com/price/{symbol}"))
            .await?
            .json::<PriceResponse>()
            .await
            .map(|r| r.price)?;
        Ok(A::from(price))
    })
}
```

The `from_async` closure still uses `.await` internally. Only the outermost function signature changes.

## Pattern 3: Error Types

**Before** — single monolithic error enum

```rust
#[derive(Debug)]
enum AppError {
    DbError(DbError),
    MailError(MailError),
    NotFound(String),
}
```

**After** — effects propagate errors through `From` bounds

```rust
// Keep domain errors as-is
#[derive(Debug)] struct NotFoundError(String);

// Effect signatures declare what they can fail with:
fn get_user<A, E, R>(id: u64) -> Effect<A, E, R>
where
    E: From<DbError> + From<NotFoundError> + 'static, // …
```

You still need an `AppError` at the top level (in `main` or your HTTP handler), but individual functions no longer need to know about unrelated error variants.

## Pattern 4: Shared State

**Before** — `Arc<Mutex<T>>` passed through function calls

```rust
async fn handler(state: Arc<Mutex<AppState>>) -> Response {
    let mut s = state.lock().unwrap();
    s.request_count += 1;
    // …
}
```

**After** — shared state in a service, accessed via `R`

```rust
service_key!(AppStateKey: Arc<Mutex<AppState>>);

fn handler<A, E, R>() -> Effect<A, E, R>
where
    R: NeedsAppState + 'static,
    // …
{
    effect!(|r: &mut R| {
        let state = ~ AppStateKey;
        let mut s = state.lock().unwrap();
        s.request_count += 1;
        // …
    })
}
```

Or, for mutable state that needs transactional semantics across fibers, use `TRef`:

```rust
// Replace Arc<Mutex<Counter>> with TRef<u64>
service_key!(CounterKey: TRef<u64>);

fn increment_counter<E, R>() -> Effect<u64, E, R>
where
    R: NeedsCounter + 'static,
    E: 'static,
{
    effect!(|r: &mut R| {
        let counter = ~ CounterKey;
        ~ commit(counter.modify_stm(|n| n + 1));
        ~ commit(counter.read_stm())
    })
}
```

## Pattern 5: Resource Cleanup

**Before** — manual `drop` or relying on `Drop` impls

```rust
async fn with_connection<F, T>(pool: &Pool, f: F) -> Result<T, DbError>
where F: AsyncFnOnce(&Connection) -> Result<T, DbError>
{
    let conn = pool.get().await?;
    let result = f(&conn).await;
    // conn is dropped here — relies on Drop
    result
}
```

**After** — explicit `Scope`

```rust
fn with_connection<A, E, R, F>(f: F) -> Effect<A, E, R>
where
    F: FnOnce(&Connection) -> Effect<A, E, R> + 'static,
    R: NeedsPool + 'static,
    E: From<DbError> + 'static,
    A: 'static,
{
    effect!(|r: &mut R| {
        let pool = ~ PoolKey;
        ~ scope.acquire(
            pool.get(),           // acquire
            |conn| pool.release(conn),  // release (always runs)
            |conn| f(conn),       // use
        )
    })
}
```

The `Scope` finalizer runs whether the inner effect succeeds, fails, or is cancelled. `Drop` doesn't give you that guarantee for async code.

## HTTP boundaries: raw `reqwest` → workspace crates

After effects replace bare `async fn`, move HTTP edges toward **typed services**:

1. **`id_effect_platform`** — introduce **`HttpClient`** + **`HttpClientKey`** when you want **portable** requests and swap implementations (e.g. Wiremock-backed client in tests). Drive with **`id_effect_tokio::run_async`**.
2. **`id_effect_reqwest`** — when you want **`reqwest::Client`** keyed in **`R`**, **`RequestBuilder`**-style **`send`**, **pools**, or **`json_schema`** decoding—see [HTTP via reqwest](./part2/ch07-07-reqwest-http.md).

Host either style under **Axum** with **`id_effect_axum`** ([Axum host](./part2/ch07-08-axum-host.md)) so handlers stay thin and domain code stays in **`Effect`**.

## Migration Strategy

Migrate gradually, one module at a time:

1. Start with leaf functions (those with no id_effect dependencies yet) — convert them first.
2. Move up the call graph. Functions that call converted leaf functions become easy to convert.
3. Push the `run_blocking` call to `main` or the request handler entry point.
4. Convert tests last — once business logic is effect-based, tests become simple layer swaps.

You can mix old-style async functions and effect functions during the transition: wrap async functions with `from_async` and call effect functions with `run_blocking` in async contexts when needed.
