# The ~ Operator Explained

The `~` (tilde) is the bind operator inside `effect!`. It means: "execute this effect and give me its success value; if it fails, propagate the failure and stop."

## Basic Usage

```rust
effect! {
    let user = ~ fetch_user(42);   // bind the result to `user`
    user.name
}
```

`~ fetch_user(42)` desugars to a `flat_map`. The rest of the block becomes the body of the closure.

## Discarding Results

When you don't need the value, use `~` without a binding:

```rust
effect! {
    ~ log_event("processing started");   // run for side effect, discard result
    let result = ~ do_work();
    ~ log_event("processing done");
    result
}
```

Both `~ log_event(...)` expressions run for their effects and the `()` return is discarded.

## Method Calls on Effects

`~` works on any expression that evaluates to an `Effect`. That includes method chains:

```rust
effect! {
    let user = ~ fetch_user(id).map_error(AppError::Database);
    let posts = ~ fetch_posts(user.id)
        .map_error(AppError::Database)
        .retry(Schedule::exponential(100.ms()).take(3));
    (user, posts)
}
```

The `~` applies to the entire expression, including any `.map_error()`, `.retry()`, etc. that follow.

## ~ in Conditionals and Loops

You can use `~` inside `if` expressions and loops:

```rust
effect! {
    let value = if condition {
        ~ compute_a()
    } else {
        ~ compute_b()
    };
    process(value)
}
```

Both branches are effects; the macro handles either path.

```rust
effect! {
    for id in user_ids {
        ~ process_user(id);  // sequential: one at a time
    }
    "done"
}
```

Note: this is *sequential* iteration. For concurrent processing, use `fiber_all` (Chapter 9).

## What ~ Cannot Do

`~` only works *inside* an `effect!` block. Calling it outside is a compile error:

```rust
// Does not compile — ~ is not valid here
let x = ~ fetch_user(42);

// Must be inside effect!
let x = effect! { ~ fetch_user(42) };
```

Also, `~` cannot bind across an async closure boundary. If you're calling `from_async`, the body of the async block is separate:

```rust
effect! {
    let result = ~ from_async(|_r| async move {
        // Inside here, you're in regular Rust async — no ~
        let data = some_future().await?;
        Ok(data)
    });
    result
}
```

Use `~` outside the `async move` block; use `.await` inside it.

## The Old Postfix Syntax (Deprecated)

Early versions of effect-rs used a postfix tilde: `expr ~`. This is no longer valid. Always use the prefix form:

```rust
// OLD — do not use
step_a() ~;

// GOOD
~ step_a();
let x = ~ step_b();
```

If you see postfix tilde in older code, update it to the prefix form.
