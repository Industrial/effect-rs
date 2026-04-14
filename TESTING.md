# Testing Strategy

This document describes our testing approach: **TDD with BDD-style naming, module-based test trees, and rstest for parameterized tests**. The goal is 100% coverage of all input/output mutations.

## Core Principles

1. **Test-Driven Development (TDD)**: Write tests before or alongside implementation
2. **Behavior-Driven Development (BDD)**: Test names describe behavior, not implementation
3. **Mutation Coverage**: Every branch, every edge case, every output variant
4. **Module Test Trees**: Organize tests hierarchically by feature/function
5. **rstest for Parameterization**: Use `#[rstest]` with `#[case]` for input variations

---

## Test File Structure

Tests live in the same file as the code, inside a `#[cfg(test)]` module at the bottom:

```rust
// src/lib.rs or src/module.rs

pub fn calculate_spread_bps(bid: f64, ask: f64) -> Option<f64> {
    // implementation
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test modules and functions here
}
```

---

## Module Test Trees

Organize tests into a tree where **each branch represents a mutation** of inputs or behavior.

### Pattern 1: Nested Modules (Recommended for Complex Functions)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    mod function_name {
        use super::*;

        mod with_valid_input {
            use super::*;

            #[test]
            fn returns_expected_value() { }

            #[test]
            fn handles_boundary_case() { }
        }

        mod with_invalid_input {
            use super::*;

            #[test]
            fn returns_none_when_empty() { }

            #[test]
            fn returns_error_when_malformed() { }
        }
    }
}
```

### Pattern 2: Comment Dividers (For Simpler Modules)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // ---------- L1Snapshot ----------
    #[test]
    fn l1_snapshot_mid() { }

    #[test]
    fn l1_spread_bps_zero_mid() { }

    // ---------- Quote ----------
    #[test]
    fn quote_at_best_matches_snapshot() { }

    #[test]
    fn quote_at_mid_spread_symmetric() { }
}
```

---

## BDD-Style Test Naming

Test names should read as **behavior specifications**. Use this pattern:

```
<function>_<condition>_<expected_result>
```

Or for context-based naming:

```
<action>_when_<condition>
<action>_with_<input_type>
<action>_rejects_<invalid_case>
```

### Examples

```rust
// Function + scenario
fn pnl_gross_buy_profit() { }
fn pnl_gross_buy_loss() { }
fn pnl_gross_buy_zero_fill_price() { }

// Action + condition
fn new_rejects_empty_base() { }
fn new_rejects_empty_quote() { }
fn new_trims_whitespace() { }

// Behavior description
fn long_when_fast_above_slow() { }
fn flat_in_band() { }
fn simulate_empty_or_short() { }
```

### Anti-patterns (Avoid)

```rust
// Too vague
fn test_function() { }
fn it_works() { }

// Implementation-focused, not behavior-focused
fn test_branch_1() { }
fn test_line_42() { }
```

---

## rstest for Parameterized Tests

Use `rstest` when testing the same behavior with multiple inputs. Each case gets a descriptive name.

### Basic Parameterization

```rust
use rstest::rstest;

#[rstest]
#[case::above_slow(101.0, 100.0, Regime::Long)]
#[case::just_above_band(100.0021, 100.0, Regime::Long)]
fn long_when_fast_above_slow(
    #[case] ema_fast: f64,
    #[case] ema_slow: f64,
    #[case] expected: Regime,
) {
    assert_eq!(regime(ema_fast, ema_slow), expected);
}
```

### Multiple Test Groups with rstest

```rust
#[rstest]
#[case::not_a_valid_pubkey("not-a-valid-pubkey")]
#[case::empty("")]
#[case::too_short("123")]
#[case::invalid_chars("!!!")]
#[tokio::test]
async fn fails_with_invalid_pubkey_string(#[case] pubkey: &str) {
    let result = validate_pubkey(pubkey);
    assert!(result.is_err());
}
```

### When to Use rstest vs Separate Tests

**Use rstest when:**
- Testing the same assertion with different inputs
- Inputs naturally form a table of cases
- Cases share identical test logic

**Use separate tests when:**
- Different assertions for different cases
- Setup/teardown differs between cases
- Test logic varies significantly

---

## Helper Functions & Fixtures

Create helper functions to build test data. Keep them at the top of the `tests` module.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // ========== Fixtures ==========

    fn snapshot(bid: f64, ask: f64) -> L1Snapshot {
        L1Snapshot::new(bid, ask)
    }

    fn bar(close: f64, high: f64, low: f64) -> OracleBar5s {
        OracleBar5s {
            timestamp: Utc::now(),
            protocol: "pyth".to_string(),
            symbol: "SOL-USD".parse().unwrap(),
            open: close,
            high,
            low,
            close,
        }
    }

    fn default_config() -> MmConfig {
        MmConfig::default()
    }

    // ========== Tests ==========
    // ...
}
```

---

## Mutation Coverage Checklist

For every function, systematically test these mutations:

### Input Mutations

| Input Type | Test Cases |
|------------|------------|
| `f64` | positive, negative, zero, very small, very large, NaN, infinity |
| `&str` / `String` | valid, empty `""`, whitespace `"  "`, special chars |
| `Option<T>` | `Some(valid)`, `Some(edge)`, `None` |
| `Vec<T>` / `&[T]` | empty `[]`, single `[x]`, multiple `[x, y, z]` |
| `bool` | `true`, `false` |
| Enums | every variant |

### Output Mutations

| Return Type | Test Cases |
|-------------|------------|
| `bool` | cases that return `true`, cases that return `false` |
| `Option<T>` | cases that return `Some`, cases that return `None` |
| `Result<T, E>` | cases that return `Ok`, cases that return `Err` (each error variant) |
| Numeric | positive result, negative result, zero, boundary values |
| Struct | verify all fields are set correctly |

### Branch Coverage

For every `if`/`else`, `match`, or conditional:

```rust
// Code under test
fn regime(ema_fast: f64, ema_slow: f64) -> Regime {
    if ema_fast > ema_slow * 1.00002 {
        Regime::Long       // ← Test this branch
    } else if ema_fast < ema_slow * 0.99998 {
        Regime::Short      // ← Test this branch
    } else {
        Regime::Flat       // ← Test this branch
    }
}

// Tests cover all three branches
#[test] fn long_when_fast_above_slow() { }
#[test] fn short_when_fast_below_slow() { }
#[test] fn flat_in_band() { }
```

### Boundary Testing

Test values at exact boundaries:

```rust
// Boundary: fill when next_mid >= our_bid
#[test]
fn detect_fills_exact_bid_touch() {
    let (buy, sell) = detect_fills(100.0, 102.0, 100.0); // exactly at bid
    assert_eq!(buy, Some(100.0));
}

#[test]
fn detect_fills_just_below_bid() {
    let (buy, sell) = detect_fills(100.0, 102.0, 99.99); // just below
    assert_eq!(buy, None);
}
```

---

## Test Tree Example: Complete Coverage

Given this function:

```rust
pub fn quote_from_config(snapshot: L1Snapshot, config: &MmConfig) -> Option<Quote> {
    if let Some(min_bps) = config.min_spread_bps_to_quote {
        if snapshot.spread_bps() < min_bps {
            return None;
        }
    }
    Some(quote_at_mid_spread(snapshot.mid, config.spread_bps))
}
```

The test tree covers all mutations:

```
quote_from_config/
├── with_min_spread_configured/
│   ├── returns_none_when_spread_too_tight
│   └── returns_quote_when_spread_wide_enough
├── without_min_spread_configured/
│   └── always_returns_quote
└── output_verification/
    └── quote_uses_config_spread_bps
```

```rust
mod quote_from_config {
    use super::*;

    mod with_min_spread_configured {
        use super::*;

        #[test]
        fn returns_none_when_spread_too_tight() {
            let s = snapshot(99.99, 100.01); // 2 bps spread
            let config = MmConfig {
                min_spread_bps_to_quote: Some(5.0),
                ..Default::default()
            };
            assert!(quote_from_config(s, &config).is_none());
        }

        #[test]
        fn returns_quote_when_spread_wide_enough() {
            let s = snapshot(99.0, 101.0); // ~200 bps
            let config = MmConfig {
                min_spread_bps_to_quote: Some(5.0),
                ..Default::default()
            };
            assert!(quote_from_config(s, &config).is_some());
        }
    }

    mod without_min_spread_configured {
        use super::*;

        #[test]
        fn always_returns_quote() {
            let s = snapshot(99.99, 100.01); // tight spread
            let config = MmConfig {
                min_spread_bps_to_quote: None,
                ..Default::default()
            };
            assert!(quote_from_config(s, &config).is_some());
        }
    }

    mod output_verification {
        use super::*;

        #[test]
        fn quote_uses_config_spread_bps() {
            let s = snapshot(99.0, 101.0);
            let config = MmConfig {
                spread_bps: 5.0,
                min_spread_bps_to_quote: None,
                ..Default::default()
            };
            let q = quote_from_config(s, &config).unwrap();
            // mid = 100, spread_bps = 5 → bid = 99.95, ask = 100.05
            assert!((q.bid - 99.95).abs() < 1e-10);
            assert!((q.ask - 100.05).abs() < 1e-10);
        }
    }
}
```

---

## Async Tests

For async functions, use `#[tokio::test]`:

```rust
#[tokio::test]
async fn succeeds_with_valid_config() {
    let result = fetch_data("https://api.example.com/endpoint", &key).await;
    assert!(result.is_ok());
}
```

With rstest:

```rust
#[rstest]
#[case::staging("https://api.staging.example.com")]
#[case::production("https://api.example.com")]
#[tokio::test]
async fn connects_to_service(#[case] base_url: &str) {
    let result = connect(base_url).await;
    assert!(result.is_ok());
}
```

---

## Environment Variables in Tests

Use `temp-env` for tests that depend on environment variables:

```rust
use temp_env::async_with_vars;

#[tokio::test]
async fn uses_base_url_from_env() {
    temp_env::async_with_vars(
        [("BASE_URL", Some("https://api.example.com"))],
        async {
            let config = Config::from_env();
            assert_eq!(config.base_url, "https://api.example.com");
        },
    )
    .await;
}
```

---

## Integration Tests

For tests that require external services (database, NATS, RPC):

1. Mark with `#[ignore]` for CI skip
2. Document required setup in test docstring
3. Use `-- --ignored` to run manually

```rust
use effect::run_async;

/// Integration test: requires DATABASE_URL pointing to running PostgreSQL.
/// Run with: `cargo test -p my-crate integration_test_name -- --ignored`
#[tokio::test]
#[ignore]
async fn example_db_integration_requires_postgres() {
    let _url = std::env::var("DATABASE_URL").expect("DATABASE_URL required");
    // ... test logic
}
```

---

## Running Tests

**Quick Rust checks:** `moon run :format` and `moon run :check`, or `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets --all-features -- -D warnings`. This does not replace **`moon run :coverage`** or the full pre-push pipeline — see root **`README.md`** Development section.

```bash
# Run all tests
cargo test

# Run tests for a specific crate
cargo test -p effect-rs

# Run tests matching a pattern
cargo test quote_from_config

# Run ignored integration tests
cargo test -- --ignored

# Run with output (see println! etc)
cargo test -- --nocapture

# Run a specific test
cargo test quote_from_config_skips_when_spread_too_tight
```

---

## Coverage Goals

| Metric | Target |
|--------|--------|
| Line coverage | 95%+ |
| Branch coverage | 100% |
| Function coverage | 100% |
| Mutation score | 90%+ |

### Policy (strict)

Meeting the **95%** llvm-cov gates is done by **adding and maintaining real tests** (and refactoring for testability when needed). The following are **not** acceptable ways to “pass” coverage:

- Lowering `--fail-under-*` thresholds or removing gates from the `coverage` task
- Using `cargo llvm-cov` **`--ignore-filename-regex`** (or equivalent) to drop files from the denominator
- Changing Moon/Cargo test tasks only to skip failing suites or hide uncovered code
- Disabling Clippy, tests, or coverage in CI to greenwash the metric

**Do not change this document’s targets or the tooling setup to make the numbers “match.”** The percentages in the table above and the **`moon run :coverage`** / **`cargo llvm-cov nextest`** wiring (workspace scope, nextest filters, fail-under values, absence of excludes) are fixed requirements: improve code and tests until the gates pass—do not edit `TESTING.md`, `moon.yml`, or crate test tasks to relax, reword, or bypass them.

The workspace root **`moon run :coverage`** task runs **`cargo llvm-cov nextest`** on the **entire** workspace with **`--fail-under-lines` / `--fail-under-regions` / `--fail-under-functions` all at 95** and **no** filename excludes.

For a local HTML report without failing on thresholds:

```bash
devenv shell -- cargo llvm-cov nextest --html --fail-under-lines 0
```

---

## Checklist for New Code

Before submitting code, verify:

- [ ] Every public function has tests
- [ ] Every branch is covered
- [ ] Every `Option`/`Result` variant is tested
- [ ] Edge cases (empty, zero, boundary) are tested
- [ ] Error paths are tested
- [ ] Test names describe behavior
- [ ] Tests are organized in module tree
- [ ] rstest used for parameterized cases
- [ ] Async tests use `#[tokio::test]`
- [ ] Integration tests marked `#[ignore]`
