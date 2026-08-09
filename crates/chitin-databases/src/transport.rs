//! HTTP transport abstraction and reqwest-backed implementation.

use crate::{ClientConfig, HttpMethod, HttpRequest, HttpResponse, TransportError};
use async_trait::async_trait;
use bytes::Bytes;

/// Mockable HTTP transport boundary used by provider clients.
#[async_trait]
pub trait HttpTransport: Send + Sync {
  /// Executes one HTTP request.
  ///
  /// # Parameters
  ///
  /// * `request` is the transport-neutral request to execute.
  ///
  /// # Returns
  ///
  /// A buffered transport-neutral response.
  ///
  /// # Errors
  ///
  /// Returns [`TransportError`] for request construction, timeout,
  /// connection, cancellation, or response-size failures.
  async fn execute(&self, request: HttpRequest) -> Result<HttpResponse, TransportError>;
}

/// Production HTTP transport backed by a shared reqwest client.
pub(crate) struct ReqwestTransport {
  /// Shared concrete HTTP client.
  client: reqwest::Client,
  /// Maximum buffered response size.
  max_response_bytes: u64,
}

impl ReqwestTransport {
  /// Builds the production transport from shared client configuration.
  ///
  /// # Parameters
  ///
  /// * `config` contains timeouts, user agent, and response limits.
  ///
  /// # Returns
  ///
  /// A reqwest-backed transport.
  ///
  /// # Errors
  ///
  /// Returns [`TransportError::RequestBuild`] if the concrete client cannot be
  /// constructed.
  pub(crate) fn new(config: &ClientConfig) -> Result<Self, TransportError> {
    let client = reqwest::Client::builder()
      .user_agent(config.user_agent.clone())
      .connect_timeout(config.connect_timeout)
      .timeout(config.request_timeout)
      .build()
      .map_err(|_| TransportError::RequestBuild)?;
    Ok(Self {
      client,
      max_response_bytes: config.max_response_bytes,
    })
  }
}

#[async_trait]
impl HttpTransport for ReqwestTransport {
  /// Executes one request through reqwest.
  async fn execute(&self, request: HttpRequest) -> Result<HttpResponse, TransportError> {
    let mut builder = self
      .client
      .request(to_reqwest_method(request.method), request.url.clone());
    for (name, value) in &request.headers {
      builder = builder.header(name, value);
    }
    if let Some(body) = request.body {
      builder = builder.body(body);
    }

    let response = builder.send().await.map_err(map_reqwest_error)?;
    let status = response.status();
    let headers = response.headers().clone();
    if let Some(content_length) = response.content_length()
      && content_length > self.max_response_bytes
    {
      return Err(TransportError::ResponseTooLarge {
        limit: self.max_response_bytes,
        observed: Some(content_length),
      });
    }

    let body = response.bytes().await.map_err(map_reqwest_error)?;
    if body.len() as u64 > self.max_response_bytes {
      return Err(TransportError::ResponseTooLarge {
        limit: self.max_response_bytes,
        observed: Some(body.len() as u64),
      });
    }

    Ok(HttpResponse { status, headers, body })
  }
}

/// Converts a Chitin HTTP method into a reqwest method.
///
/// # Parameters
///
/// * `method` is the transport-neutral method.
///
/// # Returns
///
/// The equivalent reqwest method.
fn to_reqwest_method(method: HttpMethod) -> reqwest::Method {
  match method {
    HttpMethod::Get => reqwest::Method::GET,
    HttpMethod::Post => reqwest::Method::POST,
  }
}

/// Converts reqwest failures into sanitized transport errors.
///
/// # Parameters
///
/// * `error` is the reqwest error.
///
/// # Returns
///
/// A structured transport error.
fn map_reqwest_error(error: reqwest::Error) -> TransportError {
  if error.is_timeout() {
    TransportError::Timeout
  } else if error.is_connect() {
    TransportError::Connection
  } else if error.is_request() {
    TransportError::RequestBuild
  } else {
    TransportError::Other {
      message: error.to_string(),
    }
  }
}

impl From<Vec<u8>> for HttpResponse {
  /// Creates an OK response from raw bytes for small tests and helpers.
  fn from(body: Vec<u8>) -> Self {
    Self {
      status: http::StatusCode::OK,
      headers: http::HeaderMap::new(),
      body: Bytes::from(body),
    }
  }
}
