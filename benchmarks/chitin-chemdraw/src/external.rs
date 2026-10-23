//! Benchmark-only coordinate exchange, validation and isolated provider execution.

use anyhow::{Context, Result, ensure};
use chitin_chem::{BondOrder, MolecularGraph};
use chitin_chemdraw::{geometry::Point, layout::Layout2d};
use serde::{Deserialize, Serialize};
use std::{
  collections::BTreeMap,
  io::{Read, Seek, SeekFrom, Write},
  path::Path,
  process::{Command, Stdio},
  time::Duration,
};

const SCRIPT: &str = include_str!("../engines/coordgen/worker.py");

/// Identifies the embedded adapter independently of changes to the controller.
pub fn adapter_checksum() -> String {
  use sha2::Digest;
  format!("{:x}", sha2::Sha256::digest(SCRIPT.as_bytes()))
}

/// Checks a legacy snapshot that predates separate adapter provenance.
///
/// The old launcher passed its `include_str!` constant directly as a Python
/// argument, so its exact UTF-8 bytes remain embedded in the verified executable.
/// An unmatched adapter is rejected, never guessed or silently substituted.
pub fn matches_legacy_adapter(renderer: &Path) -> Result<bool> {
  let bytes = std::fs::read(renderer)?;
  Ok(bytes.windows(SCRIPT.len()).any(|part| part == SCRIPT.as_bytes()))
}

/// Original parser facts, captured before the external chemical perception stage.
#[derive(Debug, Deserialize, Serialize)]
pub struct Atom {
  element: u8,
  isotope: u16,
  charge: i16,
  aromatic: bool,
  hydrogens: u8,
}

/// Source-order endpoint ordinals and supplied, not perceived, bond designation.
#[derive(Debug, Deserialize, Serialize)]
pub struct Bond {
  ends: [usize; 2],
  order: String,
}

/// Complete external geometry with source coverage and provider provenance.
#[derive(Debug, Deserialize, Serialize)]
pub struct ExternalLayout {
  atoms: Vec<Atom>,
  bonds: Vec<Bond>,
  positions: Vec<[f64; 2]>,
  pub metadata: serde_json::Value,
  pub precision: String,
}

/// A provider failure remains a case, never a reason to remove it from the sample.
#[derive(Debug, Deserialize)]
pub struct Response {
  pub stage: String,
  pub error: Option<String>,
  pub timings_ms: BTreeMap<String, f64>,
  pub layout: Option<ExternalLayout>,
}

/// Probes the exact interpreter before creating a run, with a bounded subprocess.
pub fn probe(python: &Path) -> Result<serde_json::Value> {
  let result = invoke(python, None, Duration::from_secs(15))?.context("external provider probe timed out")?;
  ensure!(
    result["kind"] == "rdkit_coordgen" && result["rdkit_version"].is_string(),
    "external provider unavailable: {result}"
  );
  Ok(result)
}

/// Executes one direct child; killing it cannot leave a nested chemistry worker.
///
/// # Returns
/// Complete JSON, or `None` after killing and reaping the timed-out provider.
/// File-backed channels avoid pipe deadlocks and bound the retained output.
///
/// # Errors
/// Startup, transport and malformed provider responses are explicit errors.
pub fn invoke(
  python: &Path,
  request: Option<&serde_json::Value>,
  timeout: Duration,
) -> Result<Option<serde_json::Value>> {
  let mut input = tempfile::tempfile()?;
  if let Some(request) = request {
    serde_json::to_writer(&mut input, request)?;
  }
  input.flush()?;
  input.seek(SeekFrom::Start(0))?;
  let output = tempfile::tempfile()?;
  let errors = tempfile::tempfile()?;
  let mut command = Command::new(python);
  command.args(["-I", "-c", SCRIPT]);
  if request.is_none() {
    command.arg("--probe");
  }
  let mut child = command
    .stdin(input)
    .stdout(output.try_clone()?)
    .stderr(Stdio::from(errors.try_clone()?))
    .spawn()
    .context("start external coordinate provider")?;
  let (status, expired) = crate::wait_with_deadline(&mut child, timeout)?;
  if expired {
    return Ok(None);
  }
  let mut output = output;
  output.seek(SeekFrom::Start(0))?;
  let mut bytes = Vec::new();
  output.take(16 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
  ensure!(bytes.len() <= 16 * 1024 * 1024, "external response exceeds size limit");
  if !status.success() {
    let mut errors = errors;
    errors.seek(SeekFrom::Start(0))?;
    let mut message = String::new();
    errors.take(16_384).read_to_string(&mut message)?;
    anyhow::bail!("external provider exited {status}: {message}");
  }
  Ok(Some(
    serde_json::from_slice(&bytes).context("invalid external provider JSON")?,
  ))
}

/// Verifies source-order mapping and normalizes only the global length unit.
///
/// # Returns
/// Exact imported geometry up to positive uniform scaling. Connectivity, isotope,
/// charge, supplied aromaticity and attached-H declarations must match. No atom
/// is dropped, reordered, repaired or packed by the evaluator.
///
/// # Errors
/// Missing/foreign atoms, altered edges, nonfinite coordinates and collapsed
/// layouts are rejected instead of silently invoking an internal fallback.
pub fn validate(graph: &MolecularGraph, external: &ExternalLayout) -> Result<Layout2d> {
  ensure!(
    external.atoms.len() == graph.atom_count() && external.positions.len() == graph.atom_count(),
    "external atom coverage mismatch"
  );
  ensure!(
    external.bonds.len() == graph.bond_count(),
    "external bond coverage mismatch"
  );
  let ids: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
  for ((_, source), atom) in graph.atoms().zip(&external.atoms) {
    ensure!(
      atom.element == source.element().atomic_number()
        && atom.isotope == source.isotope().map_or(0, |isotope| isotope.mass_number())
        && atom.charge == source.formal_charge().unwrap_or(0)
        && atom.aromatic == source.is_aromatic()
        && atom.hydrogens == source.declared_hydrogens().unwrap_or(0),
      "external source atom mismatch"
    );
  }
  let indices: BTreeMap<_, _> = ids.iter().enumerate().map(|(index, &id)| (id, index)).collect();
  let expected: BTreeMap<_, _> = graph
    .bonds()
    .map(|(_, bond)| {
      let [a, b] = bond.atoms().map(|id| indices[&id]);
      let order = match bond.order() {
        BondOrder::Single => "1",
        BondOrder::Double => "2",
        BondOrder::Triple => "3",
        BondOrder::Aromatic => "aromatic",
        _ => "unsupported",
      };
      ([a.min(b), a.max(b)], order)
    })
    .collect();
  let mut actual = BTreeMap::new();
  for bond in &external.bonds {
    let [a, b] = bond.ends;
    ensure!(
      a < ids.len() && b < ids.len() && a != b,
      "external bond endpoint out of range"
    );
    ensure!(
      actual.insert([a.min(b), a.max(b)], bond.order.as_str()).is_none(),
      "duplicate external bond"
    );
  }
  ensure!(actual == expected, "external connectivity or bond order mismatch");
  let mut lengths = Vec::new();
  for &[a, b] in expected.keys() {
    let [ax, ay] = external.positions[a];
    let [bx, by] = external.positions[b];
    let length = (ax - bx).hypot(ay - by);
    ensure!(
      length.is_finite() && length > 1e-12,
      "external collapsed/nonfinite bond"
    );
    lengths.push(length);
  }
  lengths.sort_by(f64::total_cmp);
  let scale = if lengths.is_empty() {
    1.
  } else if lengths.len().is_multiple_of(2) {
    (lengths[lengths.len() / 2 - 1] + lengths[lengths.len() / 2]) / 2.
  } else {
    lengths[lengths.len() / 2]
  };
  ensure!(scale.is_finite() && scale > 0., "invalid external length scale");
  imported(
    graph,
    &external
      .positions
      .iter()
      .map(|&[x, y]| [x / scale, y / scale])
      .collect::<Vec<_>>(),
  )
}

/// Replays already validated historical coordinates without the external runtime.
pub fn imported(graph: &MolecularGraph, positions: &[[f64; 2]]) -> Result<Layout2d> {
  ensure!(
    positions.len() == graph.atom_count(),
    "imported coordinate coverage mismatch"
  );
  Ok(Layout2d::from_positions(
    graph
      .atoms()
      .zip(positions)
      .map(|((id, _), &[x, y])| (id, Point::new(x, y)))
      .collect(),
  )?)
}

#[cfg(test)]
mod tests {
  use super::*;
  use chitin_chem::io::smiles::read_smiles;

  /// Independent exchange fixture includes an isotope and a declared H count.
  fn fixture() -> Result<(MolecularGraph, ExternalLayout)> {
    let graph = read_smiles("[2H]O[13CH3]")?.into_graph();
    let layout = serde_json::from_value(serde_json::json!({
      "atoms": [
        {"element": 1, "isotope": 2, "charge": 0, "aromatic": false, "hydrogens": 0},
        {"element": 8, "isotope": 0, "charge": 0, "aromatic": false, "hydrogens": 0},
        {"element": 6, "isotope": 13, "charge": 0, "aromatic": false, "hydrogens": 3}
      ],
      "bonds": [{"ends": [0,1], "order": "1"}, {"ends": [1,2], "order": "1"}],
      "positions": [[0.,0.], [2.,0.], [3.,1.7320508075688772]],
      "metadata": {}, "precision": "standard"
    }))?;
    Ok((graph, layout))
  }

  #[test]
  fn unit_normalization_should_preserve_source_order_and_relative_geometry() -> Result<()> {
    let (graph, exchange) = fixture()?;
    let before = graph.clone();
    let layout = validate(&graph, &exchange)?;
    let points: Vec<_> = graph.atoms().map(|(id, _)| layout.positions()[&id]).collect();
    assert_eq!(graph, before);
    assert_eq!(points[0], Point::new(0., 0.));
    assert!((points[1].x - 1.).abs() < 1e-10);
    assert!((points[2].y - 3_f64.sqrt() / 2.).abs() < 1e-10);
    Ok(())
  }

  #[test]
  fn altered_source_atoms_and_bonds_should_be_rejected() -> Result<()> {
    for mutation in 0..7 {
      let (graph, mut exchange) = fixture()?;
      match mutation {
        0 => exchange.atoms.swap(0, 1),
        1 => exchange.atoms[2].isotope = 12,
        2 => exchange.atoms[2].hydrogens = 2,
        3 => exchange.bonds[0].order = "2".into(),
        4 => exchange.bonds[0].ends = [0, 2],
        5 => exchange.bonds[0].ends = [0, 99],
        _ => exchange.bonds[1].ends = [0, 1],
      }
      assert!(validate(&graph, &exchange).is_err(), "mutation {mutation}");
    }
    Ok(())
  }

  #[test]
  fn invalid_geometry_should_never_fall_back_to_internal_layout() -> Result<()> {
    for mutation in 0..3 {
      let (graph, mut exchange) = fixture()?;
      match mutation {
        0 => {
          exchange.positions.pop();
        }
        1 => exchange.positions[1] = exchange.positions[0],
        _ => exchange.positions[0][0] = f64::NAN,
      }
      assert!(validate(&graph, &exchange).is_err());
    }
    Ok(())
  }
}
