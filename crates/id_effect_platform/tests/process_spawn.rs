use id_effect::{Cons, Context, Layer, Nil, run_async};
use id_effect_platform::process::{
  CommandSpec, ProcessRuntimeKey, TokioProcessRuntime, layer_process_runtime, spawn_wait,
};

type Env = Context<Cons<id_effect::Service<ProcessRuntimeKey, TokioProcessRuntime>, Nil>>;

#[cfg(unix)]
#[tokio::test]
async fn spawn_true_exits_zero() {
  let stack = layer_process_runtime(TokioProcessRuntime);
  let svc = stack.build().unwrap();
  let env = Context::new(Cons(svc, Nil));
  let st = run_async(spawn_wait::<Env, _>(CommandSpec::new("true")), env)
    .await
    .expect("spawn");
  assert!(st.success());
}
