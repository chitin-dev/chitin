//! Provenance metadata for downloaded database resources.

use std::time::SystemTime;

use http::{HeaderMap, header};
use url::Url;

/// External database provider identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProviderId {
  /// RCSB Protein Data Bank.
  Rcsb,
}

/// Provenance for a downloaded provider resource.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Provenance {
  /// Database provider that served the resource.
  pub provider: ProviderId,
  /// Provider-native resource identifier.
  pub resource_identifier: String,
  /// Time at which the request was issued.
  pub requested_at: SystemTime,
  /// Fully resolved request URL.
  pub resolved_url: Url,
  /// HTTP ETag response header, when present.
  pub etag: Option<String>,
  /// HTTP Last-Modified response header, when present.
  pub last_modified: Option<String>,
  /// HTTP Content-Length response header, when present.
  pub content_length: Option<u64>,
}

impl Provenance {
  /// Creates provenance from request and response metadata.
  ///
  /// # Parameters
  ///
  /// * `provider` identifies the database provider.
  /// * `resource_identifier` identifies the provider-native resource.
  /// * `requested_at` is the request start timestamp.
  /// * `resolved_url` is the URL used for the request.
  /// * `headers` are response headers used to populate transport metadata.
  ///
  /// # Returns
  ///
  /// A populated provenance record.
  pub fn from_headers(
    provider: ProviderId,
    resource_identifier: impl Into<String>,
    requested_at: SystemTime,
    resolved_url: Url,
    headers: &HeaderMap,
  ) -> Self {
    Self {
      provider,
      resource_identifier: resource_identifier.into(),
      requested_at,
      resolved_url,
      etag: header_to_string(headers, header::ETAG),
      last_modified: header_to_string(headers, header::LAST_MODIFIED),
      content_length: headers
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok()),
    }
  }
}

/// Reads one response header as a string.
///
/// # Parameters
///
/// * `headers` is the response header map.
/// * `name` is the header name to read.
///
/// # Returns
///
/// `Some(String)` when the header is present and valid UTF-8.
fn header_to_string(headers: &HeaderMap, name: header::HeaderName) -> Option<String> {
  headers
    .get(name)
    .and_then(|value| value.to_str().ok())
    .map(ToOwned::to_owned)
}
