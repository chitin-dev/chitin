//! Shared database error types.

use http::StatusCode;

/// Transport-layer failure.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum TransportError {
  /// Request timed out.
  #[error("request timed out")]
  Timeout,
  /// Connection failed before a response was received.
  #[error("connection failed")]
  Connection,
  /// Request URL was invalid.
  #[error("invalid URL: {url}")]
  InvalidUrl {
    /// Invalid URL string.
    url: String,
  },
  /// Response body exceeded the configured size limit.
  #[error("response exceeded size limit {limit} bytes; observed {observed:?}")]
  ResponseTooLarge {
    /// Configured maximum response size.
    limit: u64,
    /// Observed response size, when known.
    observed: Option<u64>,
  },
  /// Request could not be built by the concrete transport.
  #[error("failed to build request")]
  RequestBuild,
  /// Request was cancelled by concurrency control.
  #[error("request was cancelled")]
  Cancelled,
  /// Other transport failure.
  #[error("transport error: {message}")]
  Other {
    /// Sanitized transport error message.
    message: String,
  },
}

/// Remote provider returned an unsuccessful response.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
#[error("remote provider returned HTTP status {status}")]
pub struct RemoteError {
  /// Remote HTTP status code.
  pub status: StatusCode,
}

/// Response decoding failed.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
#[error("failed to decode {format}: {message}")]
pub struct DecodeError {
  /// Format being decoded.
  pub format: &'static str,
  /// Sanitized decode message.
  pub message: String,
}

/// Shared data-access error.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum DataError {
  /// Transport-layer failure.
  #[error(transparent)]
  Transport(#[from] TransportError),
  /// Remote provider returned an unsuccessful response.
  #[error(transparent)]
  Remote(#[from] RemoteError),
  /// Response decoding failed.
  #[error(transparent)]
  Decode(#[from] DecodeError),
  /// Request was cancelled.
  #[error("request was cancelled")]
  Cancelled,
}
