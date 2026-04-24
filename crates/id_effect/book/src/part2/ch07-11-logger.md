# Logging (`id_effect_logger`)

Workspace crate **`id_effect_logger`** provides an injectable **`EffectLogger`** service: log lines are **effect steps** that read **`EffectLogKey`** from **`R`**, so formatting and sinks stay composable and testable.

## Usage shape

- **`~EffectLogger`** inside **`effect!`** (or explicit `Get`) to obtain the handle, then **`info`**, **`warn`**, … on static or dynamic messages (`impl Into<Cow<'static, str>>`).
- **`layer_effect_logger`** (and related constructors) build the **`Service<EffectLogKey, EffectLogger>`** cell you **`Cons`** into **`Context`**.

## Backends

The crate ships pipeline pieces such as **structured JSON**, **tracing** integration, and **composite** backends—see `cargo doc -p id_effect_logger` for **`LogBackend`**, **`JsonLogBackend`**, **`TracingLogBackend`**, **`CompositeLogBackend`**.

## Relation to the rest of Part II

Logging is just another **tagged service**: same mental model as [Tags and Context](./ch05-00-tags-context.md) and [Providing Services via Layers](./ch07-03-providing-services.md). Swap backends in tests instead of silencing `println!`.

## Further reading

- `cargo doc --open -p id_effect_logger`
