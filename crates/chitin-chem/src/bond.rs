//! Bond identity and chemical order, without drawing or stereochemical shortcuts.

use crate::AtomId;

/// Supplied bond representation; no numeric aromatic bond order is implied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum BondOrder {
  /// Connectivity is known but its order is not.
  Unknown,
  /// A supplied single bond.
  Single,
  /// A supplied double bond.
  Double,
  /// A supplied triple bond.
  Triple,
  /// A supplied aromatic bond label, not a result of aromaticity perception.
  Aromatic,
}

/// An undirected edge whose endpoints belong to one [`crate::MolecularGraph`].
///
/// Endpoints retain insertion order for source mapping; connectivity itself is
/// unordered. Wedges and dashes are not bond orders and are not modelled here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bond {
  pub(crate) atoms: [AtomId; 2],
  pub(crate) order: BondOrder,
}

impl Bond {
  /// Returns the two endpoints in their supplied order.
  pub const fn atoms(&self) -> [AtomId; 2] {
    self.atoms
  }
  /// Returns the supplied chemical bond order.
  pub const fn order(&self) -> BondOrder {
    self.order
  }
}
