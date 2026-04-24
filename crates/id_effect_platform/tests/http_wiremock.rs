use id_effect::{Cons, Context, Layer, Nil, run_async};
use id_effect_platform::error::HttpError;
use id_effect_platform::http::{
  HttpClientKey, HttpRequest, ReqwestHttpClient, execute, layer_reqwest_http_client_default,
  response_body_chunk,
};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

type Env = Context<Cons<id_effect::Service<HttpClientKey, ReqwestHttpClient>, Nil>>;

#[tokio::test]
async fn execute_hits_mock_server() {
  let server = MockServer::start().await;
  Mock::given(method("GET"))
    .and(path("/hello"))
    .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
    .mount(&server)
    .await;

  let stack = layer_reqwest_http_client_default();
  let svc = stack.build().unwrap();
  let env = Context::new(Cons(svc, Nil));
  let url = format!("{}/hello", server.uri());
  let res = run_async(execute::<Env, _>(HttpRequest::get(url)), env)
    .await
    .expect("http ok");
  assert_eq!(res.status, 200);
  assert_eq!(res.body, b"ok");
  let chunk = response_body_chunk(&res);
  assert_eq!(chunk.len(), 2);
}

#[test]
fn http_error_display() {
  let e = HttpError::InvalidRequest("x".into());
  assert!(format!("{e}").contains("invalid"));
}
