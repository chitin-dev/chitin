//! RCSB request construction.

use crate::{HttpRequest, TransportError};
use bytes::Bytes;
use http::{HeaderValue, header::CONTENT_TYPE};

use super::PdbId;

/// Default RCSB Data API base URL.
pub const DEFAULT_DATA_API_BASE_URL: &str = "https://data.rcsb.org";
/// Default RCSB file download base URL.
pub const DEFAULT_FILE_DOWNLOAD_BASE_URL: &str = "https://files.rcsb.org";

/// Supported RCSB downloadable structure file formats.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StructureFormat {
  /// Legacy PDB text format.
  Pdb,
  /// PDBx/mmCIF text format.
  Mmcif,
}

/// RCSB endpoint configuration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RcsbEndpoints {
  /// Base URL for the RCSB Data API.
  data_api_base_url: String,
  /// Base URL for downloadable structure files.
  file_download_base_url: String,
}

impl StructureFormat {
  /// Returns the stable identifier used by clients and UI selectors.
  pub fn id(self) -> &'static str {
    match self {
      Self::Pdb => "pdb",
      Self::Mmcif => "mmcif",
    }
  }

  /// Returns the user-facing structure format label.
  pub fn label(self) -> &'static str {
    match self {
      Self::Pdb => "PDB",
      Self::Mmcif => "mmCIF",
    }
  }

  /// Returns the RCSB download extension.
  pub fn extension(self) -> &'static str {
    match self {
      Self::Pdb => "pdb",
      Self::Mmcif => "cif",
    }
  }

  /// Returns the canonical filename for an RCSB entry in this format.
  pub fn filename(self, id: &PdbId) -> String {
    format!("{}.{}", id.as_str(), self.extension())
  }
}

impl RcsbEndpoints {
  /// Creates endpoint configuration for public RCSB services.
  pub fn new() -> Self {
    Self {
      data_api_base_url: DEFAULT_DATA_API_BASE_URL.to_string(),
      file_download_base_url: DEFAULT_FILE_DOWNLOAD_BASE_URL.to_string(),
    }
  }

  /// Creates endpoint configuration with custom base URLs.
  ///
  /// # Parameters
  ///
  /// * `data_api_base_url` is the base URL for metadata endpoints.
  /// * `file_download_base_url` is the base URL for structure downloads.
  ///
  /// # Returns
  ///
  /// Endpoint configuration with normalized base URLs.
  ///
  /// # Errors
  ///
  /// Returns [`TransportError::InvalidUrl`] when either base URL is invalid.
  pub fn with_base_urls(data_api_base_url: &str, file_download_base_url: &str) -> Result<Self, TransportError> {
    let data_api_base_url = normalize_base_url(data_api_base_url)?;
    let file_download_base_url = normalize_base_url(file_download_base_url)?;
    Ok(Self {
      data_api_base_url,
      file_download_base_url,
    })
  }

  /// Builds an RCSB entry metadata request.
  ///
  /// # Parameters
  ///
  /// * `id` identifies the requested PDB entry.
  ///
  /// # Returns
  ///
  /// A GET request for `/rest/v1/core/entry/{id}`.
  pub fn entry_metadata_request(&self, id: &PdbId) -> Result<HttpRequest, TransportError> {
    let url = parse_url(&format!(
      "{}/rest/v1/core/entry/{}",
      self.data_api_base_url,
      id.as_str()
    ))?;
    Ok(HttpRequest::get(url))
  }

  /// Builds an RCSB structure download request.
  ///
  /// # Parameters
  ///
  /// * `id` identifies the requested PDB entry.
  /// * `format` selects the raw structure format.
  ///
  /// # Returns
  ///
  /// A GET request for `/download/{id}.{extension}`.
  pub fn structure_download_request(&self, id: &PdbId, format: StructureFormat) -> Result<HttpRequest, TransportError> {
    let url = parse_url(&format!(
      "{}/download/{}.{}",
      self.file_download_base_url,
      id.as_str(),
      format.extension()
    ))?;
    Ok(HttpRequest::get(url))
  }

  /// Builds an RCSB GraphQL request.
  ///
  /// # Parameters
  ///
  /// * `query` is the GraphQL JSON payload.
  ///
  /// # Returns
  ///
  /// A POST request for `/graphql`.
  pub fn graphql_request(&self, query: impl Into<Bytes>) -> Result<HttpRequest, TransportError> {
    let url = parse_url(&format!("{}/graphql", self.data_api_base_url))?;
    let mut request = HttpRequest::post(url, query);
    request
      .headers
      .insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    Ok(request)
  }
}

impl Default for RcsbEndpoints {
  /// Creates default RCSB endpoints.
  fn default() -> Self {
    Self::new()
  }
}

/// Validates and normalizes a base URL.
///
/// # Parameters
///
/// * `value` is the base URL string.
///
/// # Returns
///
/// A base URL without trailing slash characters.
fn normalize_base_url(value: &str) -> Result<String, TransportError> {
  let trimmed = value.trim_end_matches('/');
  parse_url(trimmed)?;
  Ok(trimmed.to_string())
}

/// Parses a full URL.
///
/// # Parameters
///
/// * `value` is the full URL string.
///
/// # Returns
///
/// A parsed URL.
fn parse_url(value: &str) -> Result<url::Url, TransportError> {
  url::Url::parse(value).map_err(|_| TransportError::InvalidUrl { url: value.to_string() })
}
