//! Terminal-stroke shortening before required continuous clash relaxation.
//! Source attribution and redistribution terms are in licenses/coordinate-generation.txt.

use super::{Error, Work, clashes::intersects, math::Vector, topology::Model};

/// Shortens colliding terminal strokes in source bond order.
///
/// # Errors
/// Pair enumeration is bounded. Scratch coordinates are committed only after
/// every required terminal inspection succeeds.
pub(super) fn terminals(model: &Model, points: &mut [Vector], work: &mut Work) -> Result<(), Error> {
  work.charge(points.len())?;
  let mut scratch = points.to_vec();
  let mut component = vec![0; model.atoms.len()];
  for (id, nodes) in model.components.iter().enumerate() {
    for &a in nodes {
      component[a] = id;
    }
  }
  for edge in &model.edges {
    let [a, b] = edge.ends;
    let (terminal, root) = if model.neighbors[b].len() == 1 {
      (b, a)
    } else if model.neighbors[a].len() == 1 {
      (a, b)
    } else {
      continue;
    };
    for other in &model.edges {
      work.charge(1)?;
      if component[a] != component[other.ends[0]] || edge.ends.iter().any(|a| other.ends.contains(a)) {
        continue;
      }
      let [c, d] = other.ends;
      if intersects([scratch[a], scratch[b], scratch[c], scratch[d]]) {
        scratch[terminal] = (scratch[root] + (scratch[terminal] - scratch[root]) * 0.1).rounded();
      }
    }
  }
  points.copy_from_slice(&scratch);
  Ok(())
}
