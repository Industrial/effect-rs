//! GET a URL via [`id_effect_platform::http`] + Tokio (`id_effect_tokio::run_async`).

use id_effect::{Cons, Context, Layer, Nil, run_async};
use id_effect_platform::http::{
  HttpClientKey, HttpRequest, ReqwestHttpClient, execute, layer_reqwest_http_client_default,
};

type Env = Context<Cons<id_effect::Service<HttpClientKey, ReqwestHttpClient>, Nil>>;

#[tokio::main]
async fn main() -> Result<(), id_effect_platform::error::HttpError> {
  let url = std::env::args()
    .nth(1)
    .unwrap_or_else(|| "https://example.com".to_string());
  let stack = layer_reqwest_http_client_default();
  let svc = stack.build().expect("infallible layer");
  let env = Context::new(Cons(svc, Nil));
  let res = run_async(execute::<Env, _>(HttpRequest::get(url)), env).await?;
  println!("status={} bytes={}", res.status, res.body.len());
  Ok(())
}
