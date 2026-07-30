//! Transport-neutral response and downloaded artifact types.

use bytes::Bytes;
use http::{HeaderMap, StatusCode};

use crate::Provenance;

/// Buffered HTTP response returned by database transports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpResponse {
  /// HTTP response status.
  pub status: StatusCode,
  /// HTTP response headers.
  pub headers: HeaderMap,
  /// Buffered response body.
  pub body: Bytes,
}

/// Provider-neutral downloaded artifact format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtifactFormat {
  /// Legacy PDB text format.
  Pdb,
  /// PDBx/mmCIF text format.
  Mmcif,
  /// JSON metadata or API response content.
  Json,
}

/// Raw downloaded provider artifact with provenance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadedArtifact {
  /// Raw artifact format.
  pub format: ArtifactFormat,
  /// Raw artifact bytes.
  pub content: Bytes,
  /// Provider and transport provenance.
  pub provenance: Provenance,
}
