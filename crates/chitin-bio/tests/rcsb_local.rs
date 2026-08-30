//! Offline regression tests for downloaded RCSB structure fixtures.
//!
//! The fixture directories are intentionally flat so adding a structure only
//! requires copying one `.pdb` file and/or one `.cif` file. These tests never
//! access the network; [`rcsb_online.rs`] owns that separate responsibility.

use std::path::{Path, PathBuf};

use chitin_bio::structure::{BondSource, MmcifParser, PdbParser, Structure, StructureParseResult, StructureScene};

/// Structure-file format associated with one local fixture directory.
#[derive(Clone, Copy)]
enum FixtureFormat {
  Pdb,
  Mmcif,
}

impl FixtureFormat {
  /// Returns the human-readable format label used in test output.
  fn label(self) -> &'static str {
    match self {
      Self::Pdb => "PDB",
      Self::Mmcif => "mmCIF",
    }
  }

  /// Returns the expected fixture file extension.
  fn extension(self) -> &'static str {
    match self {
      Self::Pdb => "pdb",
      Self::Mmcif => "cif",
    }
  }
}

/// Returns the configured RCSB fixture root, defaulting to checked-in fixtures.
fn fixture_root() -> PathBuf {
  let Some(configured) = std::env::var_os("CHITIN_BIO_FIXTURE_ROOT") else {
    return PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rcsb");
  };
  let configured = PathBuf::from(configured);
  if configured.is_absolute() {
    configured
  } else {
    // Relative paths are interpreted from the workspace root, matching the
    // location from which `just bio-local` is normally invoked.
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").join(configured)
  }
}

/// Lists flat fixture files with one expected extension in deterministic order.
fn fixture_paths(directory: &Path, extension: &str) -> Vec<PathBuf> {
  let entries =
    std::fs::read_dir(directory).unwrap_or_else(|error| panic!("{} should be readable: {error}", directory.display()));
  let mut paths = entries
    .map(|entry| entry.unwrap_or_else(|error| panic!("fixture directory entry should be readable: {error}")))
    .map(|entry| entry.path())
    .filter(|path| path.extension().and_then(|extension| extension.to_str()) == Some(extension))
    .collect::<Vec<_>>();
  paths.sort();
  paths
}

/// Parses a fixture with the reader corresponding to its file format.
fn parse_fixture(format: FixtureFormat, bytes: &[u8]) -> Result<StructureParseResult, String> {
  match format {
    FixtureFormat::Pdb => PdbParser::new().parse_bytes(bytes).map_err(|error| error.to_string()),
    FixtureFormat::Mmcif => MmcifParser::new().parse_bytes(bytes).map_err(|error| error.to_string()),
  }
}

/// Checks the invariants common to every parsed structure source.
fn assert_structure_is_valid(path: &Path, structure: &Structure) {
  assert!(
    !structure.models().is_empty(),
    "{} should contain a model",
    path.display()
  );
  assert!(!structure.atoms().is_empty(), "{} should contain atoms", path.display());
  structure
    .validate_invariants()
    .unwrap_or_else(|error| panic!("{} violates structure invariants: {error}", path.display()));
}

/// Prints the common macromolecular summary for one parsed fixture.
fn print_structure_summary(format: FixtureFormat, path: &Path, bytes: &[u8], parsed: &StructureParseResult) {
  let structure = &parsed.structure;
  let scene = StructureScene::from_first_model(structure)
    .unwrap_or_else(|error| panic!("{} should produce a render scene: {error}", path.display()));
  let inferred_bond_count = scene
    .bonds
    .iter()
    .filter(|bond| bond.source == BondSource::DistanceInference)
    .count();
  println!(
    "RCSB {} {}:
   - {} bytes,
   - {} models,
   - {} chains,
   - {} residues,
   - {} atoms,
   - {} bonds,
   - {} render bonds ({} distance-inferred),
   - {} polymer entities,
   - {} missing polymer residues,
   - {} secondary ranges,
   - {} diagnostics",
    path.file_stem().and_then(|value| value.to_str()).unwrap_or("?"),
    format.label(),
    bytes.len(),
    structure.models().len(),
    structure.chains().len(),
    structure.residues().len(),
    structure.atoms().len(),
    structure.bonds().len(),
    scene.bonds.len(),
    inferred_bond_count,
    structure.polymer_entities().len(),
    structure.missing_polymer_residues().len(),
    structure.secondary_ranges().len(),
    parsed.diagnostics.len(),
  );
}

#[test]
fn local_rcsb_fixtures_should_parse() {
  let root = fixture_root();
  let fixtures = [
    (
      FixtureFormat::Pdb,
      fixture_paths(&root.join("pdb"), FixtureFormat::Pdb.extension()),
    ),
    (
      FixtureFormat::Mmcif,
      fixture_paths(&root.join("mmcif"), FixtureFormat::Mmcif.extension()),
    ),
  ];
  let fixture_count = fixtures.iter().map(|(_, paths)| paths.len()).sum::<usize>();
  assert!(
    fixture_count > 0,
    "the local RCSB fixture directories should not be empty"
  );

  for (format, paths) in fixtures {
    for path in paths {
      let bytes = std::fs::read(&path).unwrap_or_else(|error| panic!("{} should be readable: {error}", path.display()));
      let parsed = parse_fixture(format, &bytes)
        .unwrap_or_else(|error| panic!("{} {} should parse: {error}", path.display(), format.label()));
      assert_structure_is_valid(&path, &parsed.structure);
      print_structure_summary(format, &path, &bytes, &parsed);
    }
  }
}

#[test]
fn local_pdb_and_mmcif_fixtures_should_have_matching_ids() {
  let root = fixture_root();
  let pdb_ids = fixture_paths(&root.join("pdb"), FixtureFormat::Pdb.extension())
    .into_iter()
    .filter_map(|path| {
      path
        .file_stem()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_uppercase)
    })
    .collect::<Vec<_>>();
  let mmcif_ids = fixture_paths(&root.join("mmcif"), FixtureFormat::Mmcif.extension())
    .into_iter()
    .filter_map(|path| {
      path
        .file_stem()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_uppercase)
    })
    .collect::<Vec<_>>();

  assert_eq!(pdb_ids, mmcif_ids, "PDB and mmCIF fixture IDs should be paired");
}
