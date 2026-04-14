# ServiceEnv and service_env — The Glue

`service_key!` handles the tag definition. `ServiceEnv` and `service_env` provide the glue for accessing a service from the environment inside an effect.

## service_env: Access a Service

```rust
use effect::{service_env, ServiceEnv};

// Access a service and use it
fn get_user(id: u64) -> Effect<User, DbError, ServiceEnv<UserRepositoryTag>> {
    effect! {
        let repo = ~ service_env::<UserRepositoryTag>();
        ~ repo.get_user(id)
    }
}
```

`service_env::<K>()` returns an effect that, when run, extracts the `Arc<dyn Trait>` identified by `K` from the environment.

`ServiceEnv<K>` is a type alias for the `R` required by `service_env` — effectively `Context<Cons<Tagged<K>, Nil>>` but more readable.

## The ~ Tag Shorthand

Inside `effect!`, you can use the tag directly with `~`:

```rust
fn get_user(id: u64) -> Effect<User, DbError, impl NeedsUserRepository> {
    effect! {
        let repo: Arc<dyn UserRepository> = ~ UserRepositoryTag;
        ~ repo.get_user(id)
    }
}
```

`~ UserRepositoryTag` is syntactic sugar for `~ service_env::<UserRepositoryTag>()`. Both work; the shorthand is more concise.

## Multiple Services in One Effect

When a function needs several services:

```rust
fn notify_user(user_id: u64, message: &str)
-> Effect<(), AppError, impl NeedsUserRepository + NeedsEmailService>
{
    effect! {
        let repo  = ~ UserRepositoryTag;
        let email = ~ EmailServiceTag;
        
        let user = ~ repo.get_user(user_id).map_error(AppError::Db);
        ~ email.send(&user.email, message).map_error(AppError::Email);
        ()
    }
}
```

The `impl NeedsX + NeedsY` syntax is the idiomatic way to express multiple requirements. The compiler verifies that the provided environment satisfies both traits.

## ServiceEnv vs Raw Context

`ServiceEnv<K>` is a convenience type. Under the hood it's just a `Context` with one element. The difference is ergonomic:

```rust
// With ServiceEnv
fn f() -> Effect<User, E, ServiceEnv<UserRepositoryTag>> { ... }

// With raw Context (equivalent, more verbose)
fn f() -> Effect<User, E, Context<Cons<Tagged<UserRepositoryTag>, Nil>>> { ... }
```

Use `ServiceEnv` for single-service effects. Use `impl NeedsX` for effects in library code where the concrete context type shouldn't leak into the signature. The Layers machinery accepts both at runtime.
