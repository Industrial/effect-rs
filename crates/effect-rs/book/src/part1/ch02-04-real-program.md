# Your First Real Program

Let's build something complete: a small program that loads configuration, connects to a database, queries a user, and formats a greeting. It's simple enough to fit on one page, but real enough to demonstrate the full effect workflow.

## The Domain

```rust
#[derive(Debug)]
struct Config {
    db_url: String,
    app_name: String,
}

#[derive(Debug)]
struct User {
    id: u64,
    name: String,
    email: String,
}

#[derive(Debug)]
enum AppError {
    Config(String),
    Database(String),
}
```

## The Individual Steps

Each step is a focused effect:

```rust
use effect::{Effect, effect, succeed, fail};

fn load_config() -> Effect<Config, AppError, ()> {
    // In a real app, read from a file or env vars
    succeed(Config {
        db_url: "postgres://localhost/myapp".to_string(),
        app_name: "Greeter".to_string(),
    })
}

fn connect_db(config: &Config) -> Effect<Database, AppError, ()> {
    Database::connect(&config.db_url)
        .map_error(|e| AppError::Database(format!("connect: {e}")))
}

fn fetch_user(db: &Database, id: u64) -> Effect<User, AppError, ()> {
    db.query_user(id)
        .map_error(|e| AppError::Database(format!("query: {e}")))
}

fn format_greeting(config: &Config, user: &User) -> String {
    format!("{}: Hello, {}! ({})", config.app_name, user.name, user.email)
}
```

## Composing the Program

Now we compose these steps into one effect using `effect!`:

```rust
fn greet_user(user_id: u64) -> Effect<String, AppError, ()> {
    effect! {
        let config = ~ load_config();
        let db     = ~ connect_db(&config);
        let user   = ~ fetch_user(&db, user_id);
        format_greeting(&config, &user)
    }
}
```

Read it like a recipe:
1. Load config — if it fails, stop with `AppError::Config`
2. Connect to DB — if it fails, stop with `AppError::Database`  
3. Fetch user — if it fails, stop with `AppError::Database`
4. Format the greeting — this is pure, always succeeds

Nothing has run yet. `greet_user(42)` is a value.

## Running It

At the edge of the program — in `main` — we execute:

```rust
fn main() {
    match run_blocking(greet_user(42)) {
        Ok(greeting) => println!("{greeting}"),
        Err(AppError::Config(msg)) => eprintln!("Config error: {msg}"),
        Err(AppError::Database(msg)) => eprintln!("DB error: {msg}"),
    }
}
```

## Testing It

Because the effect is a description, testing is straightforward — just swap out the underlying steps:

```rust
#[test]
fn test_greeting_format() {
    let effect = effect! {
        let config = ~ succeed(Config {
            db_url: "unused".into(),
            app_name: "TestApp".into(),
        });
        let user = ~ succeed(User {
            id: 1,
            name: "Alice".into(),
            email: "alice@example.com".into(),
        });
        format_greeting(&config, &user)
    };

    let result = run_test(effect);
    assert_eq!(result.unwrap(), "TestApp: Hello, Alice! (alice@example.com)");
}
```

No mocking framework. No `Arc<dyn Trait>` plumbing. Just substitute different `succeed` values for the steps you want to control.

## What You Just Learned

You've written a complete effect-based program. Along the way you used:

- `succeed` and `fail` to construct effects from values
- `.map` and `.map_error` to transform success and error types
- `effect! { ~ ... }` to sequence effects without callback nesting
- `run_blocking` to execute at the program edge
- `run_test` to verify behaviour in tests

That's the core of 90% of what you'll write day-to-day. The next two chapters go deeper: Chapter 3 explores the `effect!` macro in detail, and Chapter 4 begins the tour of `R` — the environment type that makes dependency injection a compile-time guarantee.

You just wrote your first effect-based program. It won't be your last.
