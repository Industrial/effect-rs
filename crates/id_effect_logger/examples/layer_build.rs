//! Build the logger layer with [`id_effect_logger::layer_effect_logger`]
//! ([`id_effect::layer_service`] / Effect.ts `Layer.succeed`), then assemble a
//! [`Context`] and run an effect that extracts `EffectLogger` via `~EffectLogger`.
//!
//! Run: `devenv shell -- cargo run -p logger --example layer_build`

use ::id_effect::{Cons, Context, Effect, Layer, Nil, Service, effect, run_blocking};
use id_effect_logger::{EffectLogKey, EffectLogger, EffectLoggerError, layer_effect_logger};

type LogEnv = Context<Cons<Service<EffectLogKey, EffectLogger>, Nil>>;

fn build_env() -> LogEnv {
  let cell = layer_effect_logger()
    .build()
    .expect("layer_effect_logger is infallible");
  Context::new(Cons(cell, Nil))
}

fn main() {
  tracing_subscriber::fmt()
    .with_env_filter(tracing_subscriber::EnvFilter::new("info"))
    .init();

  let prog: Effect<(), EffectLoggerError, LogEnv> = effect!(|_r: &mut LogEnv| {
    let logger = ~EffectLogger;
    ~logger.info("logger provided via layer_effect_logger build");
  });

  let result: Result<(), EffectLoggerError> = run_blocking(prog, build_env());
  println!("{result:?}");
}
