# Pools — Reusing Expensive Resources

Creating a database connection takes time: DNS lookup, TCP handshake, TLS, authentication. Creating one per request is wasteful. A pool maintains a set of connections and lends them out, returning them when done.

effect-rs provides `Pool` and `KeyedPool` as first-class effect constructs.

## Pool: Basic Connection Pool

```rust
use effect::Pool;

// Create a pool of up to 10 connections
let pool: Pool<Connection> = Pool::new(
    || open_connection("postgres://localhost/app"),  // factory
    10,                                               // max size
);
```

The pool lazily creates connections up to the max. Idle connections are kept alive for reuse.

## Using a Pool Connection

```rust
pool.with_resource(|conn: &Connection| {
    effect! {
        let rows = ~ conn.query("SELECT * FROM users");
        rows.into_iter().map(User::from_row).collect::<Vec<_>>()
    }
})
```

`with_resource` acquires a connection from the pool, runs the effect, and returns the connection automatically when done — regardless of success, failure, or cancellation. No `acquire_release` boilerplate; the pool handles it.

## Waiting for Availability

If all connections are in use, `with_resource` waits until one becomes available:

```rust
// Concurrent requests share the pool; each waits its turn
fiber_all(vec![
    pool.with_resource(|c| query_a(c)),
    pool.with_resource(|c| query_b(c)),
    pool.with_resource(|c| query_c(c)),
])
```

The pool queues waiters and notifies them as connections are returned.

## KeyedPool: Multiple Named Pools

For scenarios with multiple distinct pools (e.g., read replica + write primary):

```rust
use effect::KeyedPool;

let pools: KeyedPool<&str, Connection> = KeyedPool::new(
    |key: &&str| open_connection(key),
    5,  // max per key
);

// Get a connection for the write primary
pools.with_resource("write-primary", |conn| { ... })

// Get a connection for the read replica
pools.with_resource("read-replica", |conn| { ... })
```

Each key has its own independently bounded pool.

## Pool as a Service

In practice, pools live in the effect environment as services:

```rust
service_key!(DbPoolKey: Pool<Connection>);

fn query_users() -> Effect<Vec<User>, DbError, impl NeedsDbPool> {
    effect! {
        let pool = ~ DbPoolKey;
        ~ pool.with_resource(|conn| {
            effect! {
                let rows = ~ conn.query("SELECT * FROM users");
                rows.iter().map(User::from_row).collect::<Vec<_>>()
            }
        })
    }
}
```

The pool is provided via a Layer:

```rust
let pool_layer = LayerFn::new(|config: &Tagged<ConfigKey>| {
    effect! {
        let pool = Pool::new(
            || Connection::open(config.value().db_url()),
            config.value().pool_size(),
        );
        tagged::<DbPoolKey>(pool)
    }
});
```

Pool creation, lifecycle, and cleanup are all handled by the Layer. Business code sees only `NeedsDbPool`.
