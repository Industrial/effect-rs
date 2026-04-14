# Stacking Layers — Composition Patterns

Individual layers do one thing. A real application needs them composed. effect-rs provides two composition patterns: sequential stacking and parallel merging.

## Sequential Stacking with .stack()

```rust
use effect::Stack;

let app_env = config_layer
    .stack(db_layer)       // Config → (Config, Database)
    .stack(logger_layer)   // → (Config, Database, Logger)
    .stack(service_layer); // → (Config, Database, Logger, Service)
```

Each `.stack()` takes the output of the previous layer and combines it with the new layer's output. The final type accumulates everything.

`.stack()` implies sequential ordering: `db_layer` runs after `config_layer` completes, because `db_layer` needs what `config_layer` produces.

## Parallel Merging with merge_all

When layers don't depend on each other, they can be built in parallel:

```rust
use effect::merge_all;

// These three layers are independent — build them concurrently
let monitoring = merge_all!(
    metrics_layer,
    tracing_layer,
    health_check_layer,
);
```

`merge_all!` takes a list of layers with the same input type and merges their outputs. If the inputs are available, all three build concurrently.

## Combining Stack and merge_all

In practice, you mix both:

```rust
let app_env = config_layer
    .stack(db_layer)
    .stack(
        // cache and redis are independent of each other but both need config+db
        merge_all!(cache_layer, redis_layer)
    )
    .stack(service_layer);
```

The build graph:
1. Build config
2. Build db (needs config)
3. Build cache and redis concurrently (both need config + db)
4. Build service (needs all of the above)

## Building and Providing

Once you have a composed layer, build it and provide it to an effect:

```rust
// Build all layers, get back a Context with everything
let env = app_env.build(()).await?;

// Provide to the effect and run
run_blocking(my_application().provide(env));
```

Or use `.provide_layer()` directly on an effect:

```rust
run_blocking(
    my_application()
        .provide_layer(app_env)
);
```

`.provide_layer()` builds the layer and provides its output in one step. This is the most common pattern at the application entry point.

## The type_only Pattern for Tests

Tests often want a subset of services. You can stack only what the test needs:

```rust
#[test]
fn test_user_service() {
    let test_layer = config_layer.stack(mock_db_layer);

    let result = run_test(
        get_user(1)
            .provide_layer(test_layer)
    );
    assert!(result.is_ok());
}
```

No need to build the full application stack. The test provides exactly what `get_user` requires — the type system enforces completeness.
