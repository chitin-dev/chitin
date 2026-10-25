//! Native local double-bond orientation and required supplied-stereo validation.
//! Source attribution and redistribution terms are in licenses/coordinate-generation.txt.

use super::{
  Error, Work,
  math::{EPSILON, Vector, same_side},
  topology::Model,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Corrects newly placed neighbors before their branches are expanded.
///
/// # Errors
/// Missing frame references, degenerate axes and exhausted work reject the frame.
pub(super) fn orient_neighbors(
  model: &Model,
  node: usize,
  seen: &BTreeSet<usize>,
  points: &mut BTreeMap<usize, Vector>,
  work: &mut Work,
) -> Result<(), Error> {
  let mut axis = None;
  let mut mirror = Vec::new();
  for &(neighbor, edge) in &model.neighbors[node] {
    if model.edges[edge].order == 2 && !model.edges[edge].cyclic && seen.contains(&neighbor) {
      axis = Some(edge);
    } else {
      mirror.push(neighbor);
    }
  }
  let Some(edge) = axis else {
    return Ok(());
  };
  let [a, b] = model.edges[edge].ends;
  let refs = if let Some(&(atoms, same)) = model
    .stereo
    .iter()
    .find(|(atoms, _)| (atoms[1] == a && atoms[2] == b) || (atoms[1] == b && atoms[2] == a))
  {
    let references = if atoms[1] == a {
      [atoms[0], atoms[3]]
    } else {
      [atoms[3], atoms[0]]
    };
    (references, same)
  } else {
    let (Some(left), Some(right)) = (
      super::ranking::first_neighbor(model, a, b, work)?,
      super::ranking::first_neighbor(model, b, a, work)?,
    ) else {
      return Ok(());
    };
    ([left, right], false)
  };
  let get = |atom| points.get(&atom).copied().ok_or(Error::CoordinateRange);
  let (start, end) = (get(a)?, get(b)?);
  // The first endpoint is visited before the opposite branch is expanded.
  // Unplaced references retain their initial zero coordinates in this stage.
  let left = points.get(&refs.0[0]).copied().unwrap_or_default();
  let right = points.get(&refs.0[1]).copied().unwrap_or_default();
  if same_side(left, right, start, end) != refs.1 {
    work.charge(mirror.len())?;
    for atom in mirror {
      let point = points.get_mut(&atom).ok_or(Error::CoordinateRange)?;
      *point = reflect(*point, start, end)?.rounded();
    }
  }
  Ok(())
}

/// Enforces supplied relations by whole-side acyclic reflection, never old closure.
///
/// # Errors
/// An unsatisfied cyclic relation, degenerate geometry or exhausted stereo budget
/// rejects the snapshot instead of borrowing a different coordinate generator.
pub(super) fn enforce(model: &Model, points: &mut [Vector], work: &mut Work) -> Result<(), Error> {
  for &(atoms, expected) in &model.stereo {
    work.charge(1)?;
    let [left, a, b, right] = atoms;
    let edge = model.neighbors[a]
      .iter()
      .find_map(|&(n, edge)| (n == b).then_some(edge))
      .ok_or(Error::CoordinateRange)?;
    if relation(points, atoms) == Some(expected) {
      continue;
    }
    if model.edges[edge].cyclic {
      return Err(Error::DoubleBondStereo(model.edges[edge].id));
    }
    work.charge(points.len())?;
    let mut seen = vec![false; points.len()];
    seen[b] = true;
    let mut queue = VecDeque::from([b]);
    while let Some(atom) = queue.pop_front() {
      work.charge(1 + model.neighbors[atom].len())?;
      for &(next, e) in &model.neighbors[atom] {
        if e == edge {
          continue;
        }
        if next == a {
          return Err(Error::DoubleBondStereo(model.edges[edge].id));
        }
        if !seen[next] {
          seen[next] = true;
          queue.push_back(next);
        }
      }
    }
    let (start, end) = (points[a], points[b]);
    for (atom, &movable) in seen.iter().enumerate() {
      if movable && atom != b {
        points[atom] = reflect(points[atom], start, end)?.rounded();
      }
    }
    if relation(points, [left, a, b, right]) != Some(expected) {
      return Err(Error::DoubleBondStereo(model.edges[edge].id));
    }
  }
  for &(atoms, expected) in &model.stereo {
    if relation(points, atoms) != Some(expected) {
      let edge = model.neighbors[atoms[1]]
        .iter()
        .find_map(|&(n, edge)| (n == atoms[2]).then_some(edge))
        .ok_or(Error::CoordinateRange)?;
      return Err(Error::DoubleBondStereo(model.edges[edge].id));
    }
  }
  Ok(())
}

/// Checks supplied geometry independently of force energies and search penalties.
pub(super) fn supplied_relations_satisfied(model: &Model, points: &[Vector]) -> bool {
  model
    .stereo
    .iter()
    .all(|&(atoms, expected)| relation(points, atoms) == Some(expected))
}

fn relation(points: &[Vector], [left, a, b, right]: [usize; 4]) -> Option<bool> {
  let positions = [a, b, left, right].map(|atom| {
    let p = points[atom];
    crate::geometry::Point::new(f64::from(p.x), f64::from(p.y))
  });
  crate::stereo_geometry::relation(positions).map(|side| side == chitin_chem::DoubleBondRelation::SameSide)
}

fn reflect(point: Vector, start: Vector, end: Vector) -> Result<Vector, Error> {
  let axis = end - start;
  if axis.square() < EPSILON {
    return Err(Error::CoordinateRange);
  }
  let delta = point - start;
  let parallel = (axis * delta.dot(axis)) / axis.square();
  Ok(start + parallel - (delta - parallel))
}
