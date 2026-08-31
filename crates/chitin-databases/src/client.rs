//! Shared database client runtime.

use std::sync::Arc;

use tokio::sync::Semaphore;

use crate::{
  CancellationToken, ClientConfig, DownloadProgressCallback, HttpRequest, HttpResponse, HttpTransport, RetryPolicy,
  TransportError, providers::rcsb::RcsbClient, retry::is_retryable_status, transport::ReqwestTransport,
};

/// Top-level database client.
#[derive(Clone)]
pub struct Client {
  /// Shared runtime state used by every provider client.
  runtime: Arc<ClientRuntime>,
}

/// Shared runtime state for provider clients.
pub(crate) struct ClientRuntime {
  /// Shared client configuration.
  config: ClientConfig,
  /// Shared HTTP transport.
  transport: Arc<dyn HttpTransport>,
  /// Shared concurrency limiter.
  semaphore: Arc<Semaphore>,
}

impl Client {
  /// Creates a database client with the production HTTP transport.
  ///
  /// # Parameters
  ///
  /// * `config` controls timeouts, retry policy, response limits, and
  ///   concurrency.
  ///
  /// # Returns
  ///
  /// A database client with shared runtime state.
  ///
  /// # Errors
  ///
  /// Returns [`TransportError`] when the production HTTP client cannot be
  /// constructed.
  pub fn new(config: ClientConfig) -> Result<Self, TransportError> {
    let transport = Arc::new(ReqwestTransport::new(&config)?);
    Ok(Self::with_transport(config, transport))
  }

  /// Creates a database client with a caller-supplied transport.
  ///
  /// # Parameters
  ///
  /// * `config` controls retry policy, response limits, and concurrency.
  /// * `transport` executes transport-neutral HTTP requests.
  ///
  /// # Returns
  ///
  /// A database client suitable for tests or alternate transports.
  pub fn with_transport(config: ClientConfig, transport: Arc<dyn HttpTransport>) -> Self {
    let max_concurrent_requests = config.max_concurrent_requests.max(1);
    Self {
      runtime: Arc::new(ClientRuntime {
        config,
        transport,
        semaphore: Arc::new(Semaphore::new(max_concurrent_requests)),
      }),
    }
  }

  /// Creates an RCSB PDB provider client.
  pub fn rcsb(&self) -> RcsbClient {
    RcsbClient::new(self.runtime.clone())
  }
}

impl ClientRuntime {
  /// Executes a request with shared concurrency, retry, and response limits.
  ///
  /// # Parameters
  ///
  /// * `request` is the request to execute.
  ///
  /// # Returns
  ///
  /// A successful buffered HTTP response.
  ///
  /// # Errors
  ///
  /// Returns [`TransportError`] for transport failures, cancellation, or
  /// response-size-limit failures.
  pub(crate) async fn execute(&self, request: HttpRequest) -> Result<HttpResponse, TransportError> {
    self.execute_with_progress(request, None).await
  }

  /// Executes a request while forwarding response-body progress.
  ///
  /// # Parameters
  ///
  /// * `request` is the transport-neutral request to execute.
  /// * `progress` receives cumulative response-body bytes and the optional
  ///   response length.
  ///
  /// # Returns
  ///
  /// A buffered response after concurrency limits, retries, and response-size
  /// checks have been applied.
  ///
  /// # Errors
  ///
  /// Returns [`TransportError`] when the request is cancelled, transport
  /// execution fails, retries are exhausted, or the response is too large.
  pub(crate) async fn execute_with_progress(
    &self,
    request: HttpRequest,
    progress: Option<DownloadProgressCallback>,
  ) -> Result<HttpResponse, TransportError> {
    let retry_policy = self.config.retry_policy;
    let cancellation = CancellationToken::new();
    let mut attempt = 1;

    loop {
      let _permit = self
        .semaphore
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| TransportError::Cancelled)?;
      let response = self
        .transport
        .execute_with_progress(request.clone(), progress.clone())
        .await;
      drop(_permit);
      match response {
        Ok(response) if self.response_too_large(&response) => {
          return Err(TransportError::ResponseTooLarge {
            limit: self.config.max_response_bytes,
            observed: Some(response.body.len() as u64),
          });
        }
        Ok(response)
          if request.idempotent
            && is_retryable_status(response.status)
            && retry_policy.should_retry_after_attempt(attempt) =>
        {
          self
            .sleep_before_retry(retry_policy, attempt, Some(&response), &cancellation)
            .await;
          attempt = attempt.saturating_add(1);
        }
        Ok(response) => return Ok(response),
        Err(error)
          if request.idempotent && error.is_retryable() && retry_policy.should_retry_after_attempt(attempt) =>
        {
          self
            .sleep_before_retry(retry_policy, attempt, None, &cancellation)
            .await;
          attempt = attempt.saturating_add(1);
        }
        Err(error) => return Err(error),
      }
    }
  }

  /// Executes a request while streaming the response into a file.
  pub(crate) async fn execute_to_file(
    &self,
    request: HttpRequest,
    destination: &std::path::Path,
    progress: Option<DownloadProgressCallback>,
    cancellation: CancellationToken,
  ) -> Result<HttpResponse, TransportError> {
    let retry_policy = self.config.retry_policy;
    let mut attempt = 1;

    loop {
      if cancellation.is_cancelled() {
        return Err(TransportError::Cancelled);
      }
      let permit = tokio::select! {
        permit = self.semaphore.clone().acquire_owned() => permit.map_err(|_| TransportError::Cancelled)?,
        _ = cancellation.cancelled() => return Err(TransportError::Cancelled),
      };
      let response = self
        .transport
        .execute_to_file(request.clone(), destination, progress.clone(), cancellation.clone())
        .await;
      drop(permit);
      match response {
        Ok(response)
          if request.idempotent
            && is_retryable_status(response.status)
            && retry_policy.should_retry_after_attempt(attempt) =>
        {
          self
            .sleep_before_retry(retry_policy, attempt, Some(&response), &cancellation)
            .await;
          attempt = attempt.saturating_add(1);
        }
        Ok(response) => return Ok(response),
        Err(error)
          if request.idempotent && error.is_retryable() && retry_policy.should_retry_after_attempt(attempt) =>
        {
          self
            .sleep_before_retry(retry_policy, attempt, None, &cancellation)
            .await;
          attempt = attempt.saturating_add(1);
        }
        Err(error) => return Err(error),
      }
    }
  }

  /// Checks whether a response exceeds the configured body limit.
  ///
  /// # Parameters
  ///
  /// * `response` is the buffered response.
  ///
  /// # Returns
  ///
  /// `true` when the body is larger than configured.
  fn response_too_large(&self, response: &HttpResponse) -> bool {
    response.body.len() as u64 > self.config.max_response_bytes
  }

  /// Sleeps before a retry attempt.
  ///
  /// # Parameters
  ///
  /// * `retry_policy` controls backoff timing.
  /// * `attempt` is the one-based failed attempt.
  /// * `response` is the optional response that triggered the retry.
  async fn sleep_before_retry(
    &self,
    retry_policy: RetryPolicy,
    attempt: u32,
    response: Option<&HttpResponse>,
    cancellation: &CancellationToken,
  ) {
    let delay = retry_policy.backoff_after_attempt(attempt, response.map(|response| &response.headers));
    if !delay.is_zero() {
      tokio::select! {
        _ = tokio::time::sleep(delay) => {}
        _ = cancellation.cancelled() => {}
      }
    }
  }
}

impl TransportError {
  /// Returns whether a transport error appears transient.
  fn is_retryable(&self) -> bool {
    matches!(self, Self::Timeout | Self::Connection | Self::Other { .. })
  }
}
