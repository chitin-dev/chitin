//! Coordinate-independent relative geometry of two explicit alkene references.

use crate::AtomId;

/// Relative sides of two chosen ligands, not CIP E/Z priority labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoubleBondRelation {
  /// The reference ligands lie on the same side of the double-bond axis.
  SameSide,
  /// The reference ligands lie on opposite sides of the double-bond axis.
  OppositeSides,
}

impl DoubleBondRelation {
  /// Returns the relation after substituting the other ligand at one endpoint.
  pub const fn inverted(self) -> Self {
    match self {
      Self::SameSide => Self::OppositeSides,
      Self::OppositeSides => Self::SameSide,
    }
  }
}

/// One explicit reference at each endpoint of an annotated double bond.
///
/// Construction is deliberately topology-free; installation on a graph checks
/// connectivity and coordination. Identical substituents are not ranked here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DoubleBondStereo {
  /// Ordered axis endpoints, paired by index with the reference atoms.
  ends: [AtomId; 2],
  /// Explicit single-bond neighbors, one at each endpoint.
  references: [AtomId; 2],
  /// Supplied relative configuration, independent of drawing orientation.
  relation: DoubleBondRelation,
}

impl DoubleBondStereo {
  /// Creates a descriptor; use `set_double_bond_stereo` to validate topology.
  pub const fn new(ends: [AtomId; 2], references: [AtomId; 2], relation: DoubleBondRelation) -> Self {
    Self {
      ends,
      references,
      relation,
    }
  }
  /// Returns the ordered double-bond endpoints.
  pub const fn ends(&self) -> [AtomId; 2] {
    self.ends
  }
  /// Returns the explicit references paired with the endpoints.
  pub const fn references(&self) -> [AtomId; 2] {
    self.references
  }
  /// Returns the supplied relation, not an inferred CIP assignment.
  pub const fn relation(&self) -> DoubleBondRelation {
    self.relation
  }
}
