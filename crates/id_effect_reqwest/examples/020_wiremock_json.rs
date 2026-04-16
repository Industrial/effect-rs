//! [`id_effect_reqwest::json`] with a typed response body.
//!
//! Run: `cargo run -p id_effect_reqwest --example 020_wiremock_json`

use id_effect::service_env;
use id_effect_reqwest::{Client, Error, ReqwestClientKey, json};
use id_effect_tokio::run_async;
use serde::{Deserialize, Serialize};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[derive(Debug, Deserialize, Serialize, PartialEq)]
struct Msg {
  n: i32,
}

#[tokio::main]
async fn main() {
  let server = MockServer::start().await;
  Mock::given(method("GET"))
    .and(path("/data"))
    .respond_with(ResponseTemplate::new(200).set_body_json(&Msg { n: 7 }))
    .mount(&server)
    .await;

  let url = format!("{}/data", server.uri());
  let env = service_env::<ReqwestClientKey, _>(Client::new());
  let msg = run_async(json::<Msg, Error, _, _, Msg>(move |c| c.get(url)), env)
    .await
    .unwrap();
  assert_eq!(msg, Msg { n: 7 });
  println!("020_wiremock_json ok");
}
