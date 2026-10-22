//! Ordered local stereochemical facts, independent of coordinates and CIP ranks.
//!
//! Graph-local ligand identities belong to graph annotations, not atom values.
//! A permutation changes the written winding, never the molecular configuration.

use crate::{AtomId, BondId, GraphError};

mod double_bond;
pub use double_bond::{DoubleBondRelation, DoubleBondStereo};

/// One tetrahedral reference direction; virtual ligands are not graph vertices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StereoLigand {
  /// An explicitly bonded atom in the same graph namespace.
  Atom(AtomId),
  /// Exactly one declared attached H on the center, distinct from an H vertex.
  Hydrogen,
  /// Undrawn fourth direction at a three-coordinate pyramidal center.
  /// Codec convention places this virtual reference last; it is never an atom.
  LonePair,
}

/// Local winding, looking from the first ligand toward the center.
///
/// The remaining three ligands run in this order. These are not CIP R/S labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TetrahedralWinding {
  /// Anticlockwise, corresponding to `@`/`@TH1` in SMILES neighbor order.
  Anticlockwise,
  /// Clockwise, corresponding to `@@`/`@TH2` in SMILES neighbor order.
  Clockwise,
}

impl TetrahedralWinding {
  /// Returns the opposite winding after an odd reference permutation.
  pub const fn inverted(self) -> Self {
    match self {
      Self::Anticlockwise => Self::Clockwise,
      Self::Clockwise => Self::Anticlockwise,
    }
  }
}

/// Four distinct ordered ligands plus their supplied relative winding.
///
/// Construction does not prove a stereogenic center or assign CIP labels.
/// Installation on a graph validates connectivity and supported coordination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TetrahedralStereo {
  /// Ordered references; position zero defines the viewing direction.
  ligands: [StereoLigand; 4],
  /// Winding of references one, two and three from reference zero.
  winding: TetrahedralWinding,
}

/// Rejected stereo references, coordination, or shared directional constraints.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum StereoError {
  /// Double-bond references are repeated, disconnected, or unsupported.
  #[error("invalid double-bond stereochemistry at bond {0}")]
  DoubleBond(BondId),
  /// Relative directional constraints cannot be serialized consistently.
  #[error("conflicting double-bond directional constraints at bond {0}")]
  DirectionConflict(BondId),
  /// A reference list contains duplicates or is not a permutation of another.
  #[error("invalid tetrahedral ligand permutation")]
  Ligands,
  /// The center does not have four supported atom/H/lone-pair directions.
  #[error("unsupported tetrahedral coordination at atom {0}")]
  Coordination(AtomId),
  /// A referenced graph entity is not live.
  #[error(transparent)]
  Topology(#[from] GraphError),
}

impl TetrahedralStereo {
  /// Creates a relative tetrahedral descriptor from four distinct references.
  ///
  /// # Errors
  /// Rejects duplicate references. Use `MolecularGraph::set_tetrahedral` to
  /// validate these identities against actual neighbors and declared H.
  pub fn new(ligands: [StereoLigand; 4], winding: TetrahedralWinding) -> Result<Self, StereoError> {
    for i in 0..4 {
      if ligands[i + 1..].contains(&ligands[i]) {
        return Err(StereoError::Ligands);
      }
    }
    Ok(Self { ligands, winding })
  }

  /// Returns the ordered atom/H/lone-pair reference directions.
  pub const fn ligands(&self) -> &[StereoLigand; 4] {
    &self.ligands
  }
  /// Returns the winding in this descriptor's reference order.
  pub const fn winding(&self) -> TetrahedralWinding {
    self.winding
  }

  /// Reorders references while preserving the represented configuration.
  ///
  /// # Parameters
  /// - `ligands`: the same four identities in a possibly different order.
  ///
  /// # Returns
  /// A descriptor with inverted winding exactly for an odd permutation.
  ///
  /// # Errors
  /// Rejects missing, duplicated, or substituted ligand identities.
  pub fn reordered(self, ligands: [StereoLigand; 4]) -> Result<Self, StereoError> {
    let mut result = Self::new(ligands, self.winding)?;
    let mut permutation = [0; 4];
    for (i, ligand) in ligands.iter().enumerate() {
      permutation[i] = self
        .ligands
        .iter()
        .position(|other| other == ligand)
        .ok_or(StereoError::Ligands)?;
    }
    let inversions = (0..4)
      .flat_map(|a| (a + 1..4).map(move |b| (a, b)))
      .filter(|&(a, b)| permutation[a] > permutation[b])
      .count();
    if inversions % 2 != 0 {
      result.winding = result.winding.inverted();
    }
    Ok(result)
  }
}
