# Stream vs Effect — When to Use Each

The choice is simple: how many values does your computation produce?

```
Effect<A, E, R>  → produces exactly one A (or fails)
Stream<A, E, R>  → produces zero or more A values over time (or fails)
```

## Concrete Examples

```rust
// Effect: get one user
fn get_user(id: u64) -> Effect<User, DbError, Db>

// Stream: all users, one at a time
fn all_users() -> Stream<User, DbError, Db>

// Effect: count rows
fn count_orders() -> Effect<u64, DbError, Db>

// Stream: export all orders for a report
fn export_orders() -> Stream<Order, DbError, Db>
```

If you fetch 10 million rows into a `Vec` and return it as an `Effect`, you'll run out of memory. A `Stream` loads and processes them incrementally.

## Stream Transformations

`Stream` has the same transformation API as `Effect`:

```rust
all_users()
    .filter(|u| u.is_active())
    .map(|u| UserSummary::from(u))
    .take(100)
```

`.map`, `.filter`, `.flat_map`, `.take`, `.drop`, `.zip` — all work on streams. None of them load the whole stream into memory; they process elements as they arrive.

## Collecting a Stream into an Effect

When you do need all the results:

```rust
let users: Effect<Vec<User>, DbError, Db> = all_users().collect();
```

`.collect()` consumes the stream and accumulates into a `Vec`. Use only when the full result fits in memory.

For large results, prefer a fold or a sink:

```rust
let count: Effect<usize, DbError, Db> = all_users().fold(0, |acc, _| acc + 1);
```

## Converting Effect to Stream

Wrap an `Effect` in a single-element stream when you need to compose with streaming operators:

```rust
use effect::Stream;

let single_user_stream: Stream<User, DbError, Db> = Stream::from_effect(get_user(1));

// Now compose with other streams
let combined = single_user_stream.chain(all_users());
```

## The Rule

- Need one result: `Effect`
- Need to process many results without loading all at once: `Stream`
- Need to compose multiple streams: `Stream` with `chain`, `zip`, `merge`
- Need all results in memory: `Stream` + `.collect()` (with appropriate size caution)
