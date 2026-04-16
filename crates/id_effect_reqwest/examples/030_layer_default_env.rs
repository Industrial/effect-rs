//! Build an [`id_effect_reqwest::Client`] with [`id_effect_reqwest::layer_reqwest_client_default`], wrap it in a
//! [`id_effect::Context`], and run an HTTP [`id_effect::Effect`].
//!
//! Run: `cargo run -p id_effect_reqwest --example 030_layer_default_env`

use id_effect::Layer;
use id_effect::context::{Cons, Context, Nil};
use id_effect_reqwest::{Error, layer_reqwest_client_default, text};
use id_effect_tokio::run_async;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::main]
async fn main() {
  let server = MockServer::start().await;
  Mock::given(method("GET"))
    .and(path("/layer"))
    .respond_with(ResponseTemplate::new(200).set_body_string("layer-ok"))
    .mount(&server)
    .await;

  let url = format!("{}/layer", server.uri());
  let layer = layer_reqwest_client_default();
  let cell = layer.build().unwrap();
  let env: Context<Cons<_, Nil>> = Context::new(Cons(cell, Nil));

  let body = run_async(text::<String, Error, _, _>(move |c| c.get(url)), env)
    .await
    .unwrap();
  assert_eq!(body, "layer-ok");
  println!("030_layer_default_env ok");
}
