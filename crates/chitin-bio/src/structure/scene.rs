//! Renderer-neutral scene extraction from indexed molecular structures.

use thiserror::Error;

use super::{AtomId, BondOrder, Element, ModelId, ResidueId, Structure};

/// Broad element families used by default molecular visualizations.
///
/// This compact classification keeps rendering policy independent from raw
/// PDB/mmCIF element spelling while retaining the exact [`Element`] in the
/// topology model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementCategory {
  /// Hydrogen and its isotopes when represented as hydrogen.
  Hydrogen,
  /// Carbon.
  Carbon,
  /// Nitrogen.
  Nitrogen,
  /// Oxygen.
  Oxygen,
  /// Phosphorus.
  Phosphorus,
  /// Sulfur.
  Sulfur,
  /// Fluorine, chlorine, bromine, or iodine.
  Halogen,
  /// A commonly encountered metallic element.
  Metal,
  /// Missing or unclassified element data.
  Other,
}

impl ElementCategory {
  /// Classifies an optional source element for visualization.
  pub fn from_element(element: Option<&Element>) -> Self {
    let Some(symbol) = element.map(|element| element.0.trim().to_ascii_uppercase()) else {
      return Self::Other;
    };

    match symbol.as_str() {
      "H" | "D" | "T" => Self::Hydrogen,
      "C" => Self::Carbon,
      "N" => Self::Nitrogen,
      "O" => Self::Oxygen,
      "P" => Self::Phosphorus,
      "S" => Self::Sulfur,
      "F" | "CL" | "BR" | "I" => Self::Halogen,
      "LI" | "NA" | "MG" | "AL" | "K" | "CA" | "MN" | "FE" | "CO" | "NI" | "CU" | "ZN" | "MO" | "CD" | "HG" => {
        Self::Metal
      }
      _ => Self::Other,
    }
  }
}

/// Axis-aligned bounds of the finite coordinates in a structure scene.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneBounds {
  /// Smallest coordinate on each Cartesian axis.
  pub min: [f32; 3],
  /// Largest coordinate on each Cartesian axis.
  pub max: [f32; 3],
}

impl SceneBounds {
  /// Returns the center of the axis-aligned bounds.
  pub fn center(self) -> [f32; 3] {
    [
      (self.min[0] + self.max[0]) * 0.5,
      (self.min[1] + self.max[1]) * 0.5,
      (self.min[2] + self.max[2]) * 0.5,
    ]
  }

  /// Returns the radius of a sphere enclosing the axis-aligned bounds.
  pub fn radius(self) -> f32 {
    let center = self.center();
    let dx = self.max[0] - center[0];
    let dy = self.max[1] - center[1];
    let dz = self.max[2] - center[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
  }
}

/// One coordinate-bearing atom prepared for scene extraction consumers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AtomSceneInstance {
  /// Stable topology identifier used for picking and UI lookup.
  pub atom_id: AtomId,
  /// Parent residue identifier used for residue-level interaction.
  pub residue_id: ResidueId,
  /// Cartesian position in ångströms.
  pub position: [f32; 3],
  /// Element family used by the default renderer color and radius policy.
  pub element: ElementCategory,
}

/// One coordinate-bearing explicit or inferred bond prepared for rendering.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BondSceneInstance {
  /// Stable topology identifiers for both endpoints.
  pub atom_ids: [AtomId; 2],
  /// Cartesian endpoint positions in ångströms.
  pub positions: [[f32; 3]; 2],
  /// Chemical bond order retained from the topology model.
  pub order: BondOrder,
}

/// Renderer-neutral geometry and identity mapping for one structure model.
///
/// The scene preserves scientific coordinates and stable topology IDs. GPU
/// layout, colors, tessellation, materials, and camera transforms remain the
/// responsibility of a renderer crate.
#[derive(Debug, Clone, PartialEq)]
pub struct StructureScene {
  /// Model represented by this scene.
  pub model_id: ModelId,
  /// Atoms with finite coordinates in the selected model.
  pub atoms: Vec<AtomSceneInstance>,
  /// Bonds whose two endpoints both have finite coordinates.
  pub bonds: Vec<BondSceneInstance>,
  /// Bounds enclosing every scene atom.
  pub bounds: SceneBounds,
}

/// Failure produced while extracting a renderable model from a structure.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum StructureSceneError {
  /// The requested model identifier does not exist.
  #[error("structure model {model_index} does not exist")]
  MissingModel {
    /// Dense index requested by the caller.
    model_index: usize,
  },
  /// The model references a coordinate set that does not exist.
  #[error("structure model {model_index} references missing coordinate set {coordinate_set_index}")]
  MissingCoordinateSet {
    /// Dense model index being extracted.
    model_index: usize,
    /// Dense coordinate-set index referenced by the model.
    coordinate_set_index: usize,
  },
  /// No model is available for the first-model convenience constructor.
  #[error("structure does not contain a model")]
  NoModels,
  /// The selected model has no finite Cartesian coordinates.
  #[error("structure model {model_index} has no finite atom coordinates")]
  NoFiniteCoordinates {
    /// Dense model index being extracted.
    model_index: usize,
  },
}

impl StructureScene {
  /// Extracts renderable atoms, bonds, and bounds for one structure model.
  ///
  /// # Parameters
  ///
  /// * `structure` is the validated indexed topology and coordinate model.
  /// * `model_id` selects the coordinate set to project into the scene.
  ///
  /// # Returns
  ///
  /// A renderer-neutral scene containing only finite coordinates, or a
  /// [`StructureSceneError`] when the model cannot produce visible geometry.
  ///
  /// # Examples
  ///
  /// ```
  /// use chitin_bio::structure::{PdbParser, StructureScene};
  ///
  /// let parsed = PdbParser::new().parse_bytes(b"ATOM      1  CA  GLY A   1       1.000   2.000   3.000  1.00 10.00           C  \nEND\n")?;
  /// let scene = StructureScene::from_first_model(&parsed.structure)?;
  /// assert_eq!(scene.atoms.len(), 1);
  /// # Ok::<(), Box<dyn std::error::Error>>(())
  /// ```
  pub fn from_model(structure: &Structure, model_id: ModelId) -> Result<Self, StructureSceneError> {
    let model = structure
      .models
      .get(model_id.index())
      .ok_or(StructureSceneError::MissingModel {
        model_index: model_id.index(),
      })?;
    let coordinates =
      structure
        .coordinate_set(model.coordinate_set_id)
        .ok_or(StructureSceneError::MissingCoordinateSet {
          model_index: model_id.index(),
          coordinate_set_index: model.coordinate_set_id.index(),
        })?;

    let mut atoms = Vec::with_capacity(structure.atoms.len());
    let mut bounds_min = [f32::INFINITY; 3];
    let mut bounds_max = [f32::NEG_INFINITY; 3];

    for (atom_index, atom) in structure.atoms.iter().enumerate() {
      let Some(position) = coordinates
        .positions
        .get(atom_index)
        .copied()
        .filter(is_finite_position)
      else {
        continue;
      };

      for axis in 0..3 {
        bounds_min[axis] = bounds_min[axis].min(position[axis]);
        bounds_max[axis] = bounds_max[axis].max(position[axis]);
      }
      atoms.push(AtomSceneInstance {
        atom_id: AtomId::from_index(atom_index),
        residue_id: atom.residue_id,
        position,
        element: ElementCategory::from_element(atom.element.as_ref()),
      });
    }

    if atoms.is_empty() {
      return Err(StructureSceneError::NoFiniteCoordinates {
        model_index: model_id.index(),
      });
    }

    // Bonds remain a topology relation until both endpoint positions exist in
    // this model. This naturally excludes alternate models represented by NaN.
    let bonds = structure
      .bonds
      .iter()
      .filter_map(|bond| {
        let a = coordinates.positions.get(bond.a.index()).copied()?;
        let b = coordinates.positions.get(bond.b.index()).copied()?;
        if !is_finite_position(&a) || !is_finite_position(&b) {
          return None;
        }
        Some(BondSceneInstance {
          atom_ids: [bond.a, bond.b],
          positions: [a, b],
          order: bond.order,
        })
      })
      .collect();

    Ok(Self {
      model_id,
      atoms,
      bonds,
      bounds: SceneBounds {
        min: bounds_min,
        max: bounds_max,
      },
    })
  }

  /// Extracts a scene for the first model in source order.
  ///
  /// # Parameters
  ///
  /// * `structure` is the validated indexed topology and coordinate model.
  ///
  /// # Returns
  ///
  /// The first model's renderer-neutral scene, or a [`StructureSceneError`]
  /// when the structure is empty or has no renderable coordinates.
  pub fn from_first_model(structure: &Structure) -> Result<Self, StructureSceneError> {
    let model_id = structure
      .models
      .first()
      .map(|model| model.id)
      .ok_or(StructureSceneError::NoModels)?;
    Self::from_model(structure, model_id)
  }
}

/// Reports whether all three Cartesian components are finite numbers.
fn is_finite_position(position: &[f32; 3]) -> bool {
  position.iter().all(|component| component.is_finite())
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::structure::{Bond, BondSource, PdbParser};

  const TWO_ATOM_PDB: &[u8] = b"ATOM      1  N   GLY A   1       1.000   2.000   3.000  1.00 10.00           N  \nATOM      2  CA  GLY A   1       5.000   6.000   7.000  1.00 10.00           C  \nEND\n";

  #[test]
  fn first_model_scene_preserves_ids_elements_and_bounds() {
    let parsed = PdbParser::new()
      .parse_bytes(TWO_ATOM_PDB)
      .unwrap_or_else(|error| panic!("test PDB should parse: {error}"));
    let scene = StructureScene::from_first_model(&parsed.structure)
      .unwrap_or_else(|error| panic!("test structure should produce a scene: {error}"));

    assert_eq!(scene.atoms.len(), 2);
    assert_eq!(scene.atoms[0].atom_id.index(), 0);
    assert_eq!(scene.atoms[0].element, ElementCategory::Nitrogen);
    assert_eq!(scene.atoms[1].element, ElementCategory::Carbon);
    assert_eq!(scene.bounds.min, [1.0, 2.0, 3.0]);
    assert_eq!(scene.bounds.max, [5.0, 6.0, 7.0]);
    assert_eq!(scene.bounds.center(), [3.0, 4.0, 5.0]);
  }

  #[test]
  fn scene_omits_bonds_with_a_missing_endpoint() {
    let parsed = PdbParser::new()
      .parse_bytes(TWO_ATOM_PDB)
      .unwrap_or_else(|error| panic!("test PDB should parse: {error}"));
    let mut structure = parsed.structure;
    structure.bonds.push(Bond {
      a: AtomId::from_index(0),
      b: AtomId::from_index(1),
      order: BondOrder::Unknown,
      source: BondSource::Conect,
    });
    structure.coordinates[0].positions[1] = [f32::NAN; 3];

    let scene = StructureScene::from_first_model(&structure)
      .unwrap_or_else(|error| panic!("remaining atom should produce a scene: {error}"));

    assert_eq!(scene.atoms.len(), 1);
    assert!(scene.bonds.is_empty());
  }
}
