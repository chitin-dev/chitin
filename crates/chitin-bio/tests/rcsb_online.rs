//! Explicit online integration tests for the RCSB download and parser boundary.

use chitin_bio::structure::{MmcifParser, PdbParser};
use chitin_databases::{
  Client, ClientConfig,
  providers::rcsb::{PdbId, StructureFormat},
};

#[tokio::test]
#[ignore = "requires network access to files.rcsb.org"]
async fn downloads_pdb_to_temp_file_and_parses_it() {
  let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("temporary directory should be created: {error}"));
  let path = directory.path().join("4HHB.pdb");
  let id = PdbId::new("4HHB").unwrap_or_else(|error| panic!("fixture ID should be valid: {error}"));

  Client::new(ClientConfig::default())
    .unwrap_or_else(|error| panic!("production HTTP client should initialize: {error}"))
    .rcsb()
    .download_structure_to_path(id, StructureFormat::Pdb, &path, |_, _| {})
    .await
    .unwrap_or_else(|error| panic!("RCSB PDB download should succeed: {error}"));

  let bytes = std::fs::read(&path).unwrap_or_else(|error| panic!("downloaded PDB should be readable: {error}"));
  let parsed = PdbParser::new()
    .parse_bytes(&bytes)
    .unwrap_or_else(|error| panic!("downloaded PDB should parse: {error}"));
  println!(
    "RCSB PDB parse result\n  file: {}\n  bytes: {}\n  models: {}\n  chains: {}\n  residues: {}\n  atoms: {}\n  coordinate sets: {}\n  diagnostics: {}",
    path.display(),
    bytes.len(),
    parsed.structure.models.len(),
    parsed.structure.chains.len(),
    parsed.structure.residues.len(),
    parsed.structure.atoms.len(),
    parsed.structure.coordinates.len(),
    parsed.diagnostics.len(),
  );
  for (atom_id, atom) in parsed.structure.atoms.iter().enumerate() {
    let position = parsed.structure.coordinates[0].positions[atom_id];
    println!(
      "  atom {atom_id}: {} {} at ({:.3}, {:.3}, {:.3})",
      atom.name,
      atom.residue_id.index(),
      position[0],
      position[1],
      position[2]
    );
  }
  assert!(!parsed.structure.atoms.is_empty());
  assert!(!parsed.structure.models.is_empty());

  directory
    .close()
    .unwrap_or_else(|error| panic!("temporary download should be removed: {error}"));
  assert!(!path.exists());
}

#[tokio::test]
#[ignore = "requires network access to files.rcsb.org"]
async fn downloads_mmcif_to_temp_file_and_parses_it() {
  let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("temporary directory should be created: {error}"));
  let path = directory.path().join("4HHB.cif");
  let id = PdbId::new("4HHB").unwrap_or_else(|error| panic!("fixture ID should be valid: {error}"));

  Client::new(ClientConfig::default())
    .unwrap_or_else(|error| panic!("production HTTP client should initialize: {error}"))
    .rcsb()
    .download_structure_to_path(id, StructureFormat::Mmcif, &path, |_, _| {})
    .await
    .unwrap_or_else(|error| panic!("RCSB mmCIF download should succeed: {error}"));

  let bytes = std::fs::read(&path).unwrap_or_else(|error| panic!("downloaded mmCIF should be readable: {error}"));
  let parsed = MmcifParser::new()
    .parse_bytes(&bytes)
    .unwrap_or_else(|error| panic!("downloaded mmCIF should parse: {error}"));
  println!(
    "RCSB mmCIF parse result\n  file: {}\n  bytes: {}\n  models: {}\n  chains: {}\n  residues: {}\n  atoms: {}\n  coordinate sets: {}\n  diagnostics: {}",
    path.display(),
    bytes.len(),
    parsed.structure.models.len(),
    parsed.structure.chains.len(),
    parsed.structure.residues.len(),
    parsed.structure.atoms.len(),
    parsed.structure.coordinates.len(),
    parsed.diagnostics.len(),
  );
  for (atom_id, atom) in parsed.structure.atoms.iter().enumerate() {
    let position = parsed.structure.coordinates[0].positions[atom_id];
    println!(
      "  atom {atom_id}: {} {} at ({:.3}, {:.3}, {:.3})",
      atom.name,
      atom.residue_id.index(),
      position[0],
      position[1],
      position[2]
    );
  }
  assert!(!parsed.structure.atoms.is_empty());
  assert!(!parsed.structure.models.is_empty());

  directory
    .close()
    .unwrap_or_else(|error| panic!("temporary download should be removed: {error}"));
  assert!(!path.exists());
}
