//! Solve shared directional-edge parity without reusing source spelling.

use crate::{BondId, BondOrder, DoubleBondRelation, GraphError, MolecularGraph, StereoError};
use std::collections::{BTreeMap, VecDeque};

/// Assigns slash bits in increasing-atom-handle orientation.
///
/// # Parameters
/// - `graph`: borrowed facts with validated graph-local reference descriptors.
///
/// # Returns
/// One consistent assignment for each connected constraint component. Shared
/// single bonds couple adjacent alkenes; choosing each double independently
/// would overwrite their common marker and silently change configuration.
///
/// # Errors
/// Rejects inconsistent parity cycles and assignments that would specify an
/// otherwise unannotated double bond. No partial text is emitted on failure.
pub(super) fn assign(graph: &MolecularGraph) -> Result<BTreeMap<BondId, bool>, StereoError> {
  let mut constraints: BTreeMap<BondId, Vec<(BondId, bool, BondId)>> = BTreeMap::new();
  for (id, stereo) in graph.double_bond_stereos() {
    let [a, b] = stereo.ends();
    let [ra, rb] = stereo.references();
    let left = graph.bond_between(a, ra).ok_or(StereoError::DoubleBond(id))?;
    let right = graph.bond_between(b, rb).ok_or(StereoError::DoubleBond(id))?;
    let parity = (stereo.relation() == DoubleBondRelation::OppositeSides) ^ (a > ra) ^ (b > rb);
    constraints.entry(left).or_default().push((right, parity, id));
    constraints.entry(right).or_default().push((left, parity, id));
  }
  if constraints.is_empty() {
    return Ok(BTreeMap::new());
  }
  let mut marked_at = BTreeMap::new();
  for &edge in constraints.keys() {
    let [a, b] = graph.bond(edge).ok_or(GraphError::MissingBond(edge))?.atoms();
    marked_at.entry(a).or_insert_with(Vec::new).push((b, edge));
    marked_at.entry(b).or_insert_with(Vec::new).push((a, edge));
  }
  // A neighboring alkene may mark the other substituent at a branched end.
  // Both references must then encode opposite outward directions, even when
  // their variables came from different double-bond descriptors.
  for (id, bond) in graph.bonds().filter(|(_, bond)| bond.order() == BondOrder::Double) {
    for end in bond.atoms() {
      let marked = marked_at.get(&end).map_or(&[][..], Vec::as_slice);
      if marked.len() > 2 {
        return Err(StereoError::DirectionConflict(id));
      }
      if let Some(&(a, left)) = marked.first() {
        for &(b, right) in marked.iter().skip(1) {
          let parity = true ^ (end > a) ^ (end > b);
          constraints.entry(left).or_default().push((right, parity, id));
          constraints.entry(right).or_default().push((left, parity, id));
        }
      }
    }
  }
  let mut values = BTreeMap::new();
  for &root in constraints.keys() {
    if values.contains_key(&root) {
      continue;
    }
    values.insert(root, false);
    let mut pending = VecDeque::from([root]);
    while let Some(edge) = pending.pop_front() {
      for &(other, parity, id) in &constraints[&edge] {
        let expected = values[&edge] ^ parity;
        if let Some(&previous) = values.get(&other) {
          if previous != expected {
            return Err(StereoError::DirectionConflict(id));
          }
        } else {
          values.insert(other, expected);
          pending.push_back(other);
        }
      }
    }
  }
  // Two marked sides imply a configuration even when not chosen deliberately.
  // Conservatively fail rather than invent a fact on a partially edited graph.
  for (id, bond) in graph.bonds() {
    if bond.order() != BondOrder::Double || graph.double_bond_stereo(id).is_some() {
      continue;
    }
    let mut marked = true;
    for end in bond.atoms() {
      marked &= marked_at.contains_key(&end);
    }
    if marked {
      return Err(StereoError::DirectionConflict(id));
    }
  }
  Ok(values)
}
