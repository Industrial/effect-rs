# What Even Is an Effect?

**An Effect is a description of a computation, not the computation itself.**

The rest of the API—`map`, `flat_map`, environment types, runners—is there to work with that description in a type-safe way.

## The Recipe Analogy

Think about a recipe for chocolate cake.

A recipe is not a cake. You can hold a recipe in your hands without any flour appearing. You can read a recipe without preheating an oven. You can photocopy a recipe, modify it (less sugar, more cocoa), combine it with a frosting recipe, and share it with a friend — all without a single cake coming into existence.

The cake only appears when someone *executes* the recipe. Takes out the ingredients, follows the steps, waits for the oven.

An `Effect` is a recipe for a computation.

When you write `succeed(42)`, you're not "succeeding" at anything. You're writing down a recipe that says "when executed, produce the value 42." The 42 doesn't exist yet. No computation has happened. You just have a piece of paper with instructions on it.

```rust
use id_effect::{Effect, succeed};

// This doesn't compute anything — it's a description
let recipe: Effect<i32, String, ()> = succeed(42);

// Still nothing has happened. `recipe` is just a value.
// We can pass it around, store it, inspect its type.
```

The computation only happens when you explicitly run it:

```rust
use id_effect::run_blocking;

// NOW something happens
let result: Result<i32, String> = run_blocking(recipe);
assert_eq!(result, Ok(42));
```

## Building Up Descriptions

Because an Effect is just data — a description — you can transform it without running it.

```rust
let recipe: Effect<i32, String, ()> = succeed(42);

// Transform the description: "when executed, produce 42, then double it"
let doubled: Effect<i32, String, ()> = recipe.map(|x| x * 2);

// Still nothing has happened! `doubled` is just a modified recipe.

// Now run it
let result = run_blocking(doubled);
assert_eq!(result, Ok(84));
```

The `.map()` call didn't execute anything. It took one recipe and produced a new recipe that includes an extra step. Like writing "double the result" at the bottom of your cake recipe — the cake doesn't change until someone bakes it.

## A More Realistic Example

Let's see what this looks like with actual I/O:

```rust
use id_effect::{Effect, effect, run_blocking};

// This function doesn't fetch anything — it returns a DESCRIPTION
// of how to fetch a user
fn fetch_user(id: u64) -> Effect<User, DbError, ()> {
    effect! {
        let conn = ~ connect_to_db();
        let user = ~ query_user(&conn, id);
        Ok(user)
    }
}

// Calling the function doesn't open any connections
let description = fetch_user(42);

// `description` is a value we can hold, pass around, combine with others
// No database has been touched

// Only when we run it does the I/O happen
let user = run_blocking(description)?;
```

That `effect!` block looks imperative — it looks like it's doing things. But it's not. It's building a description of things to do. The `~` operator means "this step depends on the previous step completing" — it's describing sequencing, not executing it.

## The Key Insight: Separation of Concerns

This separation — description vs execution — is how the challenges from the previous section (errors, dependencies, task structure) get a consistent home in the type system.

**Error handling** becomes part of the description itself. When you write:

```rust
let resilient = risky_operation.retry(Schedule::exponential(100.ms(), 3));
```

You're not adding retry logic to running code. You're modifying the *description* to say "when executed, retry up to 3 times with exponential backoff." The retry logic is baked into the recipe.

**Dependencies** become part of the type signature. When you write:

```rust
fn get_user(id: u64) -> Effect<User, DbError, Database>
```

That `Database` in the type says "this recipe requires a Database to execute." The compiler enforces it. You can't run the effect without providing a Database. No runtime surprises.

**Structured concurrency** becomes possible because the runtime knows what each effect intends to do before it does it. Spawning an effect doesn't fire and forget — it creates a handle to a structured task with clear ownership and cancellation semantics.

## What's in an Effect?

An `Effect<A, E, R>` carries three pieces of information in its type:

- `A` — the **Answer**: what you get if it succeeds
- `E` — the **Error**: what you get if it fails
- `R` — the **Requirements**: what environment is needed to run it

We'll explore all three in the next section. For now, just notice that an Effect's type tells you everything about what it does — success, failure, and dependencies — without you having to read the implementation.

```rust
// This type signature tells the whole story:
fn process_payment(
    amount: Money
) -> Effect<Receipt, PaymentError, (PaymentGateway, Logger)>

// - Produces a Receipt on success
// - Can fail with PaymentError
// - Requires a PaymentGateway and Logger to run
```

No need to read the function body to know what resources it needs or what errors it can produce. The type *is* the documentation.

## Style: imperative async vs effect descriptions

Typical `async fn` code is written as a sequence of steps: each `.await` drives the next piece of work. That is clear and idiomatic Rust.

Effect code in this library is often written so that **many domain functions return `Effect<…>`**: a value that describes work and only runs when you pass it to a runner with an environment. The style emphasizes **composition** (map, flat_map, layers, retries) before execution.

Both approaches run on the same `Future` machinery underneath. Use effects where you want environment and error structure in the type, shared policies, and test substitution at the boundary; use plain `async fn` where a small linear function is enough.

Let's look at those three type parameters in detail.
