//! Batch online tests for the RCSB download and parser boundary.
//!
//! The identifiers are maintained in `rcsb_ids.yaml` beside this test. The
//! test downloads both PDB and mmCIF for every listed identifier, then parses
//! both files through the public biological structure readers.

use std::path::Path;

use chitin_bio::structure::{MmcifParser, PdbParseResult, PdbParser};
use chitin_databases::providers::rcsb::{RcsbBatchDownloadEvent, RcsbBatchDownloadRequest};
use chitin_databases::{
  Client, ClientConfig,
  providers::rcsb::{PdbId, StructureFormat},
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
/// YAML-backed set of structures exercised by the online test.
struct RcsbFixture {
  /// Four-character PDB identifiers to download in both supported formats.
  pdb_ids: Vec<String>,
}

/// Loads and validates the identifiers listed in the adjacent YAML fixture.
fn fixture_ids() -> Vec<PdbId> {
  let fixture: RcsbFixture = serde_yaml::from_str(include_str!("rcsb_ids.yaml"))
    .unwrap_or_else(|error| panic!("RCSB YAML fixture should be valid: {error}"));
  fixture
    .pdb_ids
    .iter()
    .map(|value| PdbId::new(value).unwrap_or_else(|error| panic!("invalid PDB ID {value:?}: {error}")))
    .collect()
}

/// Creates one download request for each configured ID and supported format.
///
/// # Parameters
///
/// * `ids` contains validated PDB identifiers from the YAML fixture.
/// * `directory` is the temporary directory receiving downloaded files.
///
/// # Returns
///
/// One batch request for each identifier/format pair, with deterministic file
/// names derived from the canonical identifier.
fn fixture_requests(ids: &[PdbId], directory: &Path) -> Vec<RcsbBatchDownloadRequest> {
  ids
    .iter()
    .flat_map(|id| {
      [StructureFormat::Pdb, StructureFormat::Mmcif]
        .into_iter()
        .map(move |format| (id, format))
    })
    .map(|(id, format)| RcsbBatchDownloadRequest {
      id: id.clone(),
      format,
      destination: directory.join(format.filename(id)),
    })
    .collect()
}

/// Parses one downloaded structure using the reader selected by its format.
///
/// # Parameters
///
/// * `format` selects the PDB or mmCIF reader.
/// * `bytes` contains the downloaded structure file.
///
/// # Returns
///
/// The common parsed structure result, or a formatted parser error.
fn parse_download(format: StructureFormat, bytes: &[u8]) -> Result<PdbParseResult, String> {
  match format {
    StructureFormat::Pdb => PdbParser::new().parse_bytes(bytes).map_err(|error| error.to_string()),
    StructureFormat::Mmcif => MmcifParser::new().parse_bytes(bytes).map_err(|error| error.to_string()),
  }
}

/// Downloads and parses every PDB/mmCIF pair declared in `rcsb_ids.yaml`.
#[tokio::test]
#[ignore = "requires network access to files.rcsb.org"]
async fn downloads_configured_structures_and_parses_them() {
  let ids = fixture_ids();
  assert!(!ids.is_empty(), "the RCSB fixture should contain at least one ID");

  let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("temporary directory should be created: {error}"));
  let requests = fixture_requests(&ids, directory.path());
  let client = Client::new(ClientConfig::default())
    .unwrap_or_else(|error| panic!("production HTTP client should initialize: {error}"));

  client
    .rcsb()
    .download_structures_to_paths(&requests, |event| match event {
      RcsbBatchDownloadEvent::Started { index, total, id } => {
        println!("[{index}/{total}] downloading {id}");
      }
      RcsbBatchDownloadEvent::Completed { index, total, id, path } => {
        println!("[{index}/{total}] downloaded {id} to {}", path.display());
      }
      RcsbBatchDownloadEvent::Progress { .. } => {}
    })
    .await
    .unwrap_or_else(|error| panic!("configured RCSB batch download should succeed: {error}"));

  for request in &requests {
    let path = &request.destination;
    let bytes = std::fs::read(path).unwrap_or_else(|error| panic!("{} should be readable: {error}", path.display()));
    let parsed = parse_download(request.format, &bytes)
      .unwrap_or_else(|error| panic!("{} {} should parse: {error}", request.id, request.format.label()));
    println!(
      "RCSB {} {}: {} bytes, {} models, {} chains, {} residues, {} atoms, {} diagnostics",
      request.id,
      request.format.label(),
      bytes.len(),
      parsed.structure.models.len(),
      parsed.structure.chains.len(),
      parsed.structure.residues.len(),
      parsed.structure.atoms.len(),
      parsed.diagnostics.len(),
    );
    assert!(
      !parsed.structure.atoms.is_empty(),
      "{} should contain atoms",
      path.display()
    );
    assert!(
      !parsed.structure.models.is_empty(),
      "{} should contain models",
      path.display()
    );
  }

  directory
    .close()
    .unwrap_or_else(|error| panic!("temporary downloads should be removed: {error}"));
}
