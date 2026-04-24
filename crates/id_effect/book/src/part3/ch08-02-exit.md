# Exit — Terminal Outcomes

Every effect execution ends with an `Exit`. It's the final word on what happened.

## The Exit Type

```rust
use id_effect::Exit;

enum Exit<A, E> {
    Success(A),         // Effect completed, produced A
    Failure(Cause<E>),  // Effect failed with Cause<E>
}
```

`Exit` combines the success type and the full failure taxonomy.

- **`run_blocking`** returns **`Result<A, E>`** — you only see typed **`E`** failures; defects and interrupts are not represented as values there.
- [`run_test`](../part4/ch15-01-run-test.md) returns **`Exit<A, E>`** and is the right harness for **unit tests** that must assert on defects, interrupts, and typed failures.

For **CLI / process exit codes**, map an `Exit` with **`id_effect_cli::exit_code_for_exit`** (see [CLI with clap](./ch16-00-cli-with-clap.md)).

## Converting Exit to Result

Most application code wants `Result`. The conversion is straightforward:

```rust
let result: Result<User, AppError> = exit.into_result(|cause| match cause {
    Cause::Fail(e) => AppError::Expected(e),
    Cause::Die(_)  => AppError::Defect,
    Cause::Interrupt(_) => AppError::Cancelled,
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
let exit: Exit<A, E> = fiber.join().await;
```

This lets you inspect whether the fiber succeeded, failed with a typed error, panicked, or was cancelled — and respond appropriately in the parent fiber.

## Practical Rule

Use **`run_blocking`** (which returns **`Result<A, E>`**) for most application logic. Use **`run_test`** when you need the full **`Exit`** taxonomy in tests. At the **process edge** (binaries), combine **`run_main`** / **`exit_code_for_exit`** from **`id_effect_cli`** with the table in the [CLI exit codes](./ch16-01-cli-exit-codes.md) chapter.
