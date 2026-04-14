# R Revisited — More Than Just a Type Parameter

You've seen `R` in function signatures:

```rust
fn get_user(id: u64) -> Effect<User, DbError, Database>
```

It looks like "this needs a `Database`." But what does that mean precisely?

## R as a Contract

`R` is a *promise to the compiler*. When you write:

```rust
fn get_user(id: u64) -> Effect<User, DbError, Database> { ... }
```

You are declaring: "To run this effect, you must supply a `Database`." The compiler holds you to that promise. You cannot run `get_user(1)` without providing a `Database` — it won't compile.

```rust
let effect = get_user(1);

// This doesn't compile — Database not provided
// run_blocking(effect);

// This compiles — Database provided via .provide()
let effect_with_db = effect.provide(my_database);
run_blocking(effect_with_db);
```

The contract is not a comment. It's a type-system guarantee.

## R Flows Through Composition

When you combine effects with `effect!`, their `R` requirements merge:

```rust
fn get_user(id: u64) -> Effect<User, DbError, Database> { ... }
fn get_posts(user_id: u64) -> Effect<Vec<Post>, DbError, Database> { ... }

// Combined: R is still just Database (both needed the same thing)
fn get_user_with_posts(id: u64) -> Effect<(User, Vec<Post>), DbError, Database> {
    effect! {
        let user  = ~ get_user(id);
        let posts = ~ get_posts(user.id);
        Ok((user, posts))
    }
}
```

When both effects need the same type, the composed effect needs it once.

When they need *different* things:

```rust
fn log(msg: &str) -> Effect<(), LogError, Logger> { ... }
fn get_user(id: u64) -> Effect<User, DbError, Database> { ... }

// Combined: R is (Database, Logger) — needs BOTH
fn get_user_logged(id: u64) -> Effect<User, AppError, (Database, Logger)> {
    effect! {
        ~ log(&format!("Fetching user {id}")).map_error(AppError::Log);
        let user = ~ get_user(id).map_error(AppError::Db);
        Ok(user)
    }
}
```

The compiler infers that `get_user_logged` needs both. You don't have to declare this manually — composing effects automatically tracks their dependencies.

## Multiple Requirements

As functions grow, they naturally accumulate requirements:

```rust
fn process_order(order: Order) -> Effect<Receipt, AppError, (Database, PaymentGateway, EmailService, Logger)> {
    effect! {
        ~ log("Processing order").map_error(AppError::Log);
        let user    = ~ get_user(order.user_id).map_error(AppError::Db);
        let payment = ~ charge(order.total).map_error(AppError::Payment);
        ~ send_confirmation(&user.email).map_error(AppError::Email);
        Ok(Receipt::new(payment))
    }
}
```

Just from the type signature you know this function touches four subsystems. No need to read the implementation. No "I wonder if this calls the emailer" — the type tells you.

## Why R Instead of Parameters?

Traditional Rust would thread dependencies as function parameters:

```rust
fn process_order(order: Order, db: &Database, pay: &PaymentGateway, email: &EmailService, log: &Logger) -> Result<Receipt, AppError> { ... }
```

That works, but it forces every layer of your call stack to accept and forward dependencies it may not directly use. The `R` parameter encodes the same information in the *type of the return value* rather than in the argument list — and the compiler tracks it automatically as you compose effects.

The practical difference becomes clear in large codebases: adding a new dependency to a deep function no longer requires propagating new parameters through every caller up to `main`. The composition chain carries it automatically.

## Foreshadowing

You may be wondering: if `R` is a type like `(Database, Logger)`, how does the runtime know which field is `Database` and which is `Logger`? And what happens if you have two databases?

Tuples are positional. Position-based access breaks down as soon as you add a second item of the same type, or reorder a tuple. The solution is `Tags` — compile-time names for values in the environment. That's Chapter 5. For now, the intuition of "R = the set of things needed" is sufficient.
