# R as Documentation — Self-Describing Functions

The `R` parameter is often described as "the environment type." That's true, but it undersells the practical benefit. `R` is *living documentation* that the compiler enforces.

## The Signature Tells the Story

Consider two versions of the same function:

```rust
// Version A: traditional async
async fn process_order(order: Order) -> Result<Receipt, Error> {
    // What does this use? Read the body to find out.
    // Database? PaymentGateway? Email? Metrics?
    // You'll have to trace through 200 lines to know.
}

// Version B: effect-based
fn process_order(order: Order) -> Effect<Receipt, OrderError, (Database, PaymentGateway, EmailService, Logger)> {
    // What does this use? Look at the signature.
    // Database ✓, PaymentGateway ✓, EmailService ✓, Logger ✓
    // Done.
}
```

Version B's type is self-describing. You don't need to read the implementation to understand its dependency surface.

## Code Review Benefits

In a pull request, `R` changes are visible in the diff. If someone adds a call to `send_metrics()` inside `process_order` and the `MetricsClient` wasn't previously in `R`, the function signature must change:

```diff
- fn process_order(order: Order) -> Effect<Receipt, OrderError, (Database, PaymentGateway, EmailService, Logger)>
+ fn process_order(order: Order) -> Effect<Receipt, OrderError, (Database, PaymentGateway, EmailService, Logger, MetricsClient)>
```

This diff is in the function signature — impossible to miss. With traditional parameters or singletons, new dependencies can silently appear in implementation bodies.

## Refactoring Safety

When you refactor and remove a dependency, the compiler finds all the places that provided the now-unnecessary value. The `R` type shrinks, and all the callers that were providing the removed dep get a compile error saying they're providing something no longer needed.

```rust
// After removing Logger from process_order:

// This now fails to compile — .provide(my_logger) is unnecessary
run_blocking(
    process_order(order)
        .provide(my_db)
        .provide(my_logger) // ERROR: Logger is not part of R anymore
)?;
```

The compiler guides you to clean up callers. Traditional code leaves stale dependencies silently lingering.

## Testing Clarity

When writing a test, `R` tells you exactly what you need to mock:

```rust
#[test]
fn test_process_order() {
    // R = (Database, PaymentGateway, EmailService, Logger)
    // So the test needs these four — no more, no less
    let result = run_test(
        process_order(test_order()),
        (mock_db(), mock_payment(), mock_email(), test_logger()),
    );
    assert!(result.is_ok());
}
```

There's no "I wonder if this also touches the metrics service" uncertainty. The type says it doesn't. If you're missing a mock, the code won't compile.

## R is Not Magic

It's important to understand that `R` is just a type parameter. The "compile-time DI" property comes from:

1. Functions declaring what they need in `R`
2. The runtime refusing to execute unless `R = ()`
3. Composition automatically merging requirements

There's no reflection, no registration, no framework. Just types.

The next chapter shows how `Tags` and `Context` make this scale beyond simple tuples — handling large, complex dependency graphs without positional ambiguity.
