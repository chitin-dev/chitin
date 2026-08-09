//! Transport-neutral HTTP request types.

use bytes::Bytes;
use http::HeaderMap;
use url::Url;

/// HTTP method used by provider requests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HttpMethod {
  /// HTTP GET request.
  Get,
  /// HTTP POST request.
  Post,
}

/// Transport-neutral database HTTP request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpRequest {
  /// HTTP method expected by the endpoint.
  pub method: HttpMethod,
  /// Fully qualified endpoint URL.
  pub url: Url,
  /// Request headers.
  pub headers: HeaderMap,
  /// Optional buffered request body.
  pub body: Option<Bytes>,
  /// Whether this request can be retried after transient failures.
  pub idempotent: bool,
}

impl HttpRequest {
  /// Creates an idempotent GET request.
  ///
  /// # Parameters
  ///
  /// * `url` is the fully qualified endpoint URL.
  ///
  /// # Returns
  ///
  /// A request with no body and empty headers.
  pub fn get(url: Url) -> Self {
    Self {
      method: HttpMethod::Get,
      url,
      headers: HeaderMap::new(),
      body: None,
      idempotent: true,
    }
  }

  /// Creates a POST request.
  ///
  /// # Parameters
  ///
  /// * `url` is the fully qualified endpoint URL.
  /// * `body` is the buffered request body.
  ///
  /// # Returns
  ///
  /// A request with empty headers.
  pub fn post(url: Url, body: impl Into<Bytes>) -> Self {
    Self {
      method: HttpMethod::Post,
      url,
      headers: HeaderMap::new(),
      body: Some(body.into()),
      idempotent: false,
    }
  }
}
