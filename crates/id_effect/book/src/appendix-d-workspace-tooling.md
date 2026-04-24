# Workspace tooling (macros and lints)

This appendix covers **authoring** and **static analysis** pieces that most application readers skip—but contributors and advanced users need to know where they live.

## `id_effect_macro` and `id_effect_proc_macro`

The **`effect!`** do-notation macro is split across:

- **`id_effect_proc_macro`** — procedural macro crate (actual `TokenStream` → `TokenStream` expansion).
- **`id_effect_macro`** — user-facing definitions and re-exports consumed as a normal dependency.

When debugging “why doesn’t my `effect!` compile?”, use **`cargo expand`** on a small repro and inspect the generated bind chain. Application code should keep following [The effect! Macro](../part1/ch03-00-effect-macro.md); these crates are implementation details unless you extend the macro system.

## `id_effect_lint`

Custom **Rustc lint** crate for id_effect-specific rules lives at **`crates/id_effect_lint`**. It is **excluded** from the default workspace members in the root **`Cargo.toml`** so normal **`cargo check --workspace`** stays fast; enable it explicitly when working on lint rules or wiring CI that builds the lint driver.

For day-to-day coding, rely on **`clippy`** plus repository **[`TESTING.md`](../../../TESTING.md)**; treat **`id_effect_lint`** as an additional enforcement layer when integrated into your compiler invocation.
