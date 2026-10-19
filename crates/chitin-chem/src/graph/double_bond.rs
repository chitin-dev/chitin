//! Validated alkene annotations and conservative edit invalidation.

use super::*;
use crate::DoubleBondStereo;

impl MolecularGraph {
  /// Returns a supplied relative configuration for a live double bond.
  pub fn double_bond_stereo(&self, bond: BondId) -> Option<&DoubleBondStereo> {
    self.double_bond_stereo.get(&bond)
  }
  /// Visits supplied double-bond configurations in stable handle order.
  pub fn double_bond_stereos(&self) -> impl ExactSizeIterator<Item = (BondId, &DoubleBondStereo)> + '_ {
    self.double_bond_stereo.iter().map(|(&id, stereo)| (id, stereo))
  }

  /// Installs a relative alkene fact without assigning CIP ranks.
  ///
  /// # Parameters
  /// - `id`: a live double bond in this graph.
  /// - `stereo`: explicit single-bond references, one at each axis endpoint.
  ///
  /// # Returns
  /// The previous annotation. Clearing accepts any live bond. Structural edits
  /// at either axis endpoint invalidate the annotation conservatively.
  ///
  /// # Errors
  /// Rejects stale or disconnected references, aromatic endpoints, cumulated
  /// double bonds and unsupported coordination. Failure leaves the graph intact.
  pub fn set_double_bond_stereo(
    &mut self,
    id: BondId,
    stereo: Option<DoubleBondStereo>,
  ) -> Result<Option<DoubleBondStereo>, StereoError> {
    let bond = self.bond(id).ok_or(GraphError::MissingBond(id))?;
    if let Some(stereo) = stereo {
      let ends = stereo.ends();
      let references = stereo.references();
      if bond.order() != BondOrder::Double || !(bond.atoms() == ends || bond.atoms() == [ends[1], ends[0]]) {
        return Err(StereoError::DoubleBond(id));
      }
      for (end, reference) in ends.into_iter().zip(references) {
        let atom = self.atom(end).ok_or(GraphError::MissingAtom(end))?;
        let mut neighbors = self.neighbors(end)?;
        let degree = neighbors.len();
        if atom.is_aromatic()
          || !(2..=3).contains(&degree)
          || !matches!(atom.declared_hydrogens(), Some(0 | 1))
          || degree + usize::from(atom.declared_hydrogens().unwrap_or(0)) > 3
          || !matches!(
            atom.implicit_hydrogen_policy(),
            ImplicitHydrogenPolicy::Infer | ImplicitHydrogenPolicy::Fixed(0)
          )
          || ends.contains(&reference)
          || !self
            .bond_between(end, reference)
            .and_then(|edge| self.bond(edge))
            .is_some_and(|edge| edge.order() == BondOrder::Single)
          || neighbors
            .any(|(_, edge)| edge != id && self.bond(edge).is_none_or(|edge| edge.order() != BondOrder::Single))
        {
          return Err(StereoError::DoubleBond(id));
        }
      }
      Ok(self.double_bond_stereo.insert(id, stereo))
    } else {
      Ok(self.double_bond_stereo.remove(&id))
    }
  }

  /// Drops annotations whose central coordination was changed, not remote facts.
  pub(super) fn clear_double_stereo_at(&mut self, atoms: &[AtomId]) {
    self
      .double_bond_stereo
      .retain(|_, stereo| !stereo.ends().iter().any(|end| atoms.contains(end)));
  }
}
