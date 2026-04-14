# Chaining Effects — flat_map and the Bind

`map` handles the case where your transformation is a pure function: `A → B`. But often the next step is itself an effect. You don't want `Effect<Effect<B, E, R>, E, R>` — you want `Effect<B, E, R>`. That's `flat_map`.

## The Problem with map for Effects

Say you want to fetch a user and then fetch their posts:

```rust
fn get_user(id: u64) -> Effect<User, DbError, Database> { ... }
fn get_posts(user_id: u64) -> Effect<Vec<Post>, DbError, Database> { ... }
```

If you try to use `map`:

```rust
// This gives Effect<Effect<Vec<Post>, DbError, Database>, DbError, Database>
// — a nested effect, not what we want
let wrong = get_user(1).map(|user| get_posts(user.id));
```

`map`'s function must return a plain value. If it returns an `Effect`, you get nesting.

## flat_map: Chain Without Nesting

`flat_map` (also known as `and_then` on effects) takes a function `A → Effect<B, E, R>` and "flattens" the result:

```rust
let combined: Effect<Vec<Post>, DbError, Database> =
    get_user(1).flat_map(|user| get_posts(user.id));
```

Now you have one flat effect that, when run, first fetches the user, then uses the result to fetch posts. The nesting is gone.

## Chaining Multiple Steps

`flat_map` chains read left-to-right, but deep chains get noisy:

```rust
// Gets unwieldy quickly
let program = get_user(1)
    .flat_map(|user| get_posts(user.id)
        .flat_map(|posts| render_page(user, posts)));
```

This is where the `effect!` macro comes in.

## The effect! Macro as Syntactic Sugar

The `effect!` macro turns `flat_map` chains into readable sequential code using the `~` operator:

```rust
use effect::effect;

let program: Effect<Page, AppError, Database> = effect! {
    let user  = ~ get_user(1).map_error(AppError::Db);
    let posts = ~ get_posts(user.id).map_error(AppError::Db);
    let page  = render_page(user, posts);
    page
};
```

The `~` operator is the bind: "run this effect and give me its success value." Each `~ expr` desugars to a `flat_map`. The whole block is one effect.

Note that `render_page` (a pure function with no `~`) is just a normal Rust expression — it runs inside the macro body during execution.

## Error Short-Circuiting

Like `?` in `Result`, if any `~` step fails, the whole `effect!` exits early with that error:

```rust
let program: Effect<Page, AppError, Database> = effect! {
    let user = ~ get_user(999).map_error(AppError::Db);
    // If get_user fails, execution stops here.
    // The rest never runs.
    let posts = ~ get_posts(user.id).map_error(AppError::Db);
    render_page(user, posts)
};
```

This is sequential, not parallel. Each step waits for the previous.

## map vs flat_map — When to Use Each

| Situation | Use |
|-----------|-----|
| Transformation returns a plain value | `.map(f)` |
| Transformation returns an Effect | `.flat_map(f)` or `effect! { ~ ... }` |
| More than one sequential step | `effect! { ~ ... }` macro |

A rule of thumb: if you find yourself writing `effect.map(|v| another_effect(v))` and noticing the nested type, switch to `flat_map` or the macro.

## The Full Picture

```rust
// All equivalent:

// 1. Explicit flat_map
get_user(1)
    .flat_map(|user| get_posts(user.id))

// 2. Using effect! with ~
effect! {
    let user = ~ get_user(1);
    ~ get_posts(user.id)
}

// 3. Short form for single bind
effect! { ~ get_user(1).flat_map(|u| get_posts(u.id)) }
```

The `effect!` macro is the idiomatic choice for anything more than one step. Chapter 3 covers it in full detail.
