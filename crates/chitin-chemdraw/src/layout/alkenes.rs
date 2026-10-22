//! Required alkene geometry, applied before optional quality refinement.

use super::{Layout2d, LayoutError};
use crate::{
  geometry::{Bounds, Point},
  stereo_geometry::relation,
};
use chitin_chem::MolecularGraph;
use std::collections::BTreeSet;

/// Reflects one entire acyclic side when its reference relation is inverted.
///
/// # Parameters
/// - `graph`: validated annotations over the same IDs as `layout`.
/// - `layout`: complete unit-bond placement, owned by the current generation call.
/// - `budget`: shared work allowance across all constraints and components.
///
/// # Returns
/// Complete geometry with unchanged bond lengths and rigid ring shapes. A
/// reflection of a complete side also preserves other internal stereo relations.
/// This stage always runs, including when quality refinement is disabled.
///
/// # Errors
/// Rejects work exhaustion or an unsolved coupled constraint system. Cyclic
/// mismatches use transactional distance closure, never a torn-open ring.
pub(super) fn enforce(graph: &MolecularGraph, layout: &mut Layout2d, mut budget: usize) -> Result<(), LayoutError> {
  if graph.double_bond_stereos().next().is_none() {
    return Ok(());
  }
  let initial = layout.clone();
  match reflect(graph, layout, &mut budget) {
    Err(LayoutError::DoubleBondStereo(id)) => {
      *layout = initial;
      if super::closure::repair(graph, layout, &mut budget)? {
        Ok(())
      } else {
        Err(LayoutError::DoubleBondStereo(id))
      }
    }
    result => result,
  }
}

/// Keeps the exact rigid reflection path for ordinary acyclic constraints.
fn reflect(graph: &MolecularGraph, layout: &mut Layout2d, budget: &mut usize) -> Result<(), LayoutError> {
  for (id, stereo) in graph.double_bond_stereos() {
    *budget = budget.checked_sub(1).ok_or(LayoutError::StereoBudgetExceeded)?;
    let [a, b] = stereo.ends();
    let [ra, rb] = stereo.references();
    let points = [a, b, ra, rb].map(|atom| layout.positions[&atom]);
    let actual = relation(points).ok_or(LayoutError::DoubleBondStereo(id))?;
    if actual == stereo.relation() {
      continue;
    }
    let mut side = BTreeSet::from([b]);
    let mut pending = vec![b];
    while let Some(atom) = pending.pop() {
      let neighbors = graph.neighbors(atom)?;
      *budget = budget
        .checked_sub(1 + neighbors.len())
        .ok_or(LayoutError::StereoBudgetExceeded)?;
      for (other, edge) in neighbors {
        if edge == id {
          continue;
        }
        if other == a {
          return Err(LayoutError::DoubleBondStereo(id));
        }
        if side.insert(other) {
          pending.push(other);
        }
      }
    }
    let [origin, end, _, _] = points;
    let length = origin.distance(end);
    let (ux, uy) = ((end.x - origin.x) / length, (end.y - origin.y) / length);
    for atom in side {
      *budget = budget.checked_sub(1).ok_or(LayoutError::StereoBudgetExceeded)?;
      if atom == b {
        continue;
      }
      let point = layout.positions[&atom];
      let (dx, dy) = (point.x - origin.x, point.y - origin.y);
      let projection = dx * ux + dy * uy;
      layout.positions.insert(
        atom,
        Point::new(
          origin.x + 2. * projection * ux - dx,
          origin.y + 2. * projection * uy - dy,
        ),
      );
    }
  }
  layout.bounds = Bounds::from_points(layout.positions.values().copied())?;
  Ok(())
}
