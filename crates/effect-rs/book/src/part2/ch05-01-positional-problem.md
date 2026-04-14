# The Problem with Positional Types

If you've used tuples as `R` for a while, you've probably already hit the wall. Let's make it explicit.

## The Tuple Explosion

Two dependencies: perfectly readable.

```rust
Effect<A, E, (Database, Logger)>
```

Five dependencies: which is which?

```rust
Effect<A, E, (Pool, Pool, Logger, Config, HttpClient)>
//            ^^^^ two Pools — which is the main DB and which is the cache?
```

Tuples are positional. `(Pool, Pool, ...)` is ambiguous — both fields have the same type. There's no way to distinguish them except by index, and index-based access is error-prone and breaks silently when you reorder the tuple.

## The Fragility Problem

Positional types are fragile under change. Say your function started with:

```rust
fn foo() -> Effect<A, E, (Database, Logger)>
//                        0         1
```

Now a teammate adds `Config` between them:

```rust
fn foo() -> Effect<A, E, (Database, Config, Logger)>
//                        0         1       2
```

Every caller that was providing a tuple `(db, log)` must be updated to `(db, config, log)`. The change in position is invisible to the type system — the compiler won't tell you where the old index references are. It's a silent bomb.

## The Same-Type Collision

The deeper problem: Rust can't distinguish `Pool` for the main database from `Pool` for the cache. They're the same type. Positional tuples just accept both:

```rust
// V1: provide (main_pool, cache_pool)
// V2: accidentally swap them
effect.provide((cache_pool, main_pool))  // compiles, wrong at runtime
```

No compile error. Wrong behaviour. Possibly wrong for months before you notice.

## What We Actually Need

We need a way to give each dependency a *name* — a compile-time identifier that's independent of its type and its position in any list.

What if:
- `Database` meant "the tagged Pool known as DatabaseTag"
- `Cache` meant "the tagged Pool known as CacheTag"

Then you couldn't accidentally swap them — they'd be different types even though both are `Pool` underneath.

That's exactly what Tags provide.
