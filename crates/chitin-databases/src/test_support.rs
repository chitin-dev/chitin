//! Test support for database provider clients.

use std::{
  collections::VecDeque,
  sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
  },
  time::Duration,
};

use async_trait::async_trait;
use bytes::Bytes;
use http::{HeaderMap, StatusCode};

use crate::{HttpRequest, HttpResponse, HttpTransport, TransportError};

/// Mock transport with queued responses and request capture.
#[derive(Clone)]
pub struct MockTransport {
  /// Queued responses returned in call order.
  responses: Arc<Mutex<VecDeque<Result<HttpResponse, TransportError>>>>,
  /// Requests captured in call order.
  requests: Arc<Mutex<Vec<HttpRequest>>>,
  /// Number of currently active requests.
  active: Arc<AtomicUsize>,
  /// Maximum active requests observed by this transport.
  max_active: Arc<AtomicUsize>,
  /// Artificial delay before returning each response.
  delay: Duration,
}

impl MockTransport {
  /// Creates a mock transport with queued responses.
  ///
  /// # Parameters
  ///
  /// * `responses` are returned in order, one per request.
  ///
  /// # Returns
  ///
  /// A mock transport with no artificial delay.
  pub fn new(responses: Vec<Result<HttpResponse, TransportError>>) -> Self {
    Self {
      responses: Arc::new(Mutex::new(VecDeque::from(responses))),
      requests: Arc::new(Mutex::new(Vec::new())),
      active: Arc::new(AtomicUsize::new(0)),
      max_active: Arc::new(AtomicUsize::new(0)),
      delay: Duration::ZERO,
    }
  }

  /// Configures an artificial delay for every request.
  ///
  /// # Parameters
  ///
  /// * `delay` is awaited before returning each queued response.
  ///
  /// # Returns
  ///
  /// The updated mock transport.
  pub fn with_delay(mut self, delay: Duration) -> Self {
    self.delay = delay;
    self
  }

  /// Returns the number of captured requests, the request count, or zero if the
  /// capture lock is poisoned.
  pub fn request_count(&self) -> usize {
    self.requests.lock().map(|requests| requests.len()).unwrap_or_default()
  }

  /// Returns the first captured request URL.
  pub fn first_request_url(&self) -> Option<String> {
    self
      .requests
      .lock()
      .ok()
      .and_then(|requests| requests.first().map(|request| request.url.to_string()))
  }

  /// Returns the maximum number of active requests observed. Maximum concurrent
  /// calls to [`HttpTransport::execute`].
  pub fn max_active(&self) -> usize {
    self.max_active.load(Ordering::SeqCst)
  }
}

#[async_trait]
impl HttpTransport for MockTransport {
  /// Executes a queued mock response and captures the request.
  async fn execute(&self, request: HttpRequest) -> Result<HttpResponse, TransportError> {
    if let Ok(mut requests) = self.requests.lock() {
      requests.push(request);
    }

    let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
    self.max_active.fetch_max(active, Ordering::SeqCst);
    if !self.delay.is_zero() {
      tokio::time::sleep(self.delay).await;
    }
    self.active.fetch_sub(1, Ordering::SeqCst);

    match self.responses.lock() {
      Ok(mut responses) => responses
        .pop_front()
        .unwrap_or_else(|| Ok(response(StatusCode::OK, b""))),
      Err(_) => Err(TransportError::Other {
        message: "mock response lock poisoned".to_string(),
      }),
    }
  }
}

/// Creates a buffered mock HTTP response.
///
/// # Parameters
///
/// * `status` is the HTTP response status.
/// * `body` is the static response body.
///
/// # Returns
///
/// A response with empty headers.
pub fn response(status: StatusCode, body: &'static [u8]) -> HttpResponse {
  HttpResponse {
    status,
    headers: HeaderMap::new(),
    body: Bytes::from_static(body),
  }
}

/// Creates a buffered mock HTTP response with headers.
///
/// # Parameters
///
/// * `status` is the HTTP response status.
/// * `body` is the static response body.
/// * `headers` is the response header map.
///
/// # Returns
///
/// A response with the supplied headers.
pub fn response_with_headers(status: StatusCode, body: &'static [u8], headers: HeaderMap) -> HttpResponse {
  HttpResponse {
    status,
    headers,
    body: Bytes::from_static(body),
  }
}
