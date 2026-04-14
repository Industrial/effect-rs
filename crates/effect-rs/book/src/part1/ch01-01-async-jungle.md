# The Async Jungle

It's 2 AM. Your pager went off twenty minutes ago. The dashboard is a sea of red, and somewhere in your microservices architecture, something is very, very broken.

The error message says `connection reset`. Helpful. Which connection? The one to Postgres? Redis? The upstream payment API? Your service opens all three, and the stack trace is a maze of `tokio::spawn` and `.await` points that could be anywhere.

You scroll through the logs. There are thousands of them, all interleaved, none of them with enough context to tell you which request they belong to. You added a request ID to the logs once, but half your functions don't have access to it because it would mean threading yet another parameter through the entire call stack.

You've been here before. We all have.

Or maybe your nightmare looks different. Maybe it's a PR review where you're staring at 400 lines of changes, and 350 of them are just threading a new `MetricsClient` parameter through every function between `main()` and the code that actually uses it. The reviewer asks if you considered using dependency injection, and you laugh because you *are* using dependency injection — this *is* what dependency injection looks like in your codebase.

These aren't hypotheticals. These are Tuesdays.

Let's examine three specific patterns that make async Rust harder than it needs to be. We call them the Three Horsemen of Async Pain.

## Horseman #1: Error Handling Spaghetti

Here's a function you've probably written some version of:

```rust
async fn process_order(order: Order) -> Result<Receipt, ProcessError> {
    let config = get_config()
        .await
        .map_err(|e| ProcessError::Config(e))?;

    let user = fetch_user(&config, order.user_id)
        .await
        .map_err(|e| ProcessError::User(e))?;

    let inventory = check_inventory(&config, &order.items)
        .await
        .map_err(|e| ProcessError::Inventory(e))?;

    let payment = charge_payment(&config, &user, order.total)
        .await
        .map_err(|e| ProcessError::Payment(e))?;

    let shipment = create_shipment(&config, &order, &user)
        .await
        .map_err(|e| ProcessError::Shipment(e))?;

    Ok(Receipt::new(order, payment, shipment))
}
```

Look at that `.map_err().await?` chain. The actual business logic — fetch user, check inventory, charge payment, create shipment — is drowning in error conversion noise.

And that `ProcessError` enum? It's got five variants now. Wait until you add logging, retry logic, and fallback paths. You'll be up to fifteen variants and a `From` impl for each one.

The worst part: this function doesn't even *handle* errors. It just converts them and propagates them upward. The actual error handling — the retries, the fallbacks, the graceful degradation — lives somewhere else, bolted on from the outside, probably inconsistently.

## Horseman #2: Dependency Smuggling

Now look at a typical handler function:

```rust
async fn handle_request(
    db: &DatabasePool,
    cache: &RedisClient,
    logger: &Logger,
    config: &AppConfig,
    metrics: &MetricsClient,
    tracer: &Tracer,
    request: Request,
) -> Response {
    // Finally, actual code
}
```

Six dependencies. And this is a *simple* handler. Production code often has more.

Every function between `main()` and `handle_request` needs to pass these through. Every test needs to construct or mock all of them. Every refactor that adds a new dependency touches dozens of files.

"Use dependency injection!" someone says. You *are*. This is what explicit dependency injection looks like. The dependencies are injected. They're just smuggled through every layer of your code like contraband through airport security.

Some teams reach for globals. `lazy_static!` a database pool, `thread_local!` a request context. It works until you need to test with different configurations, or until you forget to initialize something in the right order, or until you spend a week debugging a race condition in your "thread-safe" global logger initialization.

## Horseman #3: The Runaway Task

Here's a pattern that looks harmless:

```rust
fn start_background_worker(db: DatabasePool) {
    tokio::spawn(async move {
        loop {
            match process_queue(&db).await {
                Ok(_) => {}
                Err(e) => eprintln!("Worker error: {}", e),
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });
}
```

That task runs forever. Or does it? Who owns it? What happens when your application shuts down — does the worker finish its current job, or does it get killed mid-transaction?

If `process_queue` acquires a database connection, does that connection get returned to the pool if the task is cancelled? If the task panics, do you even find out?

Spawning tasks this way is like launching missiles: easy to start, hard to recall, and occasionally catastrophic when left unattended.

Structured concurrency — the idea that spawned tasks have clear ownership and predictable lifetimes — is something other languages have figured out. In raw async Rust, you're on your own.

## The Root Cause

These three problems look different on the surface:
- Error handling is about types and conversion
- Dependency injection is about function signatures
- Task management is about concurrency

But they share a root cause: **async functions do things immediately**.

When you call `fetch_user().await`, the HTTP request fires. Right now. You can't inspect what it's going to do. You can't compose it with other operations before committing. You can't decide later whether to run it or not.

Your code is a series of *commands*: do this, then do that, handle whatever happens. You're a general shouting orders, hoping your troops don't get killed before they report back.

What if, instead, your code was a series of *descriptions*? What if `fetch_user(id)` didn't *fetch* a user — it described *how* to fetch a user? What if you could take that description, combine it with other descriptions, add error handling and retry logic, specify what resources it needs, and only then, when everything is ready, say "okay, now actually do it"?

That would change everything.

Your error handling would be part of the description, not bolted on afterward. Your dependencies would be declared in the description's type, not threaded through every function. Your tasks would have structure, because the runtime would know exactly what each task intends to do before it does it.

That's what we're going to build.

But first, we need to understand what an Effect actually is. That's the next section.
