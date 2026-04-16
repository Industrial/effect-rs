//! [`EffectLogger`] as a service value: each method returns an
//! `Effect<(), EffectLoggerError, R>` that is run with [`id_effect::run_blocking`]
//! against a `()` environment (the logger is self-contained after extraction).
//!
//! Run: `RUST_LOG=debug devenv shell -- cargo run -p logger --example effect_logger`

use ::id_effect::run_blocking;
use id_effect_logger::{EffectLogger, LogLevel};

fn main() {
  tracing_subscriber::fmt()
    .with_env_filter(
      tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("trace")),
    )
    .init();

  let log = EffectLogger;
  run_blocking(
    log.trace::<()>("trace line (raise RUST_LOG=trace to see)"),
    (),
  )
  .unwrap();
  run_blocking(
    log.debug::<()>("debug line (raise RUST_LOG=debug to see)"),
    (),
  )
  .unwrap();
  run_blocking(log.info::<()>("info: EffectLogger methods work"), ()).unwrap();
  run_blocking(log.warn::<()>("warn example"), ()).unwrap();
  run_blocking(log.error::<()>("error example"), ()).unwrap();

  // .log() also accepts an explicit LogLevel:
  run_blocking(
    log.log::<()>(LogLevel::Info, "explicit level via .log()"),
    (),
  )
  .unwrap();
}
