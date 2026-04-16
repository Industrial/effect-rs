//! Extract [`EffectLogger`] once with `~EffectLogger`, then call its methods
//! as `~logger.level(…)` steps — each returns `Effect<(), EffectLoggerError, R>`.
//!
//! Run: `RUST_LOG=trace devenv shell -- cargo run -p logger --example log_effects`

use ::id_effect::{Cons, Context, Effect, Nil, Service, effect, run_blocking};
use id_effect_logger::{EffectLogKey, EffectLogger, EffectLoggerError};

type LogCtx = Context<Cons<Service<EffectLogKey, EffectLogger>, Nil>>;

fn build_ctx() -> LogCtx {
  Context::new(Cons(Service::<EffectLogKey, _>::new(EffectLogger), Nil))
}

fn main() {
  tracing_subscriber::fmt()
    .with_env_filter(
      tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("trace")),
    )
    .init();

  let prog: Effect<(), EffectLoggerError, LogCtx> = effect!(|_r: &mut LogCtx| {
    let logger = ~EffectLogger;
    ~logger.trace("trace step");
    ~logger.debug("debug step");
    ~logger.info("info step");
    ~logger.warn("warn step");
    ~logger.error("error step");
  });

  run_blocking(prog, build_ctx()).expect("tracing never fails");
  println!("ran all five log levels via ~EffectLogger");
}
