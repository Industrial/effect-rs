# Tags — Branding Values with Identity

A `Tag` is a zero-sized type that acts as a compile-time name for a value. It associates an identifier with a value type, so two values of the same underlying type can be distinguished by their tag.

## What Is a Tag?

```rust
use effect::{Tag, Tagged, tagged};

// A Tag is a zero-sized type with an associated Value type
struct DatabaseTag;
impl Tag for DatabaseTag {
    type Value = Pool;
}

struct CacheTag;
impl Tag for CacheTag {
    type Value = Pool;  // Same underlying type, different identity
}
```

`Tagged<DatabaseTag>` is "a `Pool` identified as the database." `Tagged<CacheTag>` is "a `Pool` identified as the cache." They're different types even though both wrap `Pool`.

## Creating Tagged Values

```rust
let db_pool: Pool = connect_database();
let cache_pool: Pool = connect_cache();

// Wrap with identity
let db:    Tagged<DatabaseTag> = tagged(db_pool);
let cache: Tagged<CacheTag>    = tagged(cache_pool);
```

`tagged(value)` is a simple wrapper constructor. It moves the value inside a `Tagged<T>` newtype.

To get the value back:

```rust
let pool: &Pool = db.value();
let pool_owned: Pool = db.into_value();
```

## Why Tags Make the Compiler Your Friend

Now the swap problem from the previous section becomes a compile error:

```rust
// These are DIFFERENT types
fn needs_database<R: NeedsDatabase>() -> Effect<A, E, R> { ... }
fn needs_cache<R: NeedsCache>() -> Effect<A, E, R> { ... }

// Providing the wrong one fails to compile
effect.provide(tagged::<CacheTag>(pool))
// ERROR: expected Tagged<DatabaseTag>, got Tagged<CacheTag>
```

The compiler distinguishes them. You can't accidentally swap the database and cache connections.

## The service_key! Macro

In practice, you don't implement `Tag` by hand. The `service_key!` macro generates the boilerplate:

```rust
use effect::service_key;

service_key!(DatabaseKey: Pool);
service_key!(CacheKey: Pool);
service_key!(LoggerKey: Logger);
```

Each call creates a tag type with the right `Tag` implementation. Use these as your service keys.

## NeedsX Supertraits

When you write a function that needs a `DatabaseKey` service, you want the bound expressed cleanly. The `NeedsX` supertrait pattern does this:

```rust
// Low-level (verbose)
pub fn get_user<R>(id: u64) -> Effect<User, DbError, R>
where
    R: Get<DatabaseKey, Target = Pool>
{ ... }

// High-level (idiomatic) — define NeedsDatabase once
pub trait NeedsDatabase: Get<DatabaseKey, Target = Pool> {}
impl<R: Get<DatabaseKey, Target = Pool>> NeedsDatabase for R {}

// Now use it
pub fn get_user<R: NeedsDatabase>(id: u64) -> Effect<User, DbError, R> { ... }
```

The `NeedsX` trait is just a named alias for the `Get<Key>` bound. It makes function signatures readable and allows you to change the key implementation without updating every call site.

## Summary

| Concept | Purpose |
|---------|---------|
| `Tag` | Zero-sized type acting as a compile-time name |
| `Tagged<T>` | A value wrapped with a tag identity |
| `tagged(v)` | Wrap a value with its tag |
| `service_key!(K: V)` | Macro to generate a tag type |
| `NeedsX` | Supertrait alias for `Get<XKey>` bounds |

Tags eliminate the position problem. The next section shows how they're assembled into `Context` — the heterogeneous list that forms the `R` of a running effect.
