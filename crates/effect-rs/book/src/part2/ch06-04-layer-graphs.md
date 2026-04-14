# Layer Graphs — Automatic Dependency Resolution

For small applications, manually stacking layers in the right order is fine. For larger ones with dozens of services and complex inter-dependencies, it gets tedious and error-prone. `LayerGraph` automates it.

## Declaring a Layer Graph

```rust
use effect::{LayerGraph, LayerNode};

let graph = LayerGraph::new()
    .add(LayerNode::new("config",  config_layer))
    .add(LayerNode::new("db",      db_layer)
             .requires("config"))
    .add(LayerNode::new("cache",   cache_layer)
             .requires("config"))
    .add(LayerNode::new("mailer",  mailer_layer)
             .requires("config"))
    .add(LayerNode::new("service", service_layer)
             .requires("db")
             .requires("cache"))
    .add(LayerNode::new("app",     app_layer)
             .requires("service")
             .requires("mailer"));
```

Each `LayerNode` has a name and declares its dependencies with `.requires()`. The `LayerGraph` figures out the build order automatically.

## Planning and Building

```rust
// Compute the build plan (topological sort)
let plan: LayerPlan = graph.plan()?;

// Build according to the plan (parallelises where possible)
let env = plan.build(()).await?;
```

`LayerPlan` is the computed ordering. It runs independent layers concurrently and sequential layers in order. The graph in the example above would:
1. Build `config` first
2. Build `db`, `cache`, and `mailer` concurrently (all need `config`, none need each other)
3. Build `service` (needs `db` + `cache`)
4. Build `app` (needs `service` + `mailer`)

## Cycle Detection

`graph.plan()` returns an error if there are circular dependencies:

```rust
let bad_graph = LayerGraph::new()
    .add(LayerNode::new("a", layer_a).requires("b"))
    .add(LayerNode::new("b", layer_b).requires("a"));  // circular!

let err = bad_graph.plan();  // Err(LayerGraphError::Cycle { ... })
```

Cycles are detected at plan time, before any work begins. The error message identifies the cycle.

## Conditional Layers

Layers can be added conditionally:

```rust
let mut graph = LayerGraph::new()
    .add(LayerNode::new("config", config_layer));

if cfg!(feature = "metrics") {
    graph = graph.add(LayerNode::new("metrics", metrics_layer)
        .requires("config"));
}
```

Feature flags and environment-based configuration compose naturally with the graph API.

## When to Use LayerGraph vs Stack

| Situation | Prefer |
|-----------|--------|
| < 5 layers, clear order | `.stack()` |
| > 5 layers, complex deps | `LayerGraph` |
| Need cycle detection | `LayerGraph` |
| Conditional/pluggable services | `LayerGraph` |
| Tests with minimal deps | `.stack()` |

`LayerGraph` is overkill for small programs. For anything approaching production scale, the automatic resolution and parallelism are worth it.
