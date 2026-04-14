# Exit — Terminal Outcomes

Every effect execution ends with an `Exit`. It's the final word on what happened.

## The Exit Type

```rust
use effect::Exit;

enum Exit<E, A> {
    Success(A),         // Effect completed, produced A
    Failure(Cause<E>),  // Effect failed with Cause<E>
}
```

`Exit` combines the success type and the full failure taxonomy. It's what you get when you use `run_to_exit` instead of `run_blocking`:

```rust
use effect::run_to_exit;

// run_blocking returns Result<A, E> — loses Cause::Die and Cause::Interrupt info
let user: Result<User, DbError> = run_blocking(get_user(1))?;

// run_to_exit returns Exit<E, A> — full picture
let exit: Exit<DbError, User> = run_to_exit(get_user(1));

match exit {
    Exit::Success(user)                    => println!("Got user: {}", user.name),
    Exit::Failure(Cause::Fail(DbError::NotFound)) => println!("User not found"),
    Exit::Failure(Cause::Die(panic_val))   => eprintln!("Defect: {:?}", panic_val),
    Exit::Failure(Cause::Interrupt)        => println!("Cancelled"),
}
```

## Converting Exit to Result

Most application code wants `Result`. The conversion is straightforward:

```rust
let result: Result<User, AppError> = exit.into_result(|cause| match cause {
    Cause::Fail(e) => AppError::Expected(e),
    Cause::Die(_)  => AppError::Defect,
    Cause::Interrupt => AppError::Cancelled,
});
```

Or use the convenience method that maps `Cause::Fail(e)` → `Err(e)` and treats other causes as panics:

```rust
let result: Result<User, DbError> = exit.into_result_or_panic();
```

## Exit in Fiber Joins

When you join a fiber (Chapter 9), you get an `Exit` back:

```rust
let fiber = my_effect.fork();
let exit: Exit<E, A> = fiber.join().await;
```

This lets you inspect whether the fiber succeeded, failed with a typed error, panicked, or was cancelled — and respond appropriately in the parent fiber.

## Practical Rule

Use `run_blocking` (which returns `Result<A, E>`) for 90% of cases. Use `run_to_exit` when you need to distinguish panics from typed failures — typically at top-level handlers, supervisors, or when integrating with external error reporting.
