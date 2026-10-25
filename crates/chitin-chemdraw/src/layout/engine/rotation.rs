//! Whole-component drawing orientation, independent of conformational search.
//! Source attribution and redistribution terms are in licenses/coordinate-generation.txt.

use crate::layout::{NativeFailureKind as Failure, NativeStage as Stage};

use super::{
  Error, Work,
  fragments::{Forest, Fragment},
  math::{EPSILON, Vector, quantize},
  topology::Model,
};
use std::f32::consts::{FRAC_PI_6, PI};

/// A quantized direction histogram retains insertion order for equal-weight ties.
#[derive(Default)]
struct Votes(Vec<(f32, f32)>);

impl Votes {
  /// Adds an unoriented axis; zero and pi are merged only after all votes arrive.
  fn add(&mut self, weight: f32, angle: f32, work: &mut Work) -> Result<(), Error> {
    work.charge(1 + self.0.len())?;
    let mut angle = quantize(angle);
    while angle <= 0. {
      angle += PI;
    }
    let slot = self
      .0
      .iter()
      .position(|&(_, old)| old >= angle - EPSILON)
      .unwrap_or(self.0.len());
    if let Some((sum, old)) = self.0.get_mut(slot)
      && (*old - angle).abs() < EPSILON
    {
      *sum += weight;
    } else {
      self.0.insert(slot, (weight, angle));
    }
    Ok(())
  }

  fn best(mut self) -> Option<f32> {
    if self.0.len() > 1
      && f64::from(self.0[self.0.len() - 1].1 - self.0[0].1) >= std::f64::consts::PI - 2. * f64::from(EPSILON)
    {
      let last = self.0.pop()?;
      self.0[0].0 += last.0;
    }
    let mut best = 0;
    for i in 1..self.0.len() {
      if self.0[i].0 > self.0[best].0 {
        best = i;
      }
    }
    self.0.get(best).map(|&(_, angle)| angle)
  }
}

/// Orients each connected component without altering topology or source stereo.
///
/// # Parameters
/// - `points`: complete fifty-unit drawing coordinates, before component packing.
/// - `work`: shared allowance for votes, fusion inspection and coordinate updates.
///
/// # Errors
/// Work exhaustion or nonfinite transformed geometry rejects the enclosing layout.
pub(super) fn apply(model: &Model, forest: &Forest, points: &mut [Vector], work: &mut Work) -> Result<(), Error> {
  let backbones = backbones(model, work)?;
  let mut cyclic = vec![false; model.atoms.len()];
  for &atom in model.cycles.iter().flatten() {
    cyclic[atom] = true;
  }
  for component in &model.components {
    if component.is_empty() {
      continue;
    }
    let mut member = vec![false; model.atoms.len()];
    for &atom in component {
      member[atom] = true;
    }
    let mut votes = Votes::default();
    for &[n, c] in &backbones {
      if member[n] {
        votes.add(1000., bearing(points[n] - points[c]), work)?;
      }
    }
    for &atom in component.iter().filter(|&&a| !cyclic[a]) {
      let neighbors = &model.neighbors[atom];
      for (i, &(a, _)) in neighbors.iter().enumerate() {
        for &(b, _) in &neighbors[i + 1..] {
          let weight = 6.+2.*f32::from(model.neighbors[a].len()!=1)
            +2.*f32::from(model.neighbors[b].len()!=1)
            // The paired carbon preference belongs to the ordered second neighbor.
            +2.*f32::from(model.atoms[b].number==6)
            +f32::from(model.atoms[a].charge==0)+f32::from(model.atoms[b].charge==0);
          votes.add(weight, bearing(points[a] - points[b]), work)?;
        }
      }
    }
    for (id, edge) in model.edges.iter().enumerate().filter(|(_, edge)| member[edge.ends[0]]) {
      let [a, b] = super::orientation::oriented(id, model, forest);
      let leaves = [a, b].map(|a| model.neighbors[a].len() == 1);
      let mut angle = quantize(bearing(points[b] - points[a]));
      while angle <= 0. {
        angle += PI;
      }
      for i in 0..6 {
        let mut weight = match i {
          1 | 5 => 5.,
          0 | 3 => 1.5,
          _ => 1.,
        };
        if edge.order == 2 && i == 3 && (leaves[0] || leaves[1]) {
          weight += 1.5;
        }
        if leaves[0] && leaves[1] && i == 0 {
          weight += 10.;
        }
        votes.add(weight, angle, work)?;
        angle += FRAC_PI_6;
        if f64::from(angle) > std::f64::consts::PI {
          angle -= PI;
        }
      }
    }
    for &id in forest.order.iter().filter(|&&id| member[forest.fragments[id].atoms[0]]) {
      ring_votes(model, &forest.fragments[id], points, &mut votes, work)?;
    }
    if let Some(angle) = votes.best() {
      let center = center(component, points);
      work.charge(component.len())?;
      for &atom in component {
        points[atom] = (center + (points[atom] - center).rotate(-angle.sin(), angle.cos())).rounded();
      }
    }
    reflect(model, forest, component, &member, &backbones, points, work)?;
  }
  if points.iter().all(|p| p.finite()) {
    Ok(())
  } else {
    Err(Error::native(Stage::Orientation, Failure::NonFiniteCoordinates))
  }
}

fn bearing(ray: Vector) -> f32 {
  (-ray.y).atan2(ray.x)
}

/// The native drawing heuristic identifies N-C-alpha-C(=O) paths, not amino-acid facts.
fn backbones(model: &Model, work: &mut Work) -> Result<Vec<[usize; 2]>, Error> {
  work.charge(model.atoms.len() + 2 * model.edges.len())?;
  let carbonyl: Vec<_> = model
    .atoms
    .iter()
    .enumerate()
    .map(|(a, atom)| {
      atom.number == 6
        && model.neighbors[a]
          .iter()
          .any(|&(n, e)| model.atoms[n].number == 8 && model.edges[e].order == 2)
    })
    .collect();
  let mut result = Vec::new();
  for (a, atom) in model.atoms.iter().enumerate() {
    if atom.number != 6 || carbonyl[a] {
      continue;
    }
    let (mut nitrogen, mut carbon) = (None, None);
    for &(n, _) in &model.neighbors[a] {
      if model.atoms[n].number == 7 {
        nitrogen = Some(n);
      } else if carbonyl[n] {
        carbon = Some(n);
      }
    }
    if let (Some(n), Some(c)) = (nitrogen, carbon) {
      result.push([n, c]);
    }
  }
  Ok(result)
}

/// Ring storage is source-atom order, even though construction traverses its perimeter.
fn center(nodes: &[usize], points: &[Vector]) -> Vector {
  let mut ordered = nodes.to_vec();
  ordered.sort_unstable();
  ordered.iter().fold(Vector::default(), |sum, &a| sum + points[a]) / ordered.len() as f32
}

fn shared(model: &Model, a: usize, b: usize) -> Vec<usize> {
  let mut nodes: Vec<_> = model.cycles[a]
    .iter()
    .copied()
    .filter(|a| model.cycles[b].contains(a))
    .collect();
  nodes.sort_unstable();
  nodes
}

fn ring_votes(
  model: &Model,
  fragment: &Fragment,
  points: &[Vector],
  votes: &mut Votes,
  work: &mut Work,
) -> Result<(), Error> {
  let rings = &fragment.rings;
  if rings.is_empty() {
    return Ok(());
  }
  work.charge(rings.iter().map(|&r| model.cycles[r].len()).sum())?;
  if rings.len() == 2 {
    let ray = (center(&model.cycles[rings[1]], points) - center(&model.cycles[rings[0]], points)).normalized();
    votes.add(25., bearing(ray), work)?;
  } else if rings.len() == 3 {
    let mut origins = [
      center(&model.cycles[rings[0]], points),
      center(&model.cycles[rings[1]], points),
    ];
    let neighbors = super::rings::fused_neighbors(model, fragment, work)?;
    for (i, partners) in neighbors.iter().enumerate() {
      if partners.len() != 2 {
        continue;
      }
      let a = shared(model, rings[i], rings[partners[0]]);
      let b = shared(model, rings[i], rings[partners[1]]);
      if a.len() == 2 && b.len() == 2 {
        origins = [(points[a[0]] + points[a[1]]) * 0.5, (points[b[0]] + points[b[1]]) * 0.5];
        break;
      }
    }
    votes.add(50., bearing(origins[1] - origins[0]), work)?;
  } else {
    let neighbors = super::rings::fused_neighbors(model, fragment, work)?;
    for (i, partners) in neighbors
      .iter()
      .enumerate()
      .filter(|&(i, _)| model.cycles[rings[i]].len() == 6)
    {
      for &other in partners {
        let atoms = shared(model, rings[i], rings[other]);
        if atoms.len() == 2 {
          let angle = (f64::from(bearing(points[atoms[0]] - points[atoms[1]])) - std::f64::consts::PI * 0.5) as f32;
          votes.add(25., angle, work)?;
        }
      }
    }
  }
  Ok(())
}

fn sign(value: f32) -> f32 {
  if value > EPSILON {
    1.
  } else if value < -EPSILON {
    -1.
  } else {
    0.
  }
}

/// Chooses display reflections; tetrahedral wedge assignment remains downstream.
fn reflect(
  model: &Model,
  forest: &Forest,
  nodes: &[usize],
  member: &[bool],
  backbones: &[[usize; 2]],
  points: &mut [Vector],
  work: &mut Work,
) -> Result<(), Error> {
  if nodes.len() < 2 {
    return Ok(());
  }
  let (mut x, mut y) = (0., 0.);
  for &[n, c] in backbones.iter().filter(|&&[n, _]| member[n]) {
    x += if points[n].x - points[c].x > 0. { -100. } else { 100. };
  }
  for &id in forest.order.iter().filter(|&&id| member[forest.fragments[id].atoms[0]]) {
    let rings = &forest.fragments[id].rings;
    work.charge(rings.iter().map(|&r| model.cycles[r].len()).sum())?;
    if rings.len() == 2 {
      let (mut a, mut b) = (rings[0], rings[1]);
      if center(&model.cycles[a], points).x - center(&model.cycles[b], points).x > EPSILON {
        std::mem::swap(&mut a, &mut b);
      }
      let benzene = |ring: usize| {
        let nodes = &model.cycles[ring];
        nodes.len() == 6
          && nodes
            .iter()
            .all(|&a| model.atoms[a].number == 6 && model.neighbors[a].iter().any(|&(_, e)| model.edges[e].order == 2))
      };
      match (benzene(a), benzene(b)) {
        (false, true) => x -= 20.,
        (true, false) => x += 20.,
        _ => {}
      }
    }
    if rings.len() > 3 {
      let (mut total, mut weighted) = (Vector::default(), Vector::default());
      let (mut count, mut atoms) = (0, 0);
      for &ring in rings.iter().filter(|&&r| model.cycles[r].len() >= 4) {
        let n = model.cycles[ring].len();
        let c = center(&model.cycles[ring], points);
        total += c;
        weighted += c * n as f32;
        count += 1;
        atoms += n;
      }
      if count > 0 {
        let delta = weighted / atoms as f32 - total / count as f32;
        x -= 50. * sign(delta.x);
        y -= 50. * sign(delta.y);
      }
    }
  }
  let centroid = center(nodes, points);
  let (mut min, mut max) = (Vector::new(9999., 9999.), Vector::new(-9999., -9999.));
  work.charge(nodes.len() + model.edges.len())?;
  for &a in nodes {
    let p = points[a];
    // Ordered extrema preserve the native tie behavior used by display flipping.
    if p.x < min.x {
      min.x = p.x;
    } else if p.x > max.x {
      max.x = p.x;
    }
    if p.y < min.y {
      min.y = p.y;
    } else if p.y > max.y {
      max.y = p.y;
    }
  }
  let delta = (min + max) * 0.5 - centroid;
  x -= 0.5 * sign(delta.x);
  y += 0.5 * sign(delta.y);
  for edge in model
    .edges
    .iter()
    .filter(|edge| edge.order == 2 && member[edge.ends[0]])
  {
    let [a, b] = edge.ends;
    if model.neighbors[a].len() == 1 && model.neighbors[b].len() > 1 {
      y += sign(points[a].y - points[b].y);
    } else if model.neighbors[b].len() == 1 && model.neighbors[a].len() > 1 {
      y += sign(points[b].y - points[a].y);
    }
  }
  for &a in nodes {
    if x < 0. {
      points[a].x = -points[a].x;
    }
    if y < 0. {
      points[a].y = -points[a].y;
    }
  }
  Ok(())
}
