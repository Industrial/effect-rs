//! Minimal [`id_effect_tower::EffectService`]: map a request to an [`id_effect::Effect`] and run it with
//! [`id_effect_tokio::run_async`] inside Tower’s [`tower::Service::call`].
//!
//! Uses `#[tokio::main(flavor = "current_thread")]` because the response future from
//! [`EffectService`](id_effect_tower::EffectService) is not `Send` (see crate docs).
//!
//! Run: `cargo run -p id_effect_tower --example 001_effect_service`
//! Or: `moon run effect-tower:examples`

use id_effect::succeed;
use id_effect_tower::EffectService;
use tower::{Service, ServiceExt};

#[tokio::main(flavor = "current_thread")]
async fn main() {
  let mut svc = EffectService::new((), |_env: &mut (), x: u32| {
    succeed::<u32, (), _>(x.saturating_add(1))
  });

  let out = svc.ready().await.unwrap().call(41).await.unwrap();
  assert_eq!(out, 42);
  println!("001_effect_service: ok (41 + 1 = {out})");
}
