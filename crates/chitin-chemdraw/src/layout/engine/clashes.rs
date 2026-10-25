//! Discrete assembly penalties, separate from differentiable force interactions.
//! Source attribution and redistribution terms are in licenses/coordinate-generation.txt.

use super::{
  Error, Work,
  fragments::Forest,
  math::{BOND, EPSILON, Vector},
  topology::Model,
};

/// Scores crossings and foreign-fragment occupancy of small ring centers.
///
/// These terms request discrete search or continuous cleanup; they are not
/// artificial force gradients. Rigid template crossings still contribute here.
///
/// # Errors
/// Exhausted pairwise inspection allowance rejects the complete evaluation.
pub(super) fn score(model: &Model, forest: &Forest, points: &[Vector], work: &mut Work) -> Result<f32, Error> {
  let mut total = 0.;
  work.charge(points.len())?;
  let mut component = vec![0; points.len()];
  for (id, nodes) in model.components.iter().enumerate() {
    for &a in nodes {
      component[a] = id;
    }
  }
  for (i, first) in model.edges.iter().enumerate() {
    for second in &model.edges[i + 1..] {
      work.charge(1)?;
      if component[first.ends[0]] != component[second.ends[0]] || first.ends.iter().any(|a| second.ends.contains(a)) {
        continue;
      }
      let [a, b] = first.ends.map(|i| points[i]);
      let [c, d] = second.ends.map(|i| points[i]);
      if !intersects([a, b, c, d]) {
        continue;
      }
      let mut penalty = 2500.;
      if first
        .ends
        .iter()
        .chain(&second.ends)
        .any(|&a| model.neighbors[a].len() == 1)
      {
        penalty *= 0.5;
      }
      let contains_edge = |ring: &Vec<usize>, ends: [usize; 2]| ends.iter().all(|a| ring.contains(a));
      if model
        .cycles
        .iter()
        .any(|ring| ring.len() >= 9 && (contains_edge(ring, first.ends) || contains_edge(ring, second.ends)))
      {
        penalty *= 8.;
      }
      if model
        .cycles
        .iter()
        .any(|ring| ring.len() < 9 && (contains_edge(ring, first.ends) || contains_edge(ring, second.ends)))
      {
        penalty *= 2.;
      }
      total += penalty;
    }
  }
  for ring in model.cycles.iter().filter(|ring| ring.len() <= 9) {
    work.charge(ring.len())?;
    let center = ring.iter().fold(Vector::default(), |sum, &a| sum + points[a]) / ring.len() as f32;
    for (a, &point) in points.iter().enumerate() {
      work.charge(1)?;
      if forest.owner[a] == forest.owner[ring[0]] {
        continue;
      }
      // Disconnected components are simulated and evaluated independently.
      if component[a] != component[ring[0]] {
        continue;
      }
      let distance = (center - point).length();
      if distance < BOND {
        total += 50. + 100. * (1. - distance / BOND);
      }
    }
  }
  Ok(total)
}

/// Preserves the source segment convention: parallel overlap is not an intersection.
pub(super) fn intersects([a, b, c, d]: [Vector; 4]) -> bool {
  if a.x.max(b.x) < c.x.min(d.x)
    || a.y.max(b.y) < c.y.min(d.y)
    || a.x.min(b.x) > c.x.max(d.x)
    || a.y.min(b.y) > c.y.max(d.y)
  {
    return false;
  }
  if [a, b]
    .iter()
    .any(|&p| [c, d].iter().any(|&q| (p - q).square() < EPSILON * EPSILON))
  {
    return true;
  }
  let cross = |p: Vector, q: Vector| p.x * q.y - p.y * q.x;
  let r = b - a;
  let s = d - c;
  let denominator = cross(r, s);
  if denominator.abs() < EPSILON {
    return false;
  }
  let t = cross(c - a, s) / denominator;
  let u = cross(c - a, r) / denominator;
  (0. ..=1.).contains(&t) && (0. ..=1.).contains(&u)
}
