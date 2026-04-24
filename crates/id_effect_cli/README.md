# `id_effect_cli`

Thin helpers for building **CLIs whose bodies are [`Effect`](https://docs.rs/id_effect/latest/id_effect/kernel/struct.Effect.html) programs**, aligned with [Phase E — CLI ergonomics](../../docs/effect-ts-parity/phases/phase-e-cli.md):

- Map [`Exit`](https://docs.rs/id_effect/latest/id_effect/enum.Exit.html) / [`Cause`](https://docs.rs/id_effect/latest/id_effect/enum.Cause.html) to [`std::process::ExitCode`].
- Optional [`clap`](https://docs.rs/clap) (feature `clap`, on by default).
- [`run_main`](https://docs.rs/id_effect_cli/latest/id_effect_cli/fn.run_main.html) — optional tracing install via [`install_tracing_layer`](https://docs.rs/id_effect/latest/id_effect/fn.install_tracing_layer.html), then [`run_blocking`](https://docs.rs/id_effect/latest/id_effect/runtime/fn.run_blocking.html), then stderr logging and exit code mapping.

See the mdBook chapter **CLI entrypoints (`id_effect_cli`)** under *Part II → Services*, the [`examples/cli-minimal`](../../examples/cli-minimal/) template, and crate docs on [docs.rs](https://docs.rs/id_effect_cli).
