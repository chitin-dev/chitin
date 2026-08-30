//! Distance-based covalent bond inference for atomic coordinate models.

use std::collections::{HashMap, HashSet};

use thiserror::Error;

use crate::structure::{Atom, AtomId, BondOrder, Element, ModelId, Structure};

/// Maximum neighbor-query radius used by the default inference policy.
const DEFAULT_MAX_SEARCH_DISTANCE: f32 = 4.0;
/// Coincident or nearly coincident atoms below this distance are not bonded.
const DEFAULT_MIN_BOND_DISTANCE: f32 = 0.1;
/// Mol*-style fallback threshold for an unrecognized element symbol.
const DEFAULT_ELEMENT_THRESHOLD: f32 = 2.001;

/// Compact normalized element identity used inside the hot neighbor loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ElementKind {
  Hydrogen,
  Carbon,
  Nitrogen,
  Oxygen,
  Phosphorus,
  Sulfur,
  Silicon,
  Halogen,
  Metal,
  Other,
}

/// Tunable geometric limits for distance-based bond inference.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BondInferenceConfig {
  /// Largest distance considered by the spatial neighbor query, in ångströms.
  pub max_search_distance: f32,
  /// Smallest accepted nonzero bond distance, in ångströms.
  pub min_bond_distance: f32,
}

impl Default for BondInferenceConfig {
  fn default() -> Self {
    Self {
      max_search_distance: DEFAULT_MAX_SEARCH_DISTANCE,
      min_bond_distance: DEFAULT_MIN_BOND_DISTANCE,
    }
  }
}

/// A model-specific bond derived from atom coordinates rather than source data.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InferredBond {
  /// Coordinate model from which the bond was inferred.
  pub model_id: ModelId,
  /// Stable atom identifiers in ascending dense-index order.
  pub atom_ids: [AtomId; 2],
  /// Bond order, which remains unknown for a distance-only inference.
  pub order: BondOrder,
  /// Measured interatomic distance in ångströms.
  pub distance: f32,
}

/// Failure produced before distance-based bond inference can run.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum BondInferenceError {
  /// The requested model identifier does not exist.
  #[error("structure model {model_index} does not exist")]
  MissingModel {
    /// Dense model index requested by the caller.
    model_index: usize,
  },
  /// The selected model references a missing coordinate set.
  #[error("structure model {model_index} references missing coordinate set {coordinate_set_index}")]
  MissingCoordinateSet {
    /// Dense model index being analyzed.
    model_index: usize,
    /// Dense coordinate-set index referenced by the model.
    coordinate_set_index: usize,
  },
  /// One of the configured geometric limits is invalid.
  #[error("invalid bond inference configuration: {message}")]
  InvalidConfiguration {
    /// Static explanation of the rejected relationship.
    message: &'static str,
  },
}

/// Infers model-specific covalent connections using spatial neighbor queries.
///
/// The implementation follows the broad Mol* fallback strategy: source bonds
/// take precedence, incompatible alternate locations are excluded, hydrogen–
/// hydrogen pairs are skipped, and element-pair distance thresholds determine
/// the remaining connections. A uniform spatial grid limits comparisons to 27
/// neighboring cells, avoiding a full \(O(n^2)\) atom-pair scan for ordinary
/// molecular coordinate distributions.
///
/// # Parameters
///
/// * `structure` supplies atoms, explicit bonds, models, and coordinate sets.
/// * `model_id` selects the coordinates used for geometric inference.
/// * `config` controls the neighbor-query and minimum-distance limits.
///
/// # Returns
///
/// New model-specific bonds in stable atom-index order. Pairs already present
/// in `structure.bonds` are omitted. Invalid model references and geometric
/// limits return [`BondInferenceError`].
///
/// # Examples
///
/// ```
/// use chitin_bio::{
///   chemistry::{BondInferenceConfig, infer_bonds},
///   structure::PdbParser,
/// };
///
/// let pdb = b"ATOM      1  N   GLY A   1       0.000   0.000   0.000  1.00 10.00           N  \nATOM      2  CA  GLY A   1       1.450   0.000   0.000  1.00 10.00           C  \nEND\n";
/// let parsed = PdbParser::new().parse_bytes(pdb)?;
/// let model_id = parsed.structure.models()[0].id;
/// let bonds = infer_bonds(&parsed.structure, model_id, BondInferenceConfig::default())?;
/// assert_eq!(bonds.len(), 1);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn infer_bonds(
  structure: &Structure,
  model_id: ModelId,
  config: BondInferenceConfig,
) -> Result<Vec<InferredBond>, BondInferenceError> {
  validate_config(config)?;
  let model = structure
    .models
    .get(model_id.index())
    .ok_or(BondInferenceError::MissingModel {
      model_index: model_id.index(),
    })?;
  let coordinates =
    structure
      .coordinate_set(model.coordinate_set_id)
      .ok_or(BondInferenceError::MissingCoordinateSet {
        model_index: model_id.index(),
        coordinate_set_index: model.coordinate_set_id.index(),
      })?;

  let explicit_pairs: HashSet<_> = structure
    .bonds
    .iter()
    .map(|bond| ordered_pair(bond.a.index(), bond.b.index()))
    .collect();
  // Normalize source element strings exactly once. Neighbor traversal can be
  // substantially larger than the atom table for dense structures.
  let element_kinds: Vec<_> = structure
    .atoms
    .iter()
    .map(|atom| classify_element(atom.element.as_ref()))
    .collect();
  let mut cells: HashMap<[i32; 3], Vec<usize>> = HashMap::new();
  let mut inferred = Vec::new();

  for (atom_index, atom) in structure.atoms.iter().enumerate() {
    let Some(position) = coordinates
      .positions
      .get(atom_index)
      .copied()
      .filter(is_finite_position)
    else {
      continue;
    };
    let cell = spatial_cell(position, config.max_search_distance);

    // Only previously inserted atoms are queried, so every unordered pair is
    // examined once without a second global deduplication table.
    for offset_x in -1..=1 {
      for offset_y in -1..=1 {
        for offset_z in -1..=1 {
          let neighbor = [cell[0] + offset_x, cell[1] + offset_y, cell[2] + offset_z];
          let Some(candidate_indices) = cells.get(&neighbor) else {
            continue;
          };
          for &candidate_index in candidate_indices {
            if explicit_pairs.contains(&(candidate_index, atom_index)) {
              continue;
            }
            let candidate_atom = &structure.atoms[candidate_index];
            if !alternate_locations_are_compatible(candidate_atom, atom) {
              continue;
            }
            let candidate_position = coordinates.positions[candidate_index];
            let Some(max_distance) = pairing_threshold(element_kinds[candidate_index], element_kinds[atom_index])
            else {
              continue;
            };
            let distance_squared = squared_distance(candidate_position, position);
            let minimum_squared = config.min_bond_distance * config.min_bond_distance;
            let maximum = max_distance.min(config.max_search_distance);
            if distance_squared <= minimum_squared || distance_squared > maximum * maximum {
              continue;
            }

            inferred.push(InferredBond {
              model_id,
              atom_ids: [AtomId::from_index(candidate_index), AtomId::from_index(atom_index)],
              order: BondOrder::Unknown,
              distance: distance_squared.sqrt(),
            });
          }
        }
      }
    }
    cells.entry(cell).or_default().push(atom_index);
  }

  inferred.sort_by_key(|bond| (bond.atom_ids[0].index(), bond.atom_ids[1].index()));
  Ok(inferred)
}

/// Validates spatial-grid limits before coordinates are traversed.
fn validate_config(config: BondInferenceConfig) -> Result<(), BondInferenceError> {
  if !config.max_search_distance.is_finite() || config.max_search_distance <= 0.0 {
    return Err(BondInferenceError::InvalidConfiguration {
      message: "max_search_distance must be finite and greater than zero",
    });
  }
  if !config.min_bond_distance.is_finite() || config.min_bond_distance < 0.0 {
    return Err(BondInferenceError::InvalidConfiguration {
      message: "min_bond_distance must be finite and non-negative",
    });
  }
  if config.min_bond_distance >= config.max_search_distance {
    return Err(BondInferenceError::InvalidConfiguration {
      message: "min_bond_distance must be smaller than max_search_distance",
    });
  }
  Ok(())
}

/// Converts a Cartesian coordinate to a uniform spatial-grid cell.
fn spatial_cell(position: [f32; 3], cell_width: f32) -> [i32; 3] {
  [
    (position[0] / cell_width).floor() as i32,
    (position[1] / cell_width).floor() as i32,
    (position[2] / cell_width).floor() as i32,
  ]
}

/// Returns the accepted distance threshold for one element pair.
fn pairing_threshold(a: ElementKind, b: ElementKind) -> Option<f32> {
  if a == ElementKind::Hydrogen && b == ElementKind::Hydrogen {
    return None;
  }

  if let Some(threshold) = element_pair_override(a, b) {
    return Some(threshold);
  }
  let threshold_a = element_threshold(a);
  let threshold_b = element_threshold(b);
  Some((threshold_a + threshold_b) / 1.95)
}

/// Returns special experimentally motivated thresholds for common atom pairs.
fn element_pair_override(a: ElementKind, b: ElementKind) -> Option<f32> {
  let pair = if a <= b { (a, b) } else { (b, a) };
  match pair {
    (ElementKind::Hydrogen, ElementKind::Carbon) => Some(1.20),
    (ElementKind::Hydrogen, ElementKind::Nitrogen) => Some(1.15),
    (ElementKind::Hydrogen, ElementKind::Oxygen) => Some(1.10),
    (ElementKind::Hydrogen, ElementKind::Phosphorus) => Some(1.47),
    (ElementKind::Hydrogen, ElementKind::Sulfur) => Some(1.45),
    (ElementKind::Carbon, ElementKind::Carbon) => Some(1.75),
    (ElementKind::Carbon, ElementKind::Nitrogen) => Some(1.60),
    (ElementKind::Carbon, ElementKind::Oxygen) => Some(1.59),
    (ElementKind::Nitrogen, ElementKind::Nitrogen) => Some(1.60),
    (ElementKind::Nitrogen, ElementKind::Oxygen) => Some(1.45),
    (ElementKind::Oxygen, ElementKind::Oxygen) => Some(1.60),
    (ElementKind::Oxygen, ElementKind::Phosphorus) => Some(1.88),
    (ElementKind::Oxygen, ElementKind::Sulfur) => Some(1.80),
    (ElementKind::Sulfur, ElementKind::Sulfur) => Some(2.30),
    _ => None,
  }
}

/// Returns a conservative element-level distance threshold in ångströms.
fn element_threshold(element: ElementKind) -> f32 {
  match element {
    ElementKind::Hydrogen => 1.42,
    ElementKind::Carbon => 1.75,
    ElementKind::Nitrogen => 1.60,
    ElementKind::Oxygen => 1.52,
    ElementKind::Halogen => 1.80,
    ElementKind::Silicon | ElementKind::Sulfur => 1.90,
    ElementKind::Phosphorus => 2.00,
    ElementKind::Metal => 2.70,
    ElementKind::Other => DEFAULT_ELEMENT_THRESHOLD,
  }
}

/// Classifies source element spelling into the inference threshold table.
fn classify_element(element: Option<&Element>) -> ElementKind {
  let Some(symbol) = element.map(|element| element.0.trim()) else {
    return ElementKind::Other;
  };
  if matches_ignore_ascii_case(symbol, &["H", "D", "T"]) {
    ElementKind::Hydrogen
  } else if symbol.eq_ignore_ascii_case("C") {
    ElementKind::Carbon
  } else if symbol.eq_ignore_ascii_case("N") {
    ElementKind::Nitrogen
  } else if symbol.eq_ignore_ascii_case("O") {
    ElementKind::Oxygen
  } else if symbol.eq_ignore_ascii_case("P") {
    ElementKind::Phosphorus
  } else if symbol.eq_ignore_ascii_case("S") {
    ElementKind::Sulfur
  } else if symbol.eq_ignore_ascii_case("SI") {
    ElementKind::Silicon
  } else if matches_ignore_ascii_case(symbol, &["F", "CL", "BR", "I"]) {
    ElementKind::Halogen
  } else if is_metal(symbol) {
    ElementKind::Metal
  } else {
    ElementKind::Other
  }
}

/// Reports whether an element uses the broad metallic threshold policy.
fn is_metal(symbol: &str) -> bool {
  matches_ignore_ascii_case(
    symbol,
    &[
      "LI", "NA", "K", "RB", "CS", "FR", "BE", "MG", "CA", "SR", "BA", "RA", "AL", "SC", "TI", "V", "CR", "MN", "FE",
      "CO", "NI", "CU", "ZN", "Y", "ZR", "NB", "MO", "TC", "RU", "RH", "PD", "AG", "CD", "HF", "TA", "W", "RE", "OS",
      "IR", "PT", "AU", "HG", "PB",
    ],
  )
}

/// Performs allocation-free case-insensitive membership testing.
fn matches_ignore_ascii_case(symbol: &str, candidates: &[&str]) -> bool {
  candidates
    .iter()
    .any(|candidate| symbol.eq_ignore_ascii_case(candidate))
}

/// Reports whether alternate-location conformers may share a bond.
fn alternate_locations_are_compatible(a: &Atom, b: &Atom) -> bool {
  a.altloc.is_none() || b.altloc.is_none() || a.altloc == b.altloc
}

/// Returns an ascending dense-index pair.
fn ordered_pair(a: usize, b: usize) -> (usize, usize) {
  if a <= b { (a, b) } else { (b, a) }
}

/// Computes squared Euclidean distance without an unnecessary square root.
fn squared_distance(a: [f32; 3], b: [f32; 3]) -> f32 {
  let dx = a[0] - b[0];
  let dy = a[1] - b[1];
  let dz = a[2] - b[2];
  dx * dx + dy * dy + dz * dz
}

/// Reports whether all Cartesian components are finite numbers.
fn is_finite_position(position: &[f32; 3]) -> bool {
  position.iter().all(|component| component.is_finite())
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::structure::{Bond, BondSource, PdbParser};

  const BONDED_PDB: &[u8] = b"ATOM      1  N   GLY A   1       0.000   0.000   0.000  1.00 10.00           N  \nATOM      2  CA  GLY A   1       1.450   0.000   0.000  1.00 10.00           C  \nEND\n";

  fn parsed_bonded_structure() -> Structure {
    PdbParser::new()
      .parse_bytes(BONDED_PDB)
      .unwrap_or_else(|error| panic!("bond inference fixture should parse: {error}"))
      .structure
  }

  #[test]
  fn infer_bonds_should_connect_atoms_inside_pair_threshold() {
    let structure = parsed_bonded_structure();
    let bonds = infer_bonds(&structure, structure.models[0].id, BondInferenceConfig::default())
      .unwrap_or_else(|error| panic!("bond inference should succeed: {error}"));

    assert_eq!(bonds.len(), 1);
  }

  #[test]
  fn infer_bonds_should_not_duplicate_explicit_source_bonds() {
    let mut structure = parsed_bonded_structure();
    structure.bonds.push(Bond {
      a: AtomId::from_index(0),
      b: AtomId::from_index(1),
      order: BondOrder::Unknown,
      source: BondSource::Conect,
    });
    let bonds = infer_bonds(&structure, structure.models[0].id, BondInferenceConfig::default())
      .unwrap_or_else(|error| panic!("bond inference should succeed: {error}"));

    assert!(bonds.is_empty());
  }

  #[test]
  fn infer_bonds_should_reject_incompatible_alternate_locations() {
    let mut structure = parsed_bonded_structure();
    structure.atoms[0].altloc = Some('A');
    structure.atoms[1].altloc = Some('B');
    let bonds = infer_bonds(&structure, structure.models[0].id, BondInferenceConfig::default())
      .unwrap_or_else(|error| panic!("bond inference should succeed: {error}"));

    assert!(bonds.is_empty());
  }

  #[test]
  fn infer_bonds_should_find_neighbors_across_negative_grid_cells() {
    let mut structure = parsed_bonded_structure();
    structure.coordinates[0].positions[0] = [-0.2, 0.0, 0.0];
    structure.coordinates[0].positions[1] = [0.2, 0.0, 0.0];
    let bonds = infer_bonds(&structure, structure.models[0].id, BondInferenceConfig::default())
      .unwrap_or_else(|error| panic!("bond inference should succeed: {error}"));

    assert_eq!(bonds.len(), 1);
  }
}
