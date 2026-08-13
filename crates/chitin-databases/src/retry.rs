//! Retry policy for transient database access failures.

use std::time::Duration;

use http::{HeaderMap, StatusCode, header::RETRY_AFTER};

/// Explicit retry policy shared by provider clients.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetryPolicy {
  /// Maximum number of attempts including the initial request.
  pub max_attempts: u32,
  /// Backoff used after the first retryable failure.
  pub initial_backoff: Duration,
  /// Maximum retry backoff.
  pub max_backoff: Duration,
}

impl RetryPolicy {
  /// Returns whether this policy allows another attempt.
  pub(crate) fn should_retry_after_attempt(self, attempt: u32) -> bool {
    attempt < self.max_attempts
  }

  /// Computes the delay before the next attempt.
  ///
  /// # Parameters
  ///
  /// * `attempt` is the one-based attempt count that just failed.
  /// * `headers` are optional response headers used to honor `Retry-After`.
  ///
  /// # Returns
  ///
  /// A bounded retry delay.
  pub(crate) fn backoff_after_attempt(self, attempt: u32, headers: Option<&HeaderMap>) -> Duration {
    retry_after(headers).unwrap_or_else(|| {
      let exponent = attempt.saturating_sub(1).min(31);
      let multiplier = 1_u32.checked_shl(exponent).unwrap_or(u32::MAX);
      self.initial_backoff.saturating_mul(multiplier).min(self.max_backoff)
    })
  }
}

impl Default for RetryPolicy {
  /// Creates a small retry policy for transient provider failures.
  fn default() -> Self {
    Self {
      max_attempts: 3,
      initial_backoff: Duration::from_millis(100),
      max_backoff: Duration::from_secs(2),
    }
  }
}

/// Returns whether a status code represents a transient HTTP response.
pub(crate) fn is_retryable_status(status: StatusCode) -> bool {
  matches!(
    status,
    StatusCode::REQUEST_TIMEOUT
      | StatusCode::TOO_MANY_REQUESTS
      | StatusCode::BAD_GATEWAY
      | StatusCode::SERVICE_UNAVAILABLE
      | StatusCode::GATEWAY_TIMEOUT
  )
}

/// Parses a simple numeric `Retry-After` header.
///
/// # Parameters
///
/// * `headers` is the optional response header map.
///
/// # Returns
///
/// `Some(Duration)` when the header contains delay seconds.
fn retry_after(headers: Option<&HeaderMap>) -> Option<Duration> {
  headers?
    .get(RETRY_AFTER)?
    .to_str()
    .ok()?
    .parse::<u64>()
    .ok()
    .map(Duration::from_secs)
}
