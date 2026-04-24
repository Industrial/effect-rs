//! Portable HTTP client ([`HttpClient`]) and reqwest-backed [`ReqwestHttpClient`].

use std::time::Duration;

use bytes::Bytes;
use id_effect::kernel::Effect;
use reqwest::header::{HeaderName, HeaderValue};

use crate::error::HttpError;

id_effect::service_key!(
  /// Tag for the default [`HttpClient`] implementation in `R`.
  pub struct HttpClientKey
);

/// HTTP verb for [`HttpRequest`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HttpMethod {
  /// GET
  Get,
  /// POST
  Post,
  /// PUT
  Put,
  /// DELETE
  Delete,
  /// PATCH
  Patch,
}

/// Minimal portable HTTP request description.
#[derive(Clone, Debug)]
pub struct HttpRequest {
  /// HTTP method.
  pub method: HttpMethod,
  /// Full URL string.
  pub url: String,
  /// `(name, value)` header pairs.
  pub headers: Vec<(String, String)>,
  /// Optional request body.
  pub body: Option<Vec<u8>>,
  /// Per-request timeout (applied on top of client defaults).
  pub timeout: Option<Duration>,
  /// Maximum response body bytes to buffer in memory (default [`HttpRequest::DEFAULT_MAX_BODY_BYTES`]).
  pub max_body_bytes: Option<usize>,
}

impl HttpRequest {
  /// Default cap for buffered response bodies (10 MiB).
  pub const DEFAULT_MAX_BODY_BYTES: usize = 10 * 1024 * 1024;

  /// GET without body.
  #[inline]
  pub fn get(url: impl Into<String>) -> Self {
    Self {
      method: HttpMethod::Get,
      url: url.into(),
      headers: Vec::new(),
      body: None,
      timeout: None,
      max_body_bytes: None,
    }
  }

  /// POST with body bytes.
  #[inline]
  pub fn post(url: impl Into<String>, body: Vec<u8>) -> Self {
    Self {
      method: HttpMethod::Post,
      url: url.into(),
      headers: Vec::new(),
      body: Some(body),
      timeout: None,
      max_body_bytes: None,
    }
  }
}

/// Buffered HTTP response (MVP — full body in memory).
#[derive(Clone, Debug)]
pub struct HttpResponse {
  /// Status code.
  pub status: u16,
  /// `(name, value)` headers (lowercased names not guaranteed).
  pub headers: Vec<(String, String)>,
  /// Response body bytes.
  pub body: Vec<u8>,
}

/// Map a buffered [`HttpResponse`] body to a single [`id_effect::Chunk`] (MVP for stream interop).
#[inline]
pub fn response_body_chunk(resp: &HttpResponse) -> id_effect::Chunk<u8> {
  id_effect::Chunk::from_vec(resp.body.clone())
}

/// Capability: execute portable HTTP requests as [`Effect`] values.
pub trait HttpClient: Send + Sync + 'static {
  /// Execute `req` and return a buffered response.
  fn execute(&self, req: HttpRequest) -> Effect<HttpResponse, HttpError, ()>;
}

/// [`reqwest::Client`]-backed [`HttpClient`].
#[derive(Clone)]
pub struct ReqwestHttpClient {
  inner: reqwest::Client,
}

impl ReqwestHttpClient {
  /// Wrap an existing [`reqwest::Client`].
  #[inline]
  pub fn new(client: reqwest::Client) -> Self {
    Self { inner: client }
  }

  /// Build with [`reqwest::Client::new`].
  #[inline]
  pub fn default_client() -> Self {
    Self::new(reqwest::Client::new())
  }
}

impl HttpClient for ReqwestHttpClient {
  fn execute(&self, req: HttpRequest) -> Effect<HttpResponse, HttpError, ()> {
    let client = self.inner.clone();
    Effect::new_async(move |_r: &mut ()| {
      let req = req.clone();
      Box::pin(async move {
        let max = req
          .max_body_bytes
          .unwrap_or(HttpRequest::DEFAULT_MAX_BODY_BYTES);
        let method = match req.method {
          HttpMethod::Get => reqwest::Method::GET,
          HttpMethod::Post => reqwest::Method::POST,
          HttpMethod::Put => reqwest::Method::PUT,
          HttpMethod::Delete => reqwest::Method::DELETE,
          HttpMethod::Patch => reqwest::Method::PATCH,
        };
        let mut rb = client.request(method, &req.url);
        if let Some(t) = req.timeout {
          rb = rb.timeout(t);
        }
        for (k, v) in &req.headers {
          let name = HeaderName::from_bytes(k.as_bytes())
            .map_err(|e| HttpError::InvalidRequest(format!("bad header name {k:?}: {e}")))?;
          let value = HeaderValue::from_str(v)
            .map_err(|e| HttpError::InvalidRequest(format!("bad header value for {k:?}: {e}")))?;
          rb = rb.header(name, value);
        }
        if let Some(b) = &req.body {
          rb = rb.body(b.clone());
        }
        let resp = rb.send().await.map_err(HttpError::from)?;
        let status = resp.status().as_u16();
        let mut headers = Vec::new();
        for (k, v) in resp.headers().iter() {
          if let Ok(s) = v.to_str() {
            headers.push((k.as_str().to_string(), s.to_string()));
          }
        }
        let bytes: Bytes = resp.bytes().await.map_err(HttpError::from)?;
        let len = bytes.len();
        if len > max {
          return Err(HttpError::BodyTooLarge { len, max });
        }
        Ok(HttpResponse {
          status,
          headers,
          body: bytes.to_vec(),
        })
      })
    })
  }
}

/// [`id_effect::Service`] cell for [`HttpClientKey`].
pub type HttpClientService<H> = id_effect::Service<HttpClientKey, H>;

/// [`id_effect::layer_service`] for any cloneable [`HttpClient`] implementation.
#[inline]
pub fn layer_http_client<H>(
  client: H,
) -> id_effect::layer::LayerFn<impl Fn() -> Result<HttpClientService<H>, std::convert::Infallible>>
where
  H: Clone + HttpClient + 'static,
{
  id_effect::layer_service::<HttpClientKey, _>(client)
}

/// Same as [`layer_http_client`] with [`ReqwestHttpClient::default_client`].
#[inline]
pub fn layer_reqwest_http_client_default() -> id_effect::layer::LayerFn<
  impl Fn() -> Result<HttpClientService<ReqwestHttpClient>, std::convert::Infallible>,
> {
  layer_http_client(ReqwestHttpClient::default_client())
}

/// [`layer_http_client`] with an explicit [`reqwest::Client`].
#[inline]
pub fn layer_reqwest_http_client(
  client: reqwest::Client,
) -> id_effect::layer::LayerFn<
  impl Fn() -> Result<HttpClientService<ReqwestHttpClient>, std::convert::Infallible>,
> {
  layer_http_client(ReqwestHttpClient::new(client))
}

/// Supertrait: `R` provides an [`HttpClient`] at [`HttpClientKey`].
pub trait NeedsHttpClient<H>: id_effect::Get<HttpClientKey, id_effect::Here, Target = H> {}
impl<R, H> NeedsHttpClient<H> for R where
  R: id_effect::Get<HttpClientKey, id_effect::Here, Target = H>
{
}

/// Execute using the [`HttpClient`] installed at [`HttpClientKey`].
#[inline]
pub fn execute<R, H>(req: HttpRequest) -> Effect<HttpResponse, HttpError, R>
where
  R: NeedsHttpClient<H> + 'static,
  H: HttpClient + Clone + 'static,
{
  Effect::new_async(move |r: &mut R| {
    let client = id_effect::Get::<HttpClientKey>::get(r).clone();
    let inner = client.execute(req);
    Box::pin(async move { inner.run(&mut ()).await })
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  mod http_request {
    use super::*;

    mod get {
      use super::*;

      #[test]
      fn sets_get_method_and_empty_optional_fields() {
        let r = HttpRequest::get("https://a.test/");
        assert_eq!(r.method, HttpMethod::Get);
        assert_eq!(r.url, "https://a.test/");
        assert!(r.headers.is_empty());
        assert!(r.body.is_none());
        assert!(r.timeout.is_none());
        assert!(r.max_body_bytes.is_none());
      }
    }

    mod post {
      use super::*;

      #[test]
      fn sets_post_method_and_body() {
        let body = vec![1, 2, 3];
        let r = HttpRequest::post("https://a.test/p", body.clone());
        assert_eq!(r.method, HttpMethod::Post);
        assert_eq!(r.body, Some(body));
      }
    }

    #[test]
    fn default_max_body_bytes_constant_is_ten_mib() {
      assert_eq!(HttpRequest::DEFAULT_MAX_BODY_BYTES, 10 * 1024 * 1024);
    }
  }

  mod response_body_chunk {
    use super::*;

    #[test]
    fn yields_empty_chunk_when_body_empty() {
      let resp = HttpResponse {
        status: 204,
        headers: vec![],
        body: vec![],
      };
      let c = response_body_chunk(&resp);
      assert!(c.is_empty());
    }

    #[test]
    fn yields_non_empty_chunk_matching_body() {
      let resp = HttpResponse {
        status: 200,
        headers: vec![],
        body: vec![7, 8],
      };
      let c = response_body_chunk(&resp);
      assert_eq!(c.len(), 2);
    }
  }
}
