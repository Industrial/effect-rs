//! CLI ergonomics for [`id_effect::Effect`] programs: **exit code mapping**, optional **[`clap`]**
//! integration, and a small **`run_main`** helper.
//!
//! Design matches [Phase E — CLI ergonomics](https://github.com/Industrial/id_effect/blob/main/docs/effect-ts-parity/phases/phase-e-cli.md):
//! embrace **`clap`** for parsing, run the effect with [`id_effect::runtime::run_blocking`], map
//! [`Result`](std::result::Result) / [`id_effect::Exit`] / [`id_effect::Cause`] to
//! [`std::process::ExitCode`] for `fn main() -> ExitCode`.
//!
//! [`clap`]: https://docs.rs/clap

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod exit_code;
mod run;

pub use exit_code::{
  cause_max_exit_byte, exit_code_for_cause, exit_code_for_exit, exit_code_for_result,
};
pub use run::{RunMainConfig, run_main};

#[cfg(feature = "clap")]
#[doc(inline)]
pub use clap;
