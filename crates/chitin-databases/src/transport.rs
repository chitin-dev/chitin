//! HTTP transport abstraction and reqwest-backed implementation.

use crate::{ClientConfig, HttpMethod, HttpRequest, HttpResponse, TransportError};
use async_trait::async_trait;
use bytes::{Bytes, BytesMut};
use futures_util::StreamExt;
use std::{
  path::Path,
  sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
  },
};
use tokio::{io::AsyncWriteExt, sync::Notify};

/// Receives downloaded byte counts and the optional response length.
pub type DownloadProgressCallback = Arc<dyn Fn(u64, Option<u64>) + Send + Sync>;

/// Cooperative cancellation handle for network and file-transfer operations.
#[derive(Clone, Debug, Default)]
pub struct CancellationToken {
  state: Arc<CancellationState>,
}

#[derive(Debug, Default)]
struct CancellationState {
  cancelled: AtomicBool,
  notify: Notify,
}

impl CancellationToken {
  /// Creates a token that is initially active.
  pub fn new() -> Self {
    Self::default()
  }

  /// Requests cancellation of the associated operation.
  pub fn cancel(&self) {
    self.state.cancelled.store(true, Ordering::Release);
    self.state.notify.notify_waiters();
  }

  /// Returns whether cancellation has been requested.
  pub fn is_cancelled(&self) -> bool {
    self.state.cancelled.load(Ordering::Acquire)
  }

  /// Waits until cancellation is requested.
  pub(crate) async fn cancelled(&self) {
    let notified = self.state.notify.notified();
    if self.is_cancelled() {
      return;
    }
    notified.await;
  }
}

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

  /// Executes a request and reports response-body progress when supported.
  ///
  /// # Parameters
  ///
  /// * `request` is the transport-neutral request to execute.
  /// * `progress` receives the number of bytes received and the optional
  ///   response length.
  ///
  /// # Returns
  ///
  /// A buffered transport-neutral response after the request completes.
  ///
  /// # Errors
  ///
  /// Returns [`TransportError`] when request execution or response buffering
  /// fails.
  async fn execute_with_progress(
    &self,
    request: HttpRequest,
    progress: Option<DownloadProgressCallback>,
  ) -> Result<HttpResponse, TransportError> {
    let response = self.execute(request).await?;
    if let Some(progress) = progress {
      progress(response.body.len() as u64, Some(response.body.len() as u64));
    }
    Ok(response)
  }

  /// Streams a response into a file without retaining the body in memory.
  ///
  /// Implementations should enforce the configured response-size limit while
  /// receiving chunks and should remove or truncate `destination` when the
  /// operation fails. Callers are responsible for choosing a temporary path
  /// when the final destination must remain untouched until completion.
  ///
  /// # Parameters
  ///
  /// * `request` is the transport-neutral request to execute.
  /// * `destination` receives the streamed response body.
  /// * `progress` receives cumulative bytes received and the optional total.
  /// * `cancellation` cooperatively stops the transfer when cancelled.
  ///
  /// # Returns
  ///
  /// Response status and headers with an intentionally empty body.
  ///
  /// # Errors
  ///
  /// Returns [`TransportError`] when request execution, cancellation, size
  /// validation, or file writing fails.
  async fn execute_to_file(
    &self,
    request: HttpRequest,
    destination: &Path,
    progress: Option<DownloadProgressCallback>,
    cancellation: CancellationToken,
  ) -> Result<HttpResponse, TransportError> {
    if cancellation.is_cancelled() {
      return Err(TransportError::Cancelled);
    }
    let response = self.execute_with_progress(request, progress).await?;
    if cancellation.is_cancelled() {
      return Err(TransportError::Cancelled);
    }
    tokio::fs::write(destination, &response.body)
      .await
      .map_err(|error| TransportError::FileWrite {
        path: destination.to_path_buf(),
        message: error.to_string(),
      })?;
    Ok(HttpResponse {
      body: Bytes::new(),
      ..response
    })
  }
}

/// Production HTTP transport backed by a shared reqwest client.
pub(crate) struct ReqwestTransport {
  /// Shared concrete HTTP client.
  client: reqwest::Client,
  /// Maximum response size accepted by the transport.
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
    let mut builder = reqwest::Client::builder()
      .user_agent(config.user_agent.clone())
      .connect_timeout(config.connect_timeout)
      .read_timeout(config.read_timeout);
    if let Some(timeout) = config.request_timeout {
      builder = builder.timeout(timeout);
    }
    let client = builder.build().map_err(|_| TransportError::RequestBuild)?;
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
    self.execute_request(request, None).await
  }

  /// Executes one request while streaming response chunks to the progress callback.
  async fn execute_with_progress(
    &self,
    request: HttpRequest,
    progress: Option<DownloadProgressCallback>,
  ) -> Result<HttpResponse, TransportError> {
    self.execute_request(request, progress).await
  }

  /// Streams a response directly to a destination file.
  async fn execute_to_file(
    &self,
    request: HttpRequest,
    destination: &Path,
    progress: Option<DownloadProgressCallback>,
    cancellation: CancellationToken,
  ) -> Result<HttpResponse, TransportError> {
    self
      .execute_request_to_file(request, destination, progress, cancellation)
      .await
  }
}

impl ReqwestTransport {
  /// Executes one request, optionally streaming response-body progress.
  ///
  /// # Parameters
  ///
  /// * `request` contains the method, URL, headers, and optional body.
  /// * `progress` receives cumulative byte counts while the response body is
  ///   read.
  ///
  /// # Returns
  ///
  /// A buffered response containing the status, headers, and body.
  ///
  /// # Errors
  ///
  /// Returns [`TransportError`] for request failures, response-size-limit
  /// violations, or failures while reading the response body.
  async fn execute_request(
    &self,
    request: HttpRequest,
    progress: Option<DownloadProgressCallback>,
  ) -> Result<HttpResponse, TransportError> {
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
    let response_length = response.content_length();
    log::debug!(
      "database HTTP response: method={:?}, url={}, status={}, content_length={response_length:?}, streaming={}",
      request.method,
      request.url,
      status,
      progress.is_some()
    );
    if let Some(content_length) = response_length
      && content_length > self.max_response_bytes
    {
      return Err(TransportError::ResponseTooLarge {
        limit: self.max_response_bytes,
        observed: Some(content_length),
      });
    }

    let body = if let Some(progress) = progress {
      let total = response_length;
      let mut stream = response.bytes_stream();
      let mut body = BytesMut::new();
      let mut chunk_count = 0_u64;
      while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(map_reqwest_error)?;
        chunk_count = chunk_count.saturating_add(1);
        body.extend_from_slice(&chunk);
        if body.len() as u64 > self.max_response_bytes {
          return Err(TransportError::ResponseTooLarge {
            limit: self.max_response_bytes,
            observed: Some(body.len() as u64),
          });
        }
        log::trace!(
          "database HTTP response chunk: chunks={chunk_count}, received={}, total={total:?}",
          body.len()
        );
        if status.is_success() {
          progress(body.len() as u64, total);
        }
      }
      log::debug!(
        "database HTTP response stream completed: chunks={chunk_count}, received={}, total={total:?}",
        body.len()
      );
      body.freeze()
    } else {
      response.bytes().await.map_err(map_reqwest_error)?
    };
    if body.len() as u64 > self.max_response_bytes {
      return Err(TransportError::ResponseTooLarge {
        limit: self.max_response_bytes,
        observed: Some(body.len() as u64),
      });
    }

    Ok(HttpResponse { status, headers, body })
  }

  /// Sends a request and streams its response body to a file.
  async fn execute_request_to_file(
    &self,
    request: HttpRequest,
    destination: &Path,
    progress: Option<DownloadProgressCallback>,
    cancellation: CancellationToken,
  ) -> Result<HttpResponse, TransportError> {
    if cancellation.is_cancelled() {
      return Err(TransportError::Cancelled);
    }
    let mut builder = self
      .client
      .request(to_reqwest_method(request.method), request.url.clone());
    for (name, value) in &request.headers {
      builder = builder.header(name, value);
    }
    if let Some(body) = request.body {
      builder = builder.body(body);
    }

    let response = tokio::select! {
      result = builder.send() => result.map_err(map_reqwest_error)?,
      _ = cancellation.cancelled() => return Err(TransportError::Cancelled),
    };
    let status = response.status();
    let headers = response.headers().clone();
    let total = response.content_length();
    if let Some(content_length) = total
      && content_length > self.max_response_bytes
    {
      return Err(TransportError::ResponseTooLarge {
        limit: self.max_response_bytes,
        observed: Some(content_length),
      });
    }

    let mut file = tokio::fs::File::create(destination)
      .await
      .map_err(|error| TransportError::FileWrite {
        path: destination.to_path_buf(),
        message: error.to_string(),
      })?;
    let mut stream = response.bytes_stream();
    let mut received = 0_u64;
    loop {
      let chunk = tokio::select! {
        chunk = stream.next() => chunk,
        _ = cancellation.cancelled() => return Err(TransportError::Cancelled),
      };
      let Some(chunk) = chunk else {
        break;
      };
      let chunk = chunk.map_err(map_reqwest_error)?;
      received = received.saturating_add(chunk.len() as u64);
      if received > self.max_response_bytes {
        return Err(TransportError::ResponseTooLarge {
          limit: self.max_response_bytes,
          observed: Some(received),
        });
      }
      file
        .write_all(&chunk)
        .await
        .map_err(|error| TransportError::FileWrite {
          path: destination.to_path_buf(),
          message: error.to_string(),
        })?;
      if status.is_success()
        && let Some(progress) = progress.as_ref()
      {
        progress(received, total);
      }
    }
    file.flush().await.map_err(|error| TransportError::FileWrite {
      path: destination.to_path_buf(),
      message: error.to_string(),
    })?;
    file.sync_all().await.map_err(|error| TransportError::FileWrite {
      path: destination.to_path_buf(),
      message: error.to_string(),
    })?;
    Ok(HttpResponse {
      status,
      headers,
      body: Bytes::new(),
    })
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
