//! RCSB provider client implementation.

use std::{sync::Arc, time::SystemTime};

use http::StatusCode;

use crate::{
  ArtifactFormat, DecodeError, DownloadedArtifact, HttpResponse, Provenance, ProviderId, TransportError,
  client::ClientRuntime,
};

use super::{PdbId, RcsbEndpoints, RcsbError, StructureFormat, dto::RcsbEntryDto};

/// RCSB Protein Data Bank provider client.
#[derive(Clone)]
pub struct RcsbClient {
  /// Shared client runtime.
  runtime: Arc<ClientRuntime>,
  /// Provider endpoint configuration.
  endpoints: RcsbEndpoints,
}

/// Small stable subset of RCSB entry metadata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RcsbEntryMetadata {
  /// Entry identifier.
  pub id: PdbId,
  /// Structure title, when provided by RCSB.
  pub title: Option<String>,
  /// Experimental method names.
  pub experimental_methods: Vec<String>,
  /// Initial release date string, when provided by RCSB.
  pub initial_release_date: Option<String>,
  /// Response provenance.
  pub provenance: Provenance,
}

impl RcsbClient {
  /// Creates an RCSB provider client from shared runtime state.
  ///
  /// # Parameters
  ///
  /// * `runtime` is shared by all provider clients from the top-level client.
  ///
  /// # Returns
  ///
  /// A provider client using public RCSB endpoints.
  pub(crate) fn new(runtime: Arc<ClientRuntime>) -> Self {
    Self {
      runtime,
      endpoints: RcsbEndpoints::default(),
    }
  }

  /// Downloads a raw RCSB structure file.
  ///
  /// # Parameters
  ///
  /// * `id` identifies the RCSB PDB entry.
  /// * `format` selects PDB or mmCIF raw content.
  ///
  /// # Returns
  ///
  /// A raw downloaded artifact with provenance. The content is not parsed.
  ///
  /// # Errors
  ///
  /// Returns [`RcsbError`] for transport failures, missing entries, rate
  /// limits, or other malformed provider responses.
  pub async fn download_structure(&self, id: PdbId, format: StructureFormat) -> Result<DownloadedArtifact, RcsbError> {
    let requested_at = SystemTime::now();
    let request = self.endpoints.structure_download_request(&id, format)?;
    let resolved_url = request.url.clone();
    let response = self.runtime.execute(request).await?;
    let response = map_status(response, &id)?;
    let provenance = Provenance::from_headers(
      ProviderId::Rcsb,
      id.as_str(),
      requested_at,
      resolved_url,
      &response.headers,
    );
    Ok(DownloadedArtifact {
      format: artifact_format(format),
      content: response.body,
      provenance,
    })
  }

  /// Fetches a small stable subset of RCSB entry metadata.
  ///
  /// # Parameters
  ///
  /// * `id` identifies the RCSB PDB entry.
  ///
  /// # Returns
  ///
  /// Typed metadata decoded from the RCSB Data API.
  ///
  /// # Errors
  ///
  /// Returns [`RcsbError`] for transport failures, missing entries, rate
  /// limits, JSON decode errors, or malformed metadata.
  pub async fn entry_metadata(&self, id: PdbId) -> Result<RcsbEntryMetadata, RcsbError> {
    let requested_at = SystemTime::now();
    let request = self.endpoints.entry_metadata_request(&id)?;
    let resolved_url = request.url.clone();
    let response = self.runtime.execute(request).await?;
    let response = map_status(response, &id)?;
    let provenance = Provenance::from_headers(
      ProviderId::Rcsb,
      id.as_str(),
      requested_at,
      resolved_url,
      &response.headers,
    );
    let dto: RcsbEntryDto = serde_json::from_slice(&response.body).map_err(|error| DecodeError {
      format: "RCSB entry JSON",
      message: error.to_string(),
    })?;
    metadata_from_dto(dto, provenance)
  }
}

/// Converts a structure format into an artifact format.
///
/// # Parameters
///
/// * `format` is the requested structure file format.
///
/// # Returns
///
/// The provider-neutral artifact format.
fn artifact_format(format: StructureFormat) -> ArtifactFormat {
  match format {
    StructureFormat::Pdb => ArtifactFormat::Pdb,
    StructureFormat::Mmcif => ArtifactFormat::Mmcif,
  }
}

/// Maps provider HTTP statuses into RCSB errors.
///
/// # Parameters
///
/// * `response` is the provider response.
/// * `id` is the requested PDB identifier.
///
/// # Returns
///
/// The response when it was successful.
fn map_status(response: HttpResponse, id: &PdbId) -> Result<HttpResponse, RcsbError> {
  match response.status {
    StatusCode::OK => Ok(response),
    StatusCode::NOT_FOUND => Err(RcsbError::EntryNotFound {
      id: id.as_str().to_string(),
    }),
    StatusCode::TOO_MANY_REQUESTS => Err(RcsbError::RateLimited),
    status if status.is_success() => Ok(response),
    _ => Err(RcsbError::Transport(TransportError::Other {
      message: format!("RCSB returned HTTP {}", response.status),
    })),
  }
}

/// Converts an RCSB metadata DTO into public metadata.
///
/// # Parameters
///
/// * `dto` is the decoded RCSB wire response.
/// * `provenance` records request and response metadata.
///
/// # Returns
///
/// Public RCSB entry metadata.
fn metadata_from_dto(dto: RcsbEntryDto, provenance: Provenance) -> Result<RcsbEntryMetadata, RcsbError> {
  let id = PdbId::new(&dto.rcsb_id).map_err(RcsbError::InvalidPdbId)?;
  Ok(RcsbEntryMetadata {
    id,
    title: dto.struct_.and_then(|value| value.title),
    experimental_methods: dto
      .exptl
      .unwrap_or_default()
      .into_iter()
      .filter_map(|value| value.method)
      .collect(),
    initial_release_date: dto.rcsb_accession_info.and_then(|value| value.initial_release_date),
    provenance,
  })
}
