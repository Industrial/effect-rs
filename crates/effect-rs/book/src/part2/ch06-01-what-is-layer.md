# What Is a Layer?

An `Effect` describes a computation that *needs* an environment. A `Layer` describes *how to build* part of that environment.

Think of it this way:

```
Effect<User, DbError, Database>
  └── "I need a Database to produce a User"

Layer<Database, ConfigError, Config>
  └── "I need a Config to produce a Database"
```

They're complementary. Effects declare their needs; Layers declare how to satisfy them.

## The Layer Type

```rust
// Layer<Output, Error, Input>
//        │       │      └── What I need to build
//        │       └───────── What can go wrong while building
//        └───────────────── What I produce
Layer<Tagged<DatabaseKey>, DbError, Tagged<ConfigKey>>
```

A layer that takes a `Tagged<ConfigKey>` and produces a `Tagged<DatabaseKey>`, possibly failing with `DbError`.

## A Simple Layer

```rust
use effect::{Layer, LayerFn, effect, tagged};

let db_layer: Layer<Tagged<DatabaseKey>, DbError, Tagged<ConfigKey>> =
    LayerFn::new(|config: &Tagged<ConfigKey>| {
        effect! {
            let pool = ~ connect_pool(config.value().db_url());
            tagged::<DatabaseKey>(pool)
        }
    });
```

`LayerFn::new` wraps an effectful constructor. The closure takes the required input (the config) and returns an effect that produces the output (the database connection).

## Layers as Values

Like effects, layers are lazy descriptions. Constructing a `LayerFn` does nothing. The actual connection only happens when the layer is *built* — typically at application startup or at the top of a test.

```rust
// Build the layer (runs the constructor effect)
let db: Tagged<DatabaseKey> = db_layer.build(my_config).await?;
```

Or more commonly, layers are composed and the whole graph is built at once (covered in §6.4).

## The Key Insight

- **Effects** are programs. They describe *what to do* with dependencies.
- **Layers** are constructors. They describe *how to build* dependencies.

When you call `.provide_layer(some_layer)`, you're saying: "use this Layer's output to satisfy this Effect's `R`." The Layer builds the dependency; the Effect consumes it.

## Lifecycle and Resource Safety

Layers aren't just factories. They can also register cleanup:

```rust
LayerFn::new(|_: &Nil| {
    effect! {
        let pool = ~ connect_pool(url);
        // Register cleanup: close pool on shutdown
        ~ scope.add_finalizer(Finalizer::new(move || {
            pool.close()
        }));
        tagged::<DatabaseKey>(pool)
    }
})
```

The finalizer runs when the layer's scope is dropped — even if the application panics or a fiber is cancelled. This makes Layers the safe way to manage resources with expensive setup and teardown.

Chapter 10 covers scopes and finalizers in detail. For now, know that Layers are where you register resource lifecycles.
