# acquire_release — The RAII Pattern

`acquire_release` is a convenience wrapper around Scope that pairs acquisition and release into a single value.

## The Pattern

```rust
use effect::acquire_release;

let managed_connection = acquire_release(
    // Acquire: run this to get the resource
    open_connection(),
    // Release: run this when done (always runs)
    |conn| conn.close(),
);
```

`managed_connection` is itself an effect that:
1. When executed, opens the connection
2. Registers `conn.close()` as a finalizer in the current scope
3. Produces the connection for use

Use it with `flat_map` or `effect!`:

```rust
let result = managed_connection.flat_map(|conn| {
    do_work_with_conn(&conn)
});
// conn.close() runs after do_work_with_conn, regardless of outcome
```

Or inline:

```rust
effect! {
    let conn = ~ managed_connection;
    let data = ~ fetch_data(&conn);   // conn closes after this block
    process(data)
}
```

## Why This Is Better Than Manual Scope

`acquire_release` makes the acquisition-release pair inseparable. You can't accidentally call `open_connection()` without also registering its cleanup. The resource and its lifecycle are coupled at the point of creation.

```rust
// With manual scope: easy to forget the finalizer
let conn = ~ open_connection();
// ... (forgot: ~ scope.add_finalizer(...))

// With acquire_release: cleanup is mandatory, automatic
let conn = ~ acquire_release(open_connection(), |c| c.close());
```

## Resource Wrapping Pattern

A common convention is to wrap `acquire_release` in a helper function:

```rust
fn managed_db_connection(url: &str) -> Effect<Connection, DbError, ()> {
    acquire_release(
        Connection::open(url),
        |conn| conn.close(),
    )
}

// Usage
effect! {
    let conn = ~ managed_db_connection(config.db_url());
    ~ run_query(&conn, "SELECT 1")
}
```

The helper function documents that `open_connection()` must always be paired with `close()`. Callers don't think about the lifecycle; it's handled.

## Comparison with Drop

`acquire_release` is not a replacement for `impl Drop` — it's a complement:

- `impl Drop`: synchronous cleanup for types that own simple resources
- `acquire_release`: async cleanup for effects that acquire and release through the effect runtime

Use both appropriately. A `TcpStream` closing its OS file descriptor in `Drop` is fine. Closing a database connection pool that requires async coordination belongs in `acquire_release`.
