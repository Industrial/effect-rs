//! [`reqwest`](https://docs.rs/reqwest) integration for the workspace `effect` crate.
//!
//! ## Model
//!
//! - Treat [`reqwest::Client`] as a **service** keyed by [`ReqwestClientKey`] (Effect.ts-style tag).
//! - Build it at the composition root with [`layer_reqwest_client`] or [`layer_reqwest_client_with`].
//! - Express HTTP calls as [`Effect`] values using [`send`], [`text`], [`bytes`](crate::bytes), or [`json`].
//! - Optional: [`layer_reqwest_pool`] + [`send_pooled`] to keep a [`Pool`] of [`PooledClient`] with TTL.
//! - Optional: [`json_schema`] decodes JSON bodies via [`Schema::decode_unknown`](id_effect::schema::Schema::decode_unknown)
//!   so [`id_effect::schema::ParseError`] carries field paths.
//!
//! ## Relation to `id_effect_tokio`
//!
//! This crate depends only on **`effect`** (and `reqwest`). Async HTTP steps are ordinary
//! [`Effect::new_async`](id_effect::Effect::new_async) bodies; drive them on Tokio with
//! [`id_effect::run_async`] or the same symbol re-exported from **`id_effect_tokio`** as
//! `id_effect_tokio::run_async` (recommended in Tokio services so sleep/yield stay consistent).
//!
//! ## Example
//!
//! ```ignore
//! use id_effect::{ctx, run_async, service, Context, Cons, Nil};
//! use id_effect_reqwest::{ReqwestClientKey, send};
//!
//! type Env = Context<Cons<id_effect::Service<ReqwestClientKey, reqwest::Client>, Nil>>;
//!
//! # async fn demo() -> Result<(), reqwest::Error> {
//! let env = Context::new(Cons(
//!   service::<ReqwestClientKey, _>(reqwest::Client::new()),
//!   Nil,
//! ));
//! let eff = send(|c| c.get("https://example.com"));
//! let _res = run_async(eff, env).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Runnable examples
//!
//! See `examples/` (e.g. `cargo run -p id_effect_reqwest --example 010_wiremock_get_text`).

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::sync::Arc;
use std::time::Duration;

use ::id_effect::{
  Here, Never, Pool, Schema, Scope, context::Get, effect, fail, from_async, kernel::Effect,
  layer_service, service, succeed,
};
use id_effect::data::EffectData;
use id_effect::schema::{ParseError, Unknown};
use serde_json::Value;

pub use reqwest::{Client, ClientBuilder, Error, RequestBuilder, Response, StatusCode};

id_effect::service_key!(
  /// Tag for the default [`reqwest::Client`] service in the environment `R`.
  pub struct ReqwestClientKey
);

/// [`id_effect::Service`] cell holding a [`reqwest::Client`].
pub type ReqwestClientService = id_effect::Service<ReqwestClientKey, reqwest::Client>;

/// [`id_effect::Layer`](id_effect::layer::Layer) that provides a cloneable [`reqwest::Client`].
#[inline]
pub fn layer_reqwest_client(
  client: reqwest::Client,
) -> id_effect::layer::LayerFn<impl Fn() -> Result<ReqwestClientService, std::convert::Infallible>>
{
  layer_service::<ReqwestClientKey, _>(client)
}

/// Same as [`layer_reqwest_client`], using [`reqwest::Client::new`].
#[inline]
pub fn layer_reqwest_client_default()
-> id_effect::layer::LayerFn<impl Fn() -> Result<ReqwestClientService, std::convert::Infallible>> {
  layer_reqwest_client(reqwest::Client::new())
}

/// Build a client from [`reqwest::ClientBuilder`] and expose it as a layer.
#[inline]
pub fn layer_reqwest_client_with(
  builder: reqwest::ClientBuilder,
) -> Result<
  id_effect::layer::LayerFn<impl Fn() -> Result<ReqwestClientService, std::convert::Infallible>>,
  reqwest::Error,
> {
  let client = builder.build()?;
  Ok(layer_reqwest_client(client))
}

/// Wraps [`Client`] in an [`Arc`] so it can live in [`Pool`] ([`PartialEq`] uses pointer identity).
#[derive(Clone, Debug)]
pub struct PooledClient(Arc<Client>);

impl PooledClient {
  /// Allocation identity of the inner [`Client`] (for tests / pooling assertions).
  #[inline]
  pub fn allocation_ptr(&self) -> *const Client {
    Arc::as_ptr(&self.0)
  }
}

impl std::ops::Deref for PooledClient {
  type Target = Client;

  fn deref(&self) -> &Self::Target {
    &self.0
  }
}

impl PartialEq for PooledClient {
  fn eq(&self, other: &Self) -> bool {
    Arc::ptr_eq(&self.0, &other.0)
  }
}

impl Eq for PooledClient {}

id_effect::service_key!(
  /// Tag for [`Pool`]`<`[`PooledClient`]`, `[`Never`]`>` in `R`.
  pub struct ReqwestPoolKey
);

/// [`id_effect::Service`] cell holding a [`Pool`] of [`PooledClient`].
pub type ReqwestPoolService = id_effect::Service<ReqwestPoolKey, Pool<PooledClient, Never>>;

/// Supertrait for environments that expose a [`reqwest::Client`] at [`ReqwestClientKey`].
pub trait NeedsReqwestClient: Get<ReqwestClientKey, Here, Target = Client> {}
impl<R: Get<ReqwestClientKey, Here, Target = Client>> NeedsReqwestClient for R {}

/// Supertrait for environments that expose a [`Pool`] of [`PooledClient`] at [`ReqwestPoolKey`].
pub trait NeedsReqwestPool: Get<ReqwestPoolKey, Here, Target = Pool<PooledClient, Never>> {}
impl<R: Get<ReqwestPoolKey, Here, Target = Pool<PooledClient, Never>>> NeedsReqwestPool for R {}

/// Layer that installs a [`Pool::make_with_ttl`] of [`PooledClient`] (factory: fresh [`Client::new`] per slot).
///
/// The pool is materialized on first [`::id_effect::layer::Layer::build`] via [`::id_effect::run_blocking`]
/// inside [`::id_effect::layer::LayerEffect`], not in this crate’s library surface.
#[inline]
pub fn layer_reqwest_pool(
  capacity: usize,
  ttl: Duration,
) -> ::id_effect::layer::LayerEffect<ReqwestPoolService, Never, ()> {
  ::id_effect::layer::effect(
    Pool::make_with_ttl(capacity, ttl, || {
      succeed::<PooledClient, Never, ()>(PooledClient(Arc::new(Client::new())))
    })
    .map(service::<ReqwestPoolKey, _>),
  )
}

/// [`send`] with a client checked out from [`ReqwestPoolKey`]; returns to the pool when the inner scope closes.
#[inline]
pub fn send_pooled<A, E, R, F>(build: F) -> Effect<A, E, R>
where
  A: From<Response> + 'static,
  E: From<Error> + 'static,
  R: NeedsReqwestPool + 'static,
  F: FnOnce(&Client) -> RequestBuilder + Send + 'static,
{
  effect!(|r: &mut R| {
    let pool = Get::<ReqwestPoolKey>::get(r).clone();
    let (pooled, scope) = ~from_async(move |_r| async move {
      let mut scope = Scope::make();
      let pooled = pool
        .get()
        .run(&mut scope)
        .await
        .expect("pool factory is infallible");
      Ok::<(PooledClient, Scope), E>((pooled, scope))
    });
    let resp = ~from_async(move |_r| async move {
      build(&pooled).send().await.map_err(E::from)
    });
    scope.close();
    A::from(resp)
  })
}

/// Failure from [`json_schema`].
#[derive(Debug)]
pub enum JsonSchemaError {
  /// HTTP transport or status failure from the underlying [`reqwest`] client.
  Http(Error),
  /// Invalid JSON (syntax); see message.
  Json(String),
  /// Response body did not match the expected [`Schema`].
  Schema(ParseError),
}

fn unknown_from_json_value(value: Value) -> Unknown {
  id_effect::schema::serde_bridge::unknown_from_serde_json(value)
}

fn decode_response_schema<A, I, Es>(
  schema: &Schema<A, I, Es>,
  bytes: &[u8],
) -> Result<A, JsonSchemaError>
where
  Es: EffectData + 'static,
  A: 'static,
  I: 'static,
{
  let v: Value = serde_json::from_slice(bytes).map_err(|e| JsonSchemaError::Json(e.to_string()))?;
  let u = unknown_from_json_value(v);
  schema.decode_unknown(&u).map_err(JsonSchemaError::Schema)
}

/// [`send`] then decode the response body as JSON through `schema` ([`Schema::decode_unknown`]).
#[inline]
pub fn json_schema<R, F, A, I, Es>(
  schema: Arc<Schema<A, I, Es>>,
  build: F,
) -> Effect<A, JsonSchemaError, R>
where
  R: NeedsReqwestClient + 'static,
  F: FnOnce(&Client) -> RequestBuilder + Send + 'static,
  Es: EffectData + 'static,
  A: 'static,
  I: 'static,
{
  effect!(|r: &mut R| {
    let client = Get::<ReqwestClientKey>::get(r).clone();
    let schema_arc = Arc::clone(&schema);
    let resp = ~from_async(move |_r| async move {
      build(&client).send().await.map_err(JsonSchemaError::Http)
    });
    let buf = ~from_async(move |_r| async move {
      resp.bytes().await.map_err(JsonSchemaError::Http)
    });
    match decode_response_schema(&schema_arc, &buf) {
      Ok(v) => v,
      Err(e) => ~fail::<A, JsonSchemaError, R>(e),
    }
  })
}

/// Run [`RequestBuilder::send`](reqwest::RequestBuilder::send) using the client from `R`.
#[inline]
pub fn send<A, E, R, F>(build: F) -> Effect<A, E, R>
where
  A: From<Response> + 'static,
  E: From<Error> + 'static,
  R: NeedsReqwestClient + 'static,
  F: FnOnce(&Client) -> RequestBuilder + Send + 'static,
{
  effect!(|r: &mut R| {
    let client = Get::<ReqwestClientKey>::get(r).clone();
    ~from_async(move |_r| async move {
      build(&client)
        .send()
        .await
        .map_err(E::from)
        .map(A::from)
    })
  })
}

/// [`send`] then [`Response::text`](reqwest::Response::text).
#[inline]
pub fn text<A, E, R, F>(build: F) -> Effect<A, E, R>
where
  A: From<String> + 'static,
  E: From<Error> + 'static,
  R: NeedsReqwestClient + 'static,
  F: FnOnce(&Client) -> RequestBuilder + Send + 'static,
{
  effect!(|r: &mut R| {
    let client = Get::<ReqwestClientKey>::get(r).clone();
    let resp = ~from_async(move |_r| async move {
      build(&client).send().await.map_err(E::from)
    });
    let body = ~from_async(move |_r| async move {
      resp.text().await.map_err(E::from)
    });
    A::from(body)
  })
}

/// [`send`] then [`Response::bytes`](reqwest::Response::bytes).
#[inline]
pub fn bytes<A, E, R, F>(build: F) -> Effect<A, E, R>
where
  A: From<bytes::Bytes> + 'static,
  E: From<Error> + 'static,
  R: NeedsReqwestClient + 'static,
  F: FnOnce(&Client) -> RequestBuilder + Send + 'static,
{
  effect!(|r: &mut R| {
    let client = Get::<ReqwestClientKey>::get(r).clone();
    let resp = ~from_async(move |_r| async move {
      build(&client).send().await.map_err(E::from)
    });
    let body = ~from_async(move |_r| async move {
      resp.bytes().await.map_err(E::from)
    });
    A::from(body)
  })
}

/// [`send`] then [`Response::json`](reqwest::Response::json).
#[inline]
pub fn json<A, E, R, F, T>(build: F) -> Effect<A, E, R>
where
  A: From<T> + 'static,
  E: From<Error> + 'static,
  R: NeedsReqwestClient + 'static,
  F: FnOnce(&Client) -> RequestBuilder + Send + 'static,
  T: serde::de::DeserializeOwned + 'static,
{
  effect!(|r: &mut R| {
    let client = Get::<ReqwestClientKey>::get(r).clone();
    let resp = ~from_async(move |_r| async move {
      build(&client).send().await.map_err(E::from)
    });
    let value = ~from_async(move |_r| async move {
      resp.json::<T>().await.map_err(E::from)
    });
    A::from(value)
  })
}

/// Shorthand for [`send`]`(|c| c.get(url))`.
#[inline]
pub fn get<A, E, R>(url: String) -> Effect<A, E, R>
where
  A: From<Response> + 'static,
  E: From<Error> + 'static,
  R: NeedsReqwestClient + 'static,
{
  effect!(|_r: &mut R| {
    let x = ~send(move |c| c.get(url));
    x
  })
}

/// Shorthand for [`send`]`(|c| c.post(url))`.
#[inline]
pub fn post<A, E, R>(url: String) -> Effect<A, E, R>
where
  A: From<Response> + 'static,
  E: From<Error> + 'static,
  R: NeedsReqwestClient + 'static,
{
  effect!(|_r: &mut R| {
    let x = ~send(move |c| c.post(url));
    x
  })
}

/// Shorthand for [`send`]`(|c| c.put(url))`.
#[inline]
pub fn put<A, E, R>(url: String) -> Effect<A, E, R>
where
  A: From<Response> + 'static,
  E: From<Error> + 'static,
  R: NeedsReqwestClient + 'static,
{
  effect!(|_r: &mut R| {
    let x = ~send(move |c| c.put(url));
    x
  })
}

/// Shorthand for [`send`]`(|c| c.delete(url))`.
#[inline]
pub fn delete<A, E, R>(url: String) -> Effect<A, E, R>
where
  A: From<Response> + 'static,
  E: From<Error> + 'static,
  R: NeedsReqwestClient + 'static,
{
  effect!(|_r: &mut R| {
    let x = ~send(move |c| c.delete(url));
    x
  })
}

/// Shorthand for [`send`]`(|c| c.head(url))`.
#[inline]
pub fn head<A, E, R>(url: String) -> Effect<A, E, R>
where
  A: From<Response> + 'static,
  E: From<Error> + 'static,
  R: NeedsReqwestClient + 'static,
{
  effect!(|_r: &mut R| {
    let x = ~send(move |c| c.head(url));
    x
  })
}

/// Shorthand for [`send`]`(|c| c.patch(url))`.
#[inline]
pub fn patch<A, E, R>(url: String) -> Effect<A, E, R>
where
  A: From<Response> + 'static,
  E: From<Error> + 'static,
  R: NeedsReqwestClient + 'static,
{
  effect!(|_r: &mut R| {
    let x = ~send(move |c| c.patch(url));
    x
  })
}

#[cfg(test)]
mod tests {
  use super::*;
  use id_effect::schema;
  use id_effect::{Layer, Scope, run_async, run_blocking, service_env, succeed};
  use serde::{Deserialize, Serialize};
  use std::sync::atomic::{AtomicUsize, Ordering};
  use wiremock::matchers::{method, path};
  use wiremock::{Mock, MockServer, ResponseTemplate};

  #[tokio::test]
  async fn text_roundtrip() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
      .and(path("/ping"))
      .respond_with(ResponseTemplate::new(200).set_body_string("pong"))
      .mount(&server)
      .await;

    let url = format!("{}/ping", server.uri());
    let env = service_env::<ReqwestClientKey, _>(Client::new());
    let body = run_async(text::<String, Error, _, _>(move |c| c.get(url)), env)
      .await
      .unwrap();
    assert_eq!(body, "pong");
  }

  #[tokio::test]
  async fn json_roundtrip() {
    #[derive(Debug, Deserialize, Serialize, PartialEq)]
    struct Msg {
      n: i32,
    }

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
  }

  #[tokio::test]
  async fn layer_builds_service_cell() {
    let layer = layer_reqwest_client_default();
    let cell = layer.build().unwrap();
    assert!(cell.value.get("https://example.com").build().is_ok());
  }

  #[tokio::test]
  async fn reqwest_pool_reuses_connections() {
    let factory_calls = Arc::new(AtomicUsize::new(0));
    let fc = factory_calls.clone();
    let pool = run_blocking(
      Pool::make_with_ttl(1, Duration::from_secs(120), move || {
        fc.fetch_add(1, Ordering::SeqCst);
        succeed::<PooledClient, Never, ()>(PooledClient(Arc::new(Client::new())))
      }),
      (),
    )
    .expect("pool");

    let s1 = Scope::make();
    let c1 = run_async(pool.clone().get(), s1.clone())
      .await
      .expect("get1");
    let p1 = c1.allocation_ptr();
    s1.close();

    let s2 = Scope::make();
    let c2 = run_async(pool.get(), s2.clone()).await.expect("get2");
    assert_eq!(p1, c2.allocation_ptr());
    s2.close();

    assert_eq!(factory_calls.load(Ordering::SeqCst), 1);
  }

  #[tokio::test]
  async fn reqwest_response_schema_decode_error_has_field_path() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
      .and(path("/bad"))
      .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"name":"x","age":"oops"}"#))
      .mount(&server)
      .await;

    let url = format!("{}/bad", server.uri());
    let sch = Arc::new(schema::struct_(
      "name",
      schema::string::<()>(),
      "age",
      schema::i64::<()>(),
    ));
    let env = service_env::<ReqwestClientKey, _>(Client::new());
    let err = run_async(json_schema(sch, move |c| c.get(url)), env)
      .await
      .expect_err("schema");
    match err {
      JsonSchemaError::Schema(p) => {
        assert!(p.path.contains("age"), "path={:?}", p.path);
      }
      e => panic!("unexpected {e:?}"),
    }
  }

  // ── Additional HTTP method tests ──────────────────────────────────────────

  #[tokio::test]
  async fn bytes_roundtrip() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
      .and(path("/data"))
      .respond_with(ResponseTemplate::new(200).set_body_bytes(b"hello"))
      .mount(&server)
      .await;

    let url = format!("{}/data", server.uri());
    let env = service_env::<ReqwestClientKey, _>(Client::new());
    let body = run_async(bytes::<bytes::Bytes, Error, _, _>(move |c| c.get(url)), env)
      .await
      .unwrap();
    assert_eq!(body.as_ref(), b"hello");
  }

  #[tokio::test]
  async fn get_helper_fetches_text() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
      .and(path("/hello"))
      .respond_with(ResponseTemplate::new(200).set_body_string("world"))
      .mount(&server)
      .await;

    let url = format!("{}/hello", server.uri());
    let env = service_env::<ReqwestClientKey, _>(Client::new());
    let body = run_async(
      get::<Response, Error, _>(url).flat_map(|resp: Response| {
        id_effect::from_async(move |_r: &mut _| async move { resp.text().await })
      }),
      env,
    )
    .await
    .unwrap();
    assert_eq!(body, "world");
  }

  #[tokio::test]
  async fn post_helper_sends_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
      .and(path("/echo"))
      .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
      .mount(&server)
      .await;

    let url = format!("{}/echo", server.uri());
    let env = service_env::<ReqwestClientKey, _>(Client::new());
    let body = run_async(
      post::<Response, Error, _>(url).flat_map(|resp: Response| {
        id_effect::from_async(move |_r: &mut _| async move { resp.text().await })
      }),
      env,
    )
    .await
    .unwrap();
    assert_eq!(body, "ok");
  }

  #[tokio::test]
  async fn put_helper_sends_request() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
      .and(path("/item"))
      .respond_with(ResponseTemplate::new(200).set_body_string("updated"))
      .mount(&server)
      .await;

    let url = format!("{}/item", server.uri());
    let env = service_env::<ReqwestClientKey, _>(Client::new());
    let body = run_async(
      put::<Response, Error, _>(url).flat_map(|resp: Response| {
        id_effect::from_async(move |_r: &mut _| async move { resp.text().await })
      }),
      env,
    )
    .await
    .unwrap();
    assert_eq!(body, "updated");
  }

  #[tokio::test]
  async fn delete_helper_sends_request() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
      .and(path("/item"))
      .respond_with(ResponseTemplate::new(204))
      .mount(&server)
      .await;

    let url = format!("{}/item", server.uri());
    let env = service_env::<ReqwestClientKey, _>(Client::new());
    let resp = run_async(delete::<Response, Error, _>(url), env)
      .await
      .unwrap();
    assert_eq!(resp.status().as_u16(), 204);
  }

  #[tokio::test]
  async fn patch_helper_sends_request() {
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
      .and(path("/item"))
      .respond_with(ResponseTemplate::new(200).set_body_string("patched"))
      .mount(&server)
      .await;

    let url = format!("{}/item", server.uri());
    let env = service_env::<ReqwestClientKey, _>(Client::new());
    let body = run_async(
      patch::<Response, Error, _>(url).flat_map(|resp: Response| {
        id_effect::from_async(move |_r: &mut _| async move { resp.text().await })
      }),
      env,
    )
    .await
    .unwrap();
    assert_eq!(body, "patched");
  }

  #[tokio::test]
  async fn head_helper_sends_request() {
    let server = MockServer::start().await;
    Mock::given(method("HEAD"))
      .and(path("/status"))
      .respond_with(ResponseTemplate::new(200))
      .mount(&server)
      .await;

    let url = format!("{}/status", server.uri());
    let env = service_env::<ReqwestClientKey, _>(Client::new());
    let resp = run_async(head::<Response, Error, _>(url), env)
      .await
      .unwrap();
    assert_eq!(resp.status().as_u16(), 200);
  }

  #[tokio::test]
  async fn layer_reqwest_client_with_custom_client() {
    let layer = layer_reqwest_client(Client::new());
    let cell = layer.build().unwrap();
    assert!(cell.value.get("https://example.com").build().is_ok());
  }

  #[tokio::test]
  async fn layer_reqwest_client_with_builder() {
    let builder = Client::builder().timeout(Duration::from_secs(30));
    let layer = layer_reqwest_client_with(builder).unwrap();
    let cell = layer.build().unwrap();
    assert!(cell.value.get("https://example.com").build().is_ok());
  }

  #[tokio::test]
  async fn layer_reqwest_pool_builds_pool() {
    let layer = layer_reqwest_pool(2, Duration::from_secs(60));
    let cell = layer.build().expect("pool layer build");
    let s = Scope::make();
    let client = run_async(cell.value.get(), s.clone()).await.expect("get");
    let _ = client.allocation_ptr();
    s.close();
  }

  // ── JsonSchemaError display / error traits ────────────────────────────────

  #[test]
  fn json_schema_error_http_display() {
    // Create a fake HTTP error by making a bad URL request
    let rt = tokio::runtime::Runtime::new().unwrap();
    let err = rt.block_on(async {
      Client::new()
        .get("not-a-url")
        .send()
        .await
        .map_err(JsonSchemaError::Http)
        .unwrap_err()
    });
    let _ = format!("{err:?}");
  }

  #[test]
  fn json_schema_error_json_debug() {
    let e = JsonSchemaError::Json("bad json".to_string());
    let s = format!("{e:?}");
    assert!(s.contains("bad json"), "debug: {s}");
  }

  #[test]
  fn json_schema_error_schema_debug() {
    let e = JsonSchemaError::Schema(id_effect::schema::ParseError::new("field", "invalid"));
    let _ = format!("{e:?}");
  }

  #[test]
  fn pooled_client_partial_eq_same_arc() {
    let client = Client::new();
    let arc = Arc::new(client);
    let a = PooledClient(arc.clone());
    let b = PooledClient(arc.clone());
    assert_eq!(a, b);
  }

  #[test]
  fn pooled_client_partial_eq_different_arc() {
    let a = PooledClient(Arc::new(Client::new()));
    let b = PooledClient(Arc::new(Client::new()));
    assert_ne!(a, b);
  }

  #[tokio::test]
  async fn send_pooled_fetches_response() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
      .and(path("/pooled"))
      .respond_with(ResponseTemplate::new(200).set_body_string("pooled-ok"))
      .mount(&server)
      .await;

    let url = format!("{}/pooled", server.uri());
    let pool = run_blocking(
      Pool::make_with_ttl(1, Duration::from_secs(60), || {
        succeed::<PooledClient, Never, ()>(PooledClient(Arc::new(Client::new())))
      }),
      (),
    )
    .expect("pool");
    let env = service_env::<ReqwestPoolKey, _>(pool);
    let resp = run_async(
      send_pooled::<Response, Error, _, _>(move |c| c.get(url)),
      env,
    )
    .await
    .unwrap();
    assert_eq!(resp.status().as_u16(), 200);
  }

  #[tokio::test]
  async fn json_schema_error_bad_json_returns_json_variant() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
      .and(path("/badjson"))
      .respond_with(ResponseTemplate::new(200).set_body_string("not json at all"))
      .mount(&server)
      .await;

    let url = format!("{}/badjson", server.uri());
    let sch = Arc::new(schema::i64::<()>());
    let env = service_env::<ReqwestClientKey, _>(Client::new());
    let err = run_async(json_schema(sch, move |c| c.get(url)), env)
      .await
      .expect_err("should fail");
    assert!(matches!(err, JsonSchemaError::Json(_)));
  }
}
