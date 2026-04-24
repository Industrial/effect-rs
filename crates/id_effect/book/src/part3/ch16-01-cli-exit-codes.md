# Exit codes for `main`

When a CLI finishes, the OS only sees an **8-bit exit status**. **id_effect** distinguishes richer outcomes ([`Exit`](https://docs.rs/id_effect/latest/id_effect/enum.Exit.html), [`Cause`](https://docs.rs/id_effect/latest/id_effect/enum.Cause.html)); at the process edge you collapse that into [`std::process::ExitCode`](https://doc.rust-lang.org/std/process/struct.ExitCode.html).

## `Result` from `run_blocking`

[`run_blocking`](https://docs.rs/id_effect/latest/id_effect/runtime/fn.run_blocking.html) returns `Result<A, E>`:

| Outcome | Suggested CLI byte | Notes |
|---------|-------------------|--------|
| `Ok(_)` | `0` | success |
| `Err(_)` | `1` | typed / expected failure ([`id_effect_cli::exit_code_for_result`](https://docs.rs/id_effect_cli/latest/id_effect_cli/fn.exit_code_for_result.html)) |

[`id_effect_cli::run_main`](https://docs.rs/id_effect_cli/latest/id_effect_cli/fn.run_main.html) uses this mapping and prints `Err` with `Debug` to **stderr**.

## Full `Exit` / `Cause`

When you have an [`Exit`](https://docs.rs/id_effect/latest/id_effect/enum.Exit.html) (for example from tests or a custom driver), map **leaf** causes as follows — composite [`Cause::Both`](https://docs.rs/id_effect/latest/id_effect/enum.Cause.html#variant.Both) / [`Cause::Then`](https://docs.rs/id_effect/latest/id_effect/enum.Cause.html#variant.Then) use the **maximum** byte so “stronger” failures dominate:

| Pattern | Byte | Meaning |
|---------|------|---------|
| [`Exit::Success`](https://docs.rs/id_effect/latest/id_effect/enum.Exit.html#variant.Success) | `0` | OK |
| [`Cause::Fail`](https://docs.rs/id_effect/latest/id_effect/enum.Cause.html#variant.Fail)(_) | `1` | expected typed failure |
| [`Cause::Die`](https://docs.rs/id_effect/latest/id_effect/enum.Cause.html#variant.Die)(_) | `101` | defect (panic-style message) |
| [`Cause::Interrupt`](https://docs.rs/id_effect/latest/id_effect/enum.Cause.html#variant.Interrupt)(_) | `130` | cancellation (same convention many shells use for **SIGINT**) |
| [`Cause::Both`](https://docs.rs/id_effect/latest/id_effect/enum.Cause.html#variant.Both) / [`Cause::Then`](https://docs.rs/id_effect/latest/id_effect/enum.Cause.html#variant.Then) | `max(left, right)` | recurse |

Helpers: [`exit_code_for_exit`](https://docs.rs/id_effect_cli/latest/id_effect_cli/fn.exit_code_for_exit.html), [`exit_code_for_cause`](https://docs.rs/id_effect_cli/latest/id_effect_cli/fn.exit_code_for_cause.html), [`cause_max_exit_byte`](https://docs.rs/id_effect_cli/latest/id_effect_cli/fn.cause_max_exit_byte.html).

## Practical guidance

- Use **`1`** for “the command could not complete successfully” (missing flag, validation error, upstream HTTP 4xx mapped to your `E`, …).
- Reserve **`101`** for “the program detected an internal defect” — corresponds to [`Cause::Die`](https://docs.rs/id_effect/latest/id_effect/enum.Cause.html#variant.Die) in structured runs (see [`cause_max_exit_byte`](https://docs.rs/id_effect_cli/latest/id_effect_cli/fn.cause_max_exit_byte.html)).
- Use **`130`** only when you surface fiber **interruption** to the process edge (rare in simple CLIs).

If you need richer machine output, prefer **stderr JSON** or a dedicated output file — do not overload exit codes beyond what your operators can act on.
