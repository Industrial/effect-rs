# Building Layers — From Simple to Complex

## Simple Layers: One Input, One Output

The simplest layer takes one service and produces another:

```rust
// Produces a database connection from config
let db_layer = LayerFn::new(|config: &Tagged<ConfigKey>| {
    effect! {
        let pool = ~ connect_pool(config.value().db_url());
        tagged::<DatabaseKey>(pool)
    }
});
```

## Layers That Need Nothing

A layer that builds from scratch (no inputs) uses `Nil` or `()` as its input type:

```rust
// Config layer — reads from environment variables, needs nothing
let config_layer = LayerFn::new(|_: &Nil| {
    effect! {
        let cfg = Config::from_env()?;
        tagged::<ConfigKey>(cfg)
    }
});
```

## Layers With Multiple Inputs

To require multiple services, the input type is a tuple of tagged values:

```rust
// Logger needs both Config and MetricsClient
let logger_layer = LayerFn::new(
    |env: &(Tagged<ConfigKey>, Tagged<MetricsKey>)| {
        effect! {
            let config  = env.0.value();
            let metrics = env.1.value();
            let logger  = Logger::new(config.log_level(), metrics.clone());
            tagged::<LoggerKey>(logger)
        }
    }
);
```

## Layers That Produce Multiple Values

A layer can produce a context with several services at once:

```rust
// Build both primary and replica database pools
let db_layers = LayerFn::new(|config: &Tagged<ConfigKey>| {
    effect! {
        let primary = ~ connect_pool(config.value().primary_url());
        let replica = ~ connect_pool(config.value().replica_url());
        ctx!(
            tagged::<PrimaryDbKey>(primary),
            tagged::<ReplicaDbKey>(replica),
        )
    }
});
```

## Memoization

By default, `LayerFn` builds fresh on every call. If you want the same instance shared across multiple dependents, wrap with `.memoize()`:

```rust
let shared_config = config_layer.memoize();
// Now every layer that depends on ConfigKey gets the same instance
```

Without memoization, if three layers need `ConfigKey`, `config_layer` would run three times. With `.memoize()`, it runs once and the result is cached for the lifetime of the build.

## The Pattern in Practice

A typical application has a handful of layers that form a pipeline:

```
config_layer (no input)
  → db_layer (needs config)
  → cache_layer (needs config)
  → service_layer (needs db + cache)
```

The next section shows how to wire them together.
