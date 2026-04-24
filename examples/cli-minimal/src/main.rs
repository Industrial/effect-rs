//! Minimal CLI: [`clap`] → [`cli_minimal::app_effect`] → [`id_effect_cli::run_main`].

use clap::Parser;
use id_effect_cli::{RunMainConfig, run_main};

#[derive(Parser, Debug)]
#[command(
  name = "cli-minimal",
  about = "Phase E example (see mdBook CLI chapter)"
)]
struct Cli {
  /// API token (loaded into config as `API_TOKEN`, then wrapped as [`id_effect_config::Secret`]).
  #[arg(long)]
  token: String,
}

fn main() -> std::process::ExitCode {
  let cli = Cli::parse();
  let (eff, env) = cli_minimal::app_effect(cli.token);
  run_main(eff, env, RunMainConfig::with_tracing())
}
