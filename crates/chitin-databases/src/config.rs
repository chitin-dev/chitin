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
  /// Maximum time allowed for one request attempt, including response-body
  /// download time.
  ///
  /// The default is 10 minutes so large RCSB structure artifacts can finish
  /// downloading instead of being restarted by the retry policy.
  pub request_timeout: Duration,
  /// Maximum buffered response body size in bytes.
  ///
  /// The default is 512 MiB so large RCSB structure artifacts can be
  /// downloaded while retaining protection against unbounded responses.
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
      request_timeout: Duration::from_secs(10 * 60),
      max_response_bytes: 512 * 1024 * 1024,
      max_concurrent_requests: 8,
      retry_policy: RetryPolicy::default(),
    }
  }
}
