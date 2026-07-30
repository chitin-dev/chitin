//! RCSB wire-format DTOs.

use serde::Deserialize;

/// Small subset of RCSB entry metadata returned by the Data API.
#[derive(Debug, Deserialize)]
pub(crate) struct RcsbEntryDto {
  /// Entry identifier.
  pub rcsb_id: String,
  /// Structural title block.
  #[serde(rename = "struct")]
  pub struct_: Option<RcsbStructDto>,
  /// Experimental method records.
  pub exptl: Option<Vec<RcsbExperimentalMethodDto>>,
  /// Accession metadata.
  pub rcsb_accession_info: Option<RcsbAccessionInfoDto>,
}

/// RCSB `struct` object.
#[derive(Debug, Deserialize)]
pub(crate) struct RcsbStructDto {
  /// Structure title.
  pub title: Option<String>,
}

/// RCSB experimental method object.
#[derive(Debug, Deserialize)]
pub(crate) struct RcsbExperimentalMethodDto {
  /// Experimental method name.
  pub method: Option<String>,
}

/// RCSB accession info object.
#[derive(Debug, Deserialize)]
pub(crate) struct RcsbAccessionInfoDto {
  /// Initial release date string.
  pub initial_release_date: Option<String>,
}
