//! Shared chemical representation boundary for layout and depiction.
//!
//! Integer-only inputs remain borrowed. Aromatic inputs use the same detached
//! domain assignment in both consumers, never independent display heuristics.

use std::borrow::Cow;

use chitin_chem::{
  BondOrder, MolecularGraph,
  perception::{KekuleError, KekuleOptions, kekulize},
};

/// Resolves supplied aromatic facts without modifying the source or its handles.
///
/// # Errors
/// Propagates the restricted domain model's unsupported-state, no-assignment,
/// and work-budget failures. No representation is published before completion.
pub(crate) fn integer_bonds(graph: &MolecularGraph) -> Result<Cow<'_, MolecularGraph>, KekuleError> {
  if graph.atoms().any(|(_, atom)| atom.is_aromatic())
    || graph.bonds().any(|(_, bond)| bond.order() == BondOrder::Aromatic)
  {
    Ok(Cow::Owned(kekulize(graph, &KekuleOptions::default())?))
  } else {
    Ok(Cow::Borrowed(graph))
  }
}
