# Laziness as a Superpower

So far we've established that `Effect<A, E, R>` is a *description* of a computation — a recipe that does nothing until someone executes it. You might be thinking: "OK, but why is that good? I have to run it *eventually*. What do I gain by waiting?"

Quite a bit, if your program benefits from composing and testing **before** execution.

Here is what you can do with a computation you have not run yet.

## Effect values vs driving an `async fn`

Rust futures are lazy: calling an `async fn` returns a `Future`; the body runs when that future is polled (for example with `.await`).

The contrast here is about **what your API returns**—a raw `Future` you must await immediately in the caller, versus an `Effect` value you can store, compose, and run later.

```rust
// Returns a Future; the HTTP work runs when this future is awaited / polled
async fn fetch_user_async(id: u64) -> Result<User, HttpError> {
    http_get(&format!("https://api.example.com/users/{id}")).await
}

// Returns a description; I/O runs when the effect is executed with an environment
fn fetch_user(id: u64) -> Effect<User, HttpError, HttpClient> {
    effect! {
        let user = ~ http_get(&format!("https://api.example.com/users/{id}"));
        user
    }
}
```

Calling `fetch_user_async(1)` only builds the future; the request runs when something polls it (typically at `.await`). Calling `fetch_user(1)` returns an `Effect`—still no I/O until you run that effect with a runner and the needed `HttpClient`.

The point is not that `async fn` is “eager.” It is that **effects give you a first-class value** to combine (retries, timeouts, tests) before you commit to a particular run.

## Superpower #1: Compose First, Run Later

Because effects are values, you can build an entire program before running any of it:

```rust
fn load_dashboard(user_id: u64) -> Effect<DashboardPage, AppError, (Database, Cache, Logger)> {
    effect! {
        let user    = ~ fetch_user(user_id).map_error(AppError::Db);
        let posts   = ~ fetch_posts(user.id).map_error(AppError::Db);
        let profile = ~ build_profile(&user, &posts).map_error(AppError::Render);
        profile
    }
}

// Nothing has run yet. We have a value.
let page = load_dashboard(42);

// Chain more work onto it — still nothing runs
let logged_page = page.flat_map(|p| log_view(p));

// Only now does any of this execute
run_blocking(logged_page.provide(env));
```

Every line before `run_blocking` is pure data manipulation. You're assembling a pipeline. The pipeline can be inspected, transformed, passed to other functions, stored in a struct. The laws of composition apply cleanly because there are no side-effects sneaking in.

## Superpower #2: Retry Without Rewriting

Because an effect is a description, you can wrap it with new behavior *without touching the original*:

```rust
let flaky = call_payment_api(order);

// Add exponential back-off retry — no changes to call_payment_api
let resilient = flaky.retry(Schedule::exponential(Duration::from_millis(100), 3));

// Add a timeout on top of that — still no changes
let bounded = resilient.timeout(Duration::from_secs(5));
```

Compare this to the async version: to add retries to an `async fn`, you'd either modify the function body, wrap it in a helper that calls it in a loop, or reach for an external crate. The retry logic gets *tangled with the business logic*.

With effects, retry is just another transformation. `retry` takes a lazy description, produces a new lazy description that runs the original up to N times. No surgery on the original required.

## Superpower #3: Test Without Mocking the Universe

Because nothing runs until you provide the environment, tests can substitute controlled implementations without rewriting a single line of production code:

```rust
#[test]
fn user_not_found_returns_error() {
    let test_env = TestEnv::new()
        .with_http(stub_http_404_for("/users/99"));

    let result = run_test(fetch_user(99), test_env);

    assert!(matches!(result, Err(HttpError::NotFound)));
}
```

The same `fetch_user` function used in production runs in the test — just against a different environment. No `#[cfg(test)]` stubs. No `Arc<dyn Trait>` that you only swap out in tests. The type system ensures you've provided every dependency the effect declared.

## Sequential async vs bundled descriptions

Sequential `async fn` code is natural for linear flows: each `.await` advances the next step, and control matches the source order.

Effect-oriented APIs often bundle those steps into a single `Effect` value first, then apply cross-cutting behavior (retry, timeout, tracing) as **transformations on that value** before calling `run_*`.

That separation is useful when the same workflow must be **reused** under different policies or **tested** with a substituted environment, without copying the body of the async function.

## When Does It Actually Run?

There are exactly three places where an `Effect` executes:

```rust
// In a binary or application entry point
run_blocking(program.provide(env));

// In an async context
run_async(program.provide(env)).await;

// In tests
run_test(program, test_env);
```

Everywhere else, you're building, transforming, or combining descriptions. The runtime boundary is explicit. You know exactly where the side-effects begin.

Until `run_*` is called, your effect is just data: composable and easy to substitute in tests.

---

That's Chapter 1. You now have a picture of **why** teams adopt effects (errors, dependencies, concurrency structure), **what** an `Effect` is (a description executed with an environment), what the type parameters mean (`A` = success, `E` = failure, `R` = requirements), and **why** keeping work in description form matters for composition and testing.

Chapter 2 gets hands-on: first effects, `map`, `flat_map`, and a small end-to-end program.
