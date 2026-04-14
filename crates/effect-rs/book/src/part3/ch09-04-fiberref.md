# FiberRef — Fiber-Local State

`FiberRef` is the effect equivalent of thread-local storage. It holds a value that's scoped to the current fiber — each fiber has its own independent copy.

## Defining a FiberRef

```rust
use effect::FiberRef;

// A fiber-local trace ID, defaulting to "none"
static TRACE_ID: FiberRef<String> = FiberRef::new(|| "none".to_string());
```

`FiberRef::new` takes a factory closure that produces the initial value for each fiber. The static variable is the *key*; each fiber has its own value.

## Reading and Writing

```rust
effect! {
    // Set the trace ID for this fiber
    ~ TRACE_ID.set("req-abc-123".to_string());

    // Read it anywhere in this fiber's call stack
    let id = ~ TRACE_ID.get();
    ~ log(&format!("[{id}] processing request"));

    ~ process_request();
    Ok(())
}
```

`set` and `get` are both effects (they need the fiber context). Inside `effect!`, use `~` to bind them.

## FiberRef Doesn't Cross Fiber Boundaries

When you `fork` a new fiber, it starts with its own copy of the FiberRef value (the factory closure runs again):

```rust
effect! {
    ~ TRACE_ID.set("parent-123".to_string());
    
    let child = effect! {
        let id = ~ TRACE_ID.get();
        // id is "none" — the fork starts fresh
        println!("child trace id: {id}");
    }.fork();
    
    ~ child.join();
    Ok(())
}
```

If you want the child to inherit the parent's value, pass it explicitly or use `FiberRef::inherit`:

```rust
let child_with_inherited = effect! {
    let id = ~ TRACE_ID.get();
    TRACE_ID.locally(id, child_effect())  // child sees parent's value
};
```

`locally(value, effect)` runs the effect with a temporarily overridden FiberRef value, then restores the previous value when done.

## Common Use Cases

| Use Case | Pattern |
|----------|---------|
| Request tracing / correlation IDs | `static TRACE_ID: FiberRef<String>` |
| Per-request user context | `static CURRENT_USER: FiberRef<Option<UserId>>` |
| Metrics labels | `static OPERATION: FiberRef<&'static str>` |
| Debug context | `static CALL_PATH: FiberRef<Vec<String>>` |

`FiberRef` makes it easy to carry contextual information through deep call stacks without threading extra parameters everywhere — the fiber equivalent of request-scoped context in traditional web frameworks.
