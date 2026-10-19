//! Normalize lexical single-bond directions into relative alkene facts.

use super::{SmilesError, SmilesErrorKind, error};
use crate::{AtomId, BondId, BondOrder, DoubleBondRelation, DoubleBondStereo, MolecularGraph};
use std::{
  collections::{BTreeMap, BTreeSet},
  ops::Range,
};

/// One marker in lexical endpoint order; reversing traversal reverses its bit.
pub(super) struct Direction {
  /// Source traversal, including normalized ring-opening orientation.
  pub ends: [AtomId; 2],
  /// True for slash, false for backslash, not an E/Z designation.
  pub slash: bool,
  /// Original marker location for a positioned contradiction or subset error.
  pub span: Range<usize>,
}

/// Installs only fully specified, noncumulated double-bond configurations.
///
/// # Parameters
/// - `graph`: completed topology; no more incident edits may follow installation.
/// - `marks`: lexical directions with graph-local edge identities and source spans.
///
/// # Returns
/// Opposing markers at one endpoint must agree after changing the reference
/// ligand. Shared conjugated markers may constrain several doubles. Unconsumed
/// partial/orphan markers are rejected explicitly rather than silently erased.
///
/// # Errors
/// Reports positioned contradictions or unsupported coordination. The parser
/// owns this detached graph and discards it entirely if installation fails.
pub(super) fn install(graph: &mut MolecularGraph, marks: BTreeMap<BondId, Direction>) -> Result<(), SmilesError> {
  if marks.is_empty() {
    return Ok(());
  }
  // Index only marked edges once. Repeatedly scanning the full neighborhood
  // of a high-degree editable draft would make this stage quadratic.
  let mut at = BTreeMap::new();
  for (&edge, mark) in &marks {
    let [a, b] = mark.ends;
    at.entry(a).or_insert_with(Vec::new).push((b, edge, mark));
    at.entry(b).or_insert_with(Vec::new).push((a, edge, mark));
  }
  for row in at.values_mut() {
    row.sort_unstable_by_key(|&(neighbor, _, _)| neighbor);
  }
  let doubles: Vec<_> = graph
    .bonds()
    .filter(|(_, bond)| bond.order() == BondOrder::Double)
    .map(|(id, bond)| (id, bond.atoms()))
    .collect();
  let mut used = BTreeSet::new();
  for (id, ends) in doubles {
    let mut sites = Vec::new();
    for end in ends {
      let marked = at.get(&end).map_or(&[][..], Vec::as_slice);
      if let Some((_, _, first)) = marked.first() {
        let outward = first.slash ^ (first.ends[1] == end);
        if marked.len() > 2
          || marked
            .iter()
            .skip(1)
            .any(|(_, _, mark)| (mark.slash ^ (mark.ends[1] == end)) == outward)
        {
          return Err(error(
            first.span.clone(),
            SmilesErrorKind::Syntax("conflicting directions at double-bond endpoint"),
          ));
        }
      }
      sites.push(marked);
    }
    if let (Some(&(a, _, left)), Some(&(b, _, right))) = (sites[0].first(), sites[1].first()) {
      let same = (left.slash ^ (left.ends[1] == ends[0])) == (right.slash ^ (right.ends[1] == ends[1]));
      let relation = if same {
        DoubleBondRelation::SameSide
      } else {
        DoubleBondRelation::OppositeSides
      };
      graph
        .set_double_bond_stereo(id, Some(DoubleBondStereo::new(ends, [a, b], relation)))
        .map_err(|e| error(left.span.clone(), e.into()))?;
      used.extend(sites.into_iter().flatten().map(|(_, edge, _)| *edge));
    }
  }
  if let Some((_, mark)) = marks.iter().find(|(id, _)| !used.contains(id)) {
    return Err(error(
      mark.span.clone(),
      SmilesErrorKind::Unsupported("partial or unpaired directional stereochemistry"),
    ));
  }
  Ok(())
}
