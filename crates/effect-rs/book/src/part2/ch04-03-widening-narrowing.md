# Widening and Narrowing — Environment Transformations

Sometimes your effect needs *part* of an environment, but you have the whole thing. Or you need to thread an effect through a context that provides more than required. This is where `zoom_env` and `contramap_env` come in.

## The Mismatch Problem

Imagine your application has a large environment type:

```rust
struct AppEnv {
    db: Database,
    logger: Logger,
    config: Config,
    metrics: MetricsClient,
}
```

You have a utility function that only needs a `Logger`:

```rust
fn log_event(msg: &str) -> Effect<(), LogError, Logger> { ... }
```

You can't call this inside an `effect!` block that has `AppEnv` in scope — the types don't match. You need to *narrow* the environment down.

## zoom_env: Narrow the Environment

`zoom_env` adapts an effect to work with a *larger* environment by providing a lens from the larger type to the smaller one:

```rust
// Adapt log_event to work with AppEnv
let app_log = log_event("hello").zoom_env(|env: &AppEnv| &env.logger);
```

Now `app_log` has type `Effect<(), LogError, AppEnv>`. The function extracts the `Logger` from `AppEnv` and feeds it to the original effect.

Inside `effect!`, the pattern looks like:

```rust
fn process(data: Data) -> Effect<(), AppError, AppEnv> {
    effect! {
        ~ log_event("start").zoom_env(|e: &AppEnv| &e.logger).map_error(AppError::Log);
        ~ db_query(data).zoom_env(|e: &AppEnv| &e.db).map_error(AppError::Db);
        Ok(())
    }
}
```

## contramap_env: Transform the Environment

While `zoom_env` narrows, `contramap_env` transforms. It applies a function to convert whatever environment the caller provides into what the effect actually needs:

```rust
// Effect needs a raw string URL
fn connect(url: &str) -> Effect<Database, DbError, String> { ... }

// You have a Config that contains the URL
let with_config = connect_raw.contramap_env(|cfg: &Config| cfg.db_url.clone());
// Now type is Effect<Database, DbError, Config>
```

`contramap_env` is the formal name for "adapt the environment type." In practice, most code uses `zoom_env` for the common case of extracting a field.

## R as Documentation Revisited

These combinators highlight why `R` is valuable as documentation. When you see:

```rust
fn log_event(msg: &str) -> Effect<(), LogError, Logger>
```

You know *exactly* what this function needs. You don't need to read its body to see if it also touches the database. The `zoom_env` call at the use site makes the adaptation explicit — it's not hidden.

Compare to the pre-effect alternative:

```rust
// Traditional: you'd need to read the body to know what `env` is used for
fn log_event(env: &AppEnv, msg: &str) -> Result<(), LogError> { ... }
```

With `R`, the function declares what it needs. With `zoom_env`, the caller declares how to satisfy it.

## When to Use These

In practice, `zoom_env` and `contramap_env` appear most often in *library code* — when writing reusable utilities that should work with any environment containing the right piece. Application code typically uses Layers and service tags (Chapters 5–6) which avoid the need for explicit projection.

Think of `zoom_env` as the manual fallback when the automatic layer-based wiring isn't the right fit.
