# Providing Dependencies — The provide Method

An effect with a non-`()` `R` can't be run directly. To run it, you must satisfy its requirements. The primary tool is `.provide()`.

## Basic provide

```rust
fn get_user(id: u64) -> Effect<User, DbError, Database> { ... }

let effect: Effect<User, DbError, Database> = get_user(42);

// Satisfy the Database requirement
let ready: Effect<User, DbError, ()> = effect.provide(my_database);

// R is now () — we can run it
let user = run_blocking(ready)?;
```

`.provide(value)` takes a value of whatever type `R` needs and returns a new effect where that requirement is satisfied. When `R` becomes `()`, the effect is runnable.

## Providing Multiple Dependencies

If `R = (Database, Logger)`, call `.provide()` twice:

```rust
fn logged_get_user(id: u64) -> Effect<User, AppError, (Database, Logger)> { ... }

let user = run_blocking(
    logged_get_user(42)
        .provide(my_database)
        .provide(my_logger)
)?;
```

Order doesn't matter — each `.provide()` removes one requirement from the tuple. After both, `R = ()`.

## Partial Providing with provide_some

Sometimes you want to satisfy *some* requirements now and the rest later:

```rust
// Provides Database, still needs Logger
let partial: Effect<User, AppError, Logger> =
    logged_get_user(42).provide_some(my_database);

// Later, provide the Logger
let ready: Effect<User, AppError, ()> =
    partial.provide(my_logger);
```

`provide_some` is useful when you're building up an effect in layers — each layer provides what it knows about.

## Providing Layers (Preview)

For most real applications, you don't call `.provide()` with raw values. Instead, you use *Layers* — recipes that know how to construct dependencies from other dependencies:

```rust
// The idiomatic way in real apps
let app_effect = my_business_logic()
    .provide_layer(app_layer);
```

Layers are covered in Chapter 6. For now, think of `.provide(value)` as the low-level primitive and Layers as the high-level pattern built on top.

## Where to Call provide

The rule is simple: **provide at the program edge, not inside library functions.**

Library functions should stay generic over `R`:

```rust
// BAD — library function reaches in and provides its own deps
pub fn process_order(order: Order) -> Effect<Receipt, AppError, ()> {
    let db = Database::connect("hardcoded-url");  // where did this come from?
    inner_process(order).provide(db)
}

// GOOD — library returns requirements in R, caller provides
pub fn process_order(order: Order) -> Effect<Receipt, AppError, Database> {
    inner_process(order)
}
```

The caller — `main`, a test, or a higher-level orchestrator — knows what database to use. The library function should not.

## Summary

```rust
// Satisfy one requirement
effect.provide(value)

// Satisfy one of several requirements
effect.provide_some(value)

// Satisfy with a Layer (see Ch6)
effect.provide_layer(layer)
```

All three return a new effect with a smaller (or empty) `R`. None of them execute anything — `.provide()` is still lazy.
