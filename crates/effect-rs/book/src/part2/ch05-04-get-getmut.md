# Get and GetMut — Extracting from Context

Once you have a `Context`, you need to extract values from it. The `Get` and `GetMut` traits define the interface for type-safe lookup by tag.

## Get: Read-Only Access

```rust
use effect::Get;

fn use_database<R>(env: &R) -> &Pool
where
    R: Get<DatabaseKey>,
{
    env.get::<DatabaseKey>()
}
```

`Get<K>` is the trait bound that says "this environment contains a value tagged with `K`." The `get::<K>()` method returns a reference to that value.

The compiler finds the right element in the `Cons` chain automatically. Position doesn't matter — it searches by tag identity.

## GetMut: Mutable Access

```rust
use effect::GetMut;

fn increment_counter<R>(env: &mut R)
where
    R: GetMut<CounterKey>,
{
    let counter: &mut Counter = env.get_mut::<CounterKey>();
    counter.increment();
}
```

`GetMut` is the mutable variant. It's less commonly needed in effect code (effects generally avoid shared mutable state in favour of `TRef` or services), but it's there for integration scenarios.

## The ~ Operator Uses Get Internally

Inside an `effect!` block, the `~` operator is what calls `get::<K>()`:

```rust
effect! {
    let db = ~ DatabaseKey;  // equivalent to env.get::<DatabaseKey>()
    let user = ~ db.fetch_user(id);
    user
}
```

The `~ ServiceKey` form binds the service to a local name. This is the primary way you access services in effect code — you rarely call `get()` directly.

## NeedsX Supertraits (Recap)

Rather than writing `Get<DatabaseKey>` in every function bound, define a `NeedsDatabase` supertrait:

```rust
pub trait NeedsDatabase: Get<DatabaseKey> {}
impl<R: Get<DatabaseKey>> NeedsDatabase for R {}
```

Then use it:

```rust
fn get_user<R: NeedsDatabase>(id: u64) -> Effect<User, DbError, R> { ... }
fn get_posts<R: NeedsDatabase>(uid: u64) -> Effect<Vec<Post>, DbError, R> { ... }

// Composed: still just NeedsDatabase (same requirement)
fn get_user_with_posts<R: NeedsDatabase>(id: u64) -> Effect<(User, Vec<Post>), DbError, R> { ... }
```

The `NeedsX` pattern keeps signatures readable. Define one per service in your application.

## Compile-Time Guarantees

The key property: if you write `Get<DatabaseKey>` in a bound, and the caller tries to run the effect without providing `DatabaseKey`, you get a **compile error**, not a runtime panic.

```rust
// Missing DatabaseKey in the context
let bad_env = ctx!(tagged::<LoggerKey>(my_logger));

// This won't compile — bad_env doesn't satisfy NeedsDatabase
run_blocking(get_user(42).provide(bad_env));
// ERROR: the trait bound `Context<Cons<Tagged<LoggerKey>, Nil>>: NeedsDatabase` is not satisfied
```

The error message tells you exactly what's missing. No runtime "service not found" exceptions. No defensive `unwrap`s in service lookup code.

This is the payoff of the whole Tags/Context system: an application that compiles is an application where every service dependency is satisfied.
