# CLI entrypoints with `clap` + `Effect`

Rust’s standard answer for parsing is **[`clap`](https://docs.rs/clap)**. For **id_effect**, treat the binary as a **thin shell**:

1. **Parse** argv into a struct (`#[derive(clap::Parser)]`).
2. **Assemble** layers / `MapConfigProvider` / other `R` values from flags and environment.
3. **Run** your program `Effect<A, E, R>` with [`run_blocking`](https://docs.rs/id_effect/latest/id_effect/runtime/fn.run_blocking.html) (or an async driver when you integrate Tokio).
4. **Exit** with [`std::process::ExitCode`] using [`id_effect_cli`](https://docs.rs/id_effect_cli) helpers (see the next sections).

This mirrors Effect.ts **`@effect/cli`**: declarative argument models composed into a single program — here the “program” is an `Effect` value rather than a `Future` chain.

## Recommended `main` shape (Rust 1.61+)

Returning [`ExitCode`](https://doc.rust-lang.org/std/process/struct.ExitCode.html) keeps `main` testable and avoids calling [`std::process::exit`](https://doc.rust-lang.org/std/process/fn.exit.html) deep inside library code:

```rust
use clap::Parser;
use id_effect_cli::{run_main, RunMainConfig};

#[derive(Parser)]
struct Cli {
    #[arg(long)]
    name: String,
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let eff = my_app(cli.name);
    run_main(eff, my_env(), RunMainConfig::with_tracing())
}
```

`my_app` returns an `Effect<…>`; `my_env()` is whatever `R` your effect needs (often a [`Context`](https://docs.rs/id_effect/latest/id_effect/context/struct.Context.html) built from [`Layer`](https://docs.rs/id_effect/latest/id_effect/layer/struct.Layer.html) stacks).

## Workspace helpers

The **`id_effect_cli`** crate (same repository) provides:

- Optional **`clap`** dependency (feature flag; on by default for convenience).
- [`run_main`](https://docs.rs/id_effect_cli/latest/id_effect_cli/fn.run_main.html) — optional tracing install, [`run_blocking`](https://docs.rs/id_effect/latest/id_effect/runtime/fn.run_blocking.html), stderr logging for `Err`, [`ExitCode`](https://doc.rust-lang.org/std/process/struct.ExitCode.html) mapping.
- [`exit_code_for_exit`](https://docs.rs/id_effect_cli/latest/id_effect_cli/fn.exit_code_for_exit.html) / [`exit_code_for_cause`](https://docs.rs/id_effect_cli/latest/id_effect_cli/fn.exit_code_for_cause.html) when you already have an [`Exit`](https://docs.rs/id_effect/latest/id_effect/enum.Exit.html) from [`run_test`](https://docs.rs/id_effect/latest/id_effect/fn.run_test.html) or a supervisor.

## Template

See the checked-in example **[`examples/cli-minimal`](https://github.com/Industrial/id_effect/tree/main/examples/cli-minimal)** (`--token …`, `Secret` via [`id_effect_config`](../part2/ch07-10-config.md)).

## Further reading

- [Exit codes for `main`](./ch16-01-cli-exit-codes.md) — `Exit` / `Cause` → `ExitCode` table
- [Config + `Secret` from flags](./ch16-02-cli-config-secret.md) — wiring `id_effect_config` at the edge
- [Error handling](./ch08-00-error-handling.md) — [`Cause`](https://docs.rs/id_effect/latest/id_effect/enum.Cause.html) vs plain `E`
- [Configuration (`id_effect_config`)](../part2/ch07-10-config.md)
