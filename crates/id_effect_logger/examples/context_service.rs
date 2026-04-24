//! Provide [`EffectLogger`] via a concrete [`Context`], then extract it inside
//! `effect!` with `~EffectLogger` and use its methods with `~`.
//!
//! Run: `devenv shell -- cargo run -p logger --example context_service`

use ::id_effect::{Cons, Context, Effect, Nil, Service, effect, run_blocking};
use id_effect_logger::{EffectLogKey, EffectLogger, EffectLoggerError};

type LogR = Context<Cons<Service<EffectLogKey, EffectLogger>, Nil>>;

fn build_ctx() -> LogR {
  Context::new(Cons(Service::<EffectLogKey, _>::new(EffectLogger), Nil))
}

fn main() {
  tracing_subscriber::fmt()
    .with_env_filter(tracing_subscriber::EnvFilter::new("info"))
    .init();

  let prog: Effect<(), EffectLoggerError, LogR> = effect!(|_r: &mut LogR| {
    let logger = ~EffectLogger;
    ~logger.info("resolved EffectLogger via Context + provide_service");
    ~logger.warn("warn from the same extracted logger");
  });

  run_blocking(prog, build_ctx()).expect("EffectLoggerError is never produced by tracing");
}
