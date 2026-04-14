# Scopes and Finalizers — Guaranteed Cleanup

A `Scope` is a region of execution with a finalizer registry. Any cleanup effects registered in the scope run when the scope exits — regardless of how it exits.

## Creating a Scope

```rust
use effect::{Scope, Finalizer, scoped};

let result = scoped(|scope| {
    effect! {
        let conn = ~ open_connection();

        // Register cleanup — runs when scope exits
        ~ scope.add_finalizer(Finalizer::new(move || {
            conn.close()
        }));

        // Do work
        let data = ~ fetch_data(&conn);
        process(data)
    }
});
```

`scoped` creates a scope, runs the inner effect, and then — in all cases — runs the registered finalizers in reverse order.

## Finalizers Always Run

```rust
let result = scoped(|scope| {
    effect! {
        let conn = ~ open_connection();
        ~ scope.add_finalizer(Finalizer::new(move || conn.close()));

        ~ risky_operation();  // may panic or fail
        "done"
    }
});
// Whether risky_operation succeeds, fails, or panics:
// conn.close() ALWAYS runs before result is returned
```

This is the guarantee that RAII can't provide in async: the finalizer is an async effect that runs in the right context, at the right time, always.

## Multiple Finalizers

Finalizers run in reverse registration order (last-in, first-out — like RAII destructors):

```rust
scoped(|scope| {
    effect! {
        let conn   = ~ open_connection();
        let txn    = ~ begin_transaction(&conn);
        let cursor = ~ open_cursor(&txn);

        ~ scope.add_finalizer(Finalizer::new(move || close_cursor(cursor)));   // runs 3rd
        ~ scope.add_finalizer(Finalizer::new(move || rollback_or_commit(txn))); // runs 2nd... wait
        ~ scope.add_finalizer(Finalizer::new(move || close_connection(conn)));  // wait — read below
    }
})
```

Actually, the first registered finalizer runs *last*. Register cleanup in the order you want it to run, reversed: register connection first (so it closes last), cursor last (so it closes first).

## Scope Inheritance

Scopes nest. A child scope's finalizers run before the parent's:

```rust
scoped(|outer| {
    scoped(|inner| {
        effect! {
            ~ inner.add_finalizer(Finalizer::new(|| cleanup_inner()));
            ~ outer.add_finalizer(Finalizer::new(|| cleanup_outer()));
            work()
        }
    })
})
// Execution order: cleanup_inner(), then cleanup_outer()
```

Layers use scopes internally — every resource a Layer builds can register its own finalizer, and the whole graph tears down cleanly when the application shuts down.
