//! Dense molecular constraints for topology-free macrocycle contour matching.
//! Source attribution and redistribution terms are in licenses/coordinate-generation.txt.

use super::contours::{Rules as ContourRules, select as select_contour};
use super::{
  Error, Work,
  math::{BOND, Vector},
  topology::Model,
};
use std::collections::VecDeque;

/// Builds a constrained contour without invoking a compatibility coordinate engine.
///
/// # Parameters
/// - `model`, `ring`: immutable dense source topology and one ordered cycle.
/// - `work`: shared traversal and shape-matching allowance.
///
/// # Returns
/// Complete ring coordinates, or `None` when constraints need the open-ring
/// construction path. Odd closures require downstream force relaxation.
///
/// # Errors
/// Inconsistent ring edges, attachment-size overflow and exhausted work propagate.
pub(super) fn generate(model: &Model, ring: &[usize], work: &mut Work) -> Result<Option<Vec<Vector>>, Error> {
  let n = ring.len();
  let mut rules = ContourRules {
    hetero: ring.iter().map(|&a| model.atoms[a].number != 6).collect(),
    ..Default::default()
  };
  for (i, &atom) in ring.iter().enumerate() {
    work.charge(model.neighbors[atom].len() + model.cycles.iter().map(Vec::len).sum::<usize>())?;
    let previous = ring[(i + n - 1) % n];
    let next = ring[(i + 1) % n];
    if model.neighbors[atom].len() != 2 {
      let mut count = 0_usize;
      for &(branch, _) in &model.neighbors[atom] {
        if branch != previous && branch != next {
          count = count
            .checked_add(children(model, branch, atom, work)?)
            .ok_or(Error::CoordinateRange)?;
        }
      }
      rules.attachments.push((i, count));
    }
    for (face, nodes) in model
      .cycles
      .iter()
      .enumerate()
      .filter(|(_, nodes)| nodes.len() < 9 && nodes.contains(&atom))
    {
      // Only the first external neighbor determines the source face constraint.
      let outside = model.neighbors[atom].iter().find(|&&(a, _)| !ring.contains(&a));
      let force = outside.is_some_and(|&(a, _)| nodes.contains(&a));
      rules.fusions.push((i, face, force));
    }
    let edge = model.neighbors[atom]
      .iter()
      .find(|&&(a, _)| a == next)
      .map(|&(_, edge)| edge)
      .ok_or(Error::Invariant(model.atoms[atom].id))?;
    if model.edges[edge].order != 2
      || model
        .cycles
        .iter()
        .any(|nodes| nodes.len() < 9 && nodes.contains(&atom) && nodes.contains(&next))
    {
      continue;
    }
    let following = ring[(i + 2) % n];
    if let Some(&(slots, same)) = model
      .stereo
      .iter()
      .find(|(slots, _)| (slots[1] == atom && slots[2] == next) || (slots[1] == next && slots[2] == atom))
    {
      let [left, right] = if slots[1] == atom {
        [slots[0], slots[3]]
      } else {
        [slots[3], slots[0]]
      };
      rules.sides.push((
        [(i + n - 1) % n, i, (i + 1) % n, (i + 2) % n],
        same ^ (left != previous) ^ (right != following),
      ));
    } else {
      let a = super::ranking::first_neighbor(model, atom, next, work)?;
      let b = super::ranking::first_neighbor(model, next, atom, work)?;
      // The unspecified drawing convention is E, not a new chemical stereo fact.
      let same = (a != Some(previous)) ^ (b != Some(following));
      rules.sides.push(([(i + n - 1) % n, i, (i + 1) % n, (i + 2) % n], same));
    }
  }
  Ok(select_contour(n, &rules, work)?.map(|path| {
    path
      .into_iter()
      .map(|p| Vector::new((p.x * f64::from(BOND)) as f32, (p.y * f64::from(BOND)) as f32).rounded())
      .collect()
  }))
}

/// Counts unique reachable attachment atoms with the center removed.
fn children(model: &Model, branch: usize, center: usize, work: &mut Work) -> Result<usize, Error> {
  work.charge(model.atoms.len())?;
  let mut visited = vec![false; model.atoms.len()];
  visited[center] = true;
  visited[branch] = true;
  let mut queue = VecDeque::from([branch]);
  let mut count = 0;
  while let Some(atom) = queue.pop_front() {
    work.charge(1 + model.neighbors[atom].len())?;
    count += 1;
    for &(next, _) in &model.neighbors[atom] {
      if !visited[next] {
        visited[next] = true;
        queue.push_back(next);
      }
    }
  }
  Ok(count)
}
