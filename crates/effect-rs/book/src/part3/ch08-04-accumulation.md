# Error Accumulation — Collecting All Failures

`catch` and `fold` handle errors sequentially: one effect, one error, one handler. But sometimes you need to run many operations and collect *all* their failures — not just the first.

## The Fail-Fast Problem

Sequential `effect!` short-circuits on the first failure:

```rust
effect! {
    let _ = ~ validate_name(&input.name);    // fails here →
    let _ = ~ validate_email(&input.email);  // never runs
    let _ = ~ validate_age(input.age);       // never runs
}
```

For form validation or batch imports, you want to report all errors to the user, not just the first one.

## validate_all

`validate_all` runs a collection of effects and accumulates all failures:

```rust
use effect::validate_all;

let results = validate_all(vec![
    validate_name(&input.name),
    validate_email(&input.email),
    validate_age(input.age),
]);
// Type: Effect<Vec<Name, Email, Age>, Vec<ValidationError>, ()>
```

If any validations fail, all errors are collected and returned as a `Vec`. If all succeed, you get all the success values.

## partition

`partition` runs effects and splits the results into successes and failures:

```rust
use effect::partition;

let (successes, failures): (Vec<User>, Vec<ImportError>) =
    run_blocking(partition(records.iter().map(import_record)))?;

println!("{} imported, {} failed", successes.len(), failures.len());
```

`partition` never fails. It always returns two lists: what worked and what didn't. Useful for batch operations where partial success is acceptable.

## Or: Combining Two Error Types

When composing effects with different error types, `Or` avoids flattening into a single error type before you're ready:

```rust
use effect::Or;

// Instead of converting both to AppError immediately:
type BothErrors = Or<DbError, NetworkError>;

fn combined() -> Effect<Data, BothErrors, ()> {
    db_fetch()
        .map_error(Or::Left)
        .zip(network_fetch().map_error(Or::Right))
        .map(|(a, b)| merge(a, b))
}
```

`Or<A, B>` is the coproduct of two error types. It defers the decision of how to combine them until you actually need to handle them.

## The ParseErrors Type

The Schema module (Chapter 14) uses `ParseErrors` — a structured accumulator for parsing failures with field paths:

```rust
let result: Result<User, ParseErrors> = user_schema.parse(data);

if let Err(errors) = result {
    for e in errors.iter() {
        eprintln!("At {}: {}", e.path(), e.message());
    }
}
```

`ParseErrors` is specialised for schema validation, but the pattern — collect all, report all — applies whenever you validate structured input.

## When to Accumulate vs. Short-Circuit

| Situation | Use |
|-----------|-----|
| Dependent steps (each needs previous result) | `effect!` (short-circuit) |
| Independent validations (user input) | `validate_all` |
| Batch operations (partial success OK) | `partition` |
| Schema parsing | `ParseErrors` (automatic) |

The choice is about what makes sense to the caller. Short-circuit is efficient; accumulation is informative. Use the one your users need.
