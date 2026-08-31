//! Shared database client configuration.

use std::time::Duration;

use crate::RetryPolicy;

/// Shared configuration for all database provider clients.
#[derive(Clone, Debug)]
pub struct ClientConfig {
  /// User agent sent by the production HTTP transport.
  pub user_agent: String,
  /// Maximum time allowed to establish a connection.
  pub connect_timeout: Duration,
  /// Optional maximum time allowed for one complete request attempt.
  ///
  /// The default is `None`; streamed downloads are bounded by
  /// [`ClientConfig::read_timeout`] instead.
  pub request_timeout: Option<Duration>,
  /// Maximum idle interval between response-body reads.
  pub read_timeout: Duration,
  /// Maximum response body size in bytes, for both buffered and streamed
  /// responses.
  ///
  /// The default is 512 MiB. Streamed artifacts are checked incrementally and
  /// are never retained in memory by the production transport.
  pub max_response_bytes: u64,
  /// Maximum number of in-flight provider requests sharing this client.
  pub max_concurrent_requests: usize,
  /// Retry policy used for idempotent transient failures.
  pub retry_policy: RetryPolicy,
}

impl Default for ClientConfig {
  /// Creates conservative defaults suitable for interactive desktop use.
  fn default() -> Self {
    Self {
      user_agent: format!(
        "Chitin/{} (+https://github.com/chitin-dev/chitin)",
        env!("CARGO_PKG_VERSION")
      ),
      connect_timeout: Duration::from_secs(10),
      request_timeout: None,
      read_timeout: Duration::from_secs(30),
      max_response_bytes: 512 * 1024 * 1024,
      max_concurrent_requests: 8,
      retry_policy: RetryPolicy::default(),
    }
  }
}
