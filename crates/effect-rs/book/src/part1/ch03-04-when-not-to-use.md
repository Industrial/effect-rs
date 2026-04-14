# When Not to Use the Macro

`effect!` is the idiomatic choice for most multi-step computations. But it's a macro — which means it has edges. Knowing when to reach for raw `flat_map` instead saves debugging time.

## Use Raw flat_map for Single-Step Transforms

When there's exactly one effectful step and you're transforming its result, `flat_map` is cleaner:

```rust
// Unnecessarily verbose
effect! {
    let id = ~ parse_id(raw);
    id
}

// Clear and direct
parse_id(raw).flat_map(|id| succeed(id))
// or just:
parse_id(raw)
```

Use `effect!` when you have two or more sequential steps. For one, `flat_map` or `.map` is usually enough.

## Use Combinators for Structural Patterns

Some patterns have named combinators that are more expressive than macros:

```rust
// Instead of:
effect! {
    let a = ~ step_a();
    let b = ~ step_b();
    (a, b)
}

// Consider (when steps are independent):
step_a().zip(step_b())
```

`zip` communicates intent: "I need both, in any order." The `effect!` version implies sequential dependency. For independent steps, prefer explicit combinators. (For *concurrent* independent steps, see `fiber_all` in Chapter 9.)

## Avoid Deep Nesting Within the Block

The macro eliminates nesting between `flat_map` chains. But you can still create nested `effect!` blocks, which gets confusing:

```rust
// CONFUSING — nested macro bodies
effect! {
    let result = ~ effect! {      // inner macro
        let x = ~ inner_step();
        x * 2
    };
    result + 1
}

// BETTER — flatten it
effect! {
    let x = ~ inner_step();
    let result = x * 2;
    result + 1
}
```

If you feel the urge to nest `effect!` inside `effect!`, flatten the outer block instead.

## The Macro and Type Inference

The macro occasionally confuses the type inferencer, especially when the error type isn't pinned early. If you see cryptic "can't infer type" errors inside `effect!`:

1. Annotate the return type of the enclosing function explicitly
2. Add a `.map_error(Into::into)` on the first `~` binding to anchor `E`
3. As a last resort, break out the inner logic into a named helper function

## When Generic Returns Are Needed

Library code with polymorphic `A, E, R` sometimes can't use the macro cleanly:

```rust
// This works fine with explicit function + effect!
pub fn load_config<A, E, R>() -> Effect<A, E, R>
where
    A: From<Config> + 'static,
    E: From<ConfigError> + 'static,
    R: 'static,
{
    effect!(|_r: &mut R| {
        let cfg = read_env_config()?;
        A::from(cfg)
    })
}
```

The closure form of `effect!` (with `|_r: &mut R|`) is the right tool for generic graph-builder functions. It's still the macro, just in its raw form.

## Summary

| Situation | Prefer |
|-----------|--------|
| 2+ sequential steps | `effect! { ~ ... }` |
| 1 step, simple transform | `.map` / `.flat_map` |
| Independent steps | `.zip` / combinators |
| Generic `<A, E, R>` graph builder | `effect!(|_r: &mut R| { ... })` |
| Structural patterns (zip, race, all) | explicit combinators, not macro |

The macro is a tool, not a religion. Use it when it makes the code read like a story; use combinators when they express intent more directly.
