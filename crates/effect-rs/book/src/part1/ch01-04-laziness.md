# Laziness as a Superpower

So far we've established that `Effect<A, E, R>` is a *description* of a computation — a recipe that does nothing until someone executes it. You might be thinking: "OK, but why is that good? I have to run it *eventually*. What do I gain by waiting?"

Everything.

Let's look at what you can do with a computation you haven't run yet.

## The Eager vs. Lazy Showdown

Consider two versions of the same operation:

```rust
// EAGER: fires an HTTP request the moment you call this
async fn fetch_user_eager(id: u64) -> Result<User, HttpError> {
    http_get(&format!("https://api.example.com/users/{id}")).await
}

// LAZY: builds a description; nothing happens yet
fn fetch_user(id: u64) -> Effect<User, HttpError, HttpClient> {
    effect! {
        let user = ~ http_get(&format!("https://api.example.com/users/{id}"));
        user
    }
}
```

When you call `fetch_user_eager(1)`, the request goes out. Right now. Whether you wanted it to or not.

When you call `fetch_user(1)`, you get back a value — an `Effect<User, HttpError, HttpClient>`. The network is untouched. You're holding a description of an HTTP call, not the call itself.

This feels like a minor distinction. It isn't.

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

## The Philosophical Shift

Traditional async code operates in command mode: "Do this. Then do that. If it fails, do this other thing." Each step happens as you write it. Control and execution are interleaved.

Effect code operates in declaration mode: "Here is everything I want to accomplish, everything that can go wrong, and everything I need. I'm handing you a description. Run it when ready, run it however makes sense, run it under whatever conditions you impose."

You are not issuing orders. You are declaring intent.

This shift has a compounding effect (pun entirely intended). Once your whole codebase thinks in descriptions, every piece of infrastructure — retry, timeout, tracing, rate-limiting, circuit-breaking — can be added as a wrapper without touching the business logic it wraps. The concerns stay separate because the model keeps them separate.

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

Until `run_*` is called, your effect is just data. Beautiful, composable, testable data.

---

That's Chapter 1. You now know why effects exist (the Three Horsemen), what they are (descriptions, not actions), what the type parameters mean (`A` = success, `E` = failure, `R` = requirements), and why laziness is a feature rather than a quirk.

Chapter 2 gets hands-on. We'll write our first real effects, transform them with `map`, chain them with `flat_map`, and build a small program from scratch. Time to stop describing descriptions and start writing some.
