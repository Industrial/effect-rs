# Context and HLists — The Heterogeneous Stack

`Context` is the concrete data structure that `R` resolves to at runtime. It's a *heterogeneous list* (HList) — a stack where each element has a different type, and the compiler tracks all of them.

## The Structure: Cons / Nil

`Context` is built from two constructors:

```rust
use effect::{Context, Cons, Nil, Tagged};

// An empty context
type Empty = Nil;

// A context with one item
type WithDb = Cons<Tagged<DatabaseKey>, Nil>;

// A context with two items
type WithDbAndLogger = Cons<Tagged<DatabaseKey>, Cons<Tagged<LoggerKey>, Nil>>;
```

`Cons<Head, Tail>` prepends one item to a list. `Nil` is the empty list. It's the same idea as linked-list types in functional languages, but expressed as Rust type parameters.

## Building Context Values

Manually building `Cons` chains is verbose. The `ctx!` macro handles it:

```rust
use effect::ctx;

let env: Context<Cons<Tagged<DatabaseKey>, Cons<Tagged<LoggerKey>, Nil>>> = ctx!(
    tagged::<DatabaseKey>(my_pool),
    tagged::<LoggerKey>(my_logger),
);
```

Or use `prepend_cell` manually if you need to add to an existing context:

```rust
use effect::prepend_cell;

let base = ctx!(tagged::<LoggerKey>(my_logger));
let full = prepend_cell(tagged::<DatabaseKey>(my_pool), base);
```

Both produce the same type. `ctx!` is preferred for clarity.

## Why HLists and Not HashMap?

A `HashMap<TypeId, Box<dyn Any>>` would also store heterogeneous values. But it trades type safety for flexibility — lookups return `Box<dyn Any>`, and you have to downcast.

`Context` gives:
- **Compile-time lookup**: if you ask for `DatabaseKey` and it's not in the context, you get a compile error
- **Zero-cost access**: no hashing, no downcast, no `Option` unwrapping
- **Type preservation**: `Get<DatabaseKey>` returns `&Pool`, not `&dyn Any`

The cost is that the type of a `Context` encodes all its elements in the type parameter — which is why you see signatures like `Cons<Tagged<A>, Cons<Tagged<B>, Nil>>`. It's verbose, but it's verifiable at compile time.

## Order Doesn't Matter for Access

Unlike tuples, adding an element to a `Context` doesn't break existing lookups. `Get<DatabaseKey>` finds the `Tagged<DatabaseKey>` wherever it is in the list:

```rust
// These two contexts both support Get<DatabaseKey>
type C1 = Cons<Tagged<DatabaseKey>, Cons<Tagged<LoggerKey>, Nil>>;
type C2 = Cons<Tagged<LoggerKey>, Cons<Tagged<DatabaseKey>, Nil>>;

// Both work — order doesn't matter for tag-based access
fn use_db<R: NeedsDatabase>(r: &R) { ... }
```

This is what makes `R` stable under refactoring: adding a new service to the context doesn't change how existing services are accessed.

## R in Practice

In real application code, you rarely construct `Context` directly. Layers (Chapter 6) build it for you. Services (Chapter 7) access it through `NeedsX` bounds. You interact with `Context` directly mostly in:

- Manual test environments (constructing a test `Context` with mock services)
- Integration points where you're bridging an existing application to effect-rs
- Internal library utilities that manipulate context directly

For everything else, the layer and service machinery handles construction automatically.
