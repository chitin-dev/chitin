//! Bounded planar embedding of bicyclic blocks with three bridgehead paths.
//!
//! Nonregular arcs replace incompatible regular faces, but every skeleton edge
//! remains unit length. This is drawing geometry, not a 3D conformation or a
//! stereochemical assignment. Cages and degenerate path families stay explicit
//! unsupported cases rather than being flattened onto coincident atoms.

use std::{
  collections::{BTreeMap, BTreeSet},
  f64::consts::{PI, TAU},
};

use chitin_chem::{AtomId, MolecularGraph, perception::RingBlock};

use super::{Error, Point, Work, score};

/// Three complete, internally disjoint bridgehead-to-bridgehead paths.
pub(super) struct Bridged {
  /// Global dense rows, ordered by length then stable atom identity.
  paths: [Vec<usize>; 3],
}

impl Bridged {
  /// Recognizes a theta block without classifying external substituents as paths.
  ///
  /// # Returns
  /// `None` for other topology, including ordinary edge-fused bicyclic blocks.
  /// Only block edges participate; atom handles are mapped to layout rows once.
  ///
  /// # Errors
  /// Propagates graph inconsistencies or shared ring-work budget exhaustion.
  pub(super) fn new(
    graph: &MolecularGraph,
    block: &RingBlock,
    indices: &BTreeMap<AtomId, usize>,
    work: &mut Work,
  ) -> Result<Option<Self>, Error> {
    if block.cycle_rank() != 2 {
      return Ok(None);
    }
    work.charge(block.atoms().len() + block.bonds().len())?;
    let mut rows = BTreeMap::<usize, Vec<usize>>::new();
    for &edge in block.bonds() {
      let [a, b] = graph
        .bond(edge)
        .ok_or(chitin_chem::GraphError::MissingBond(edge))?
        .atoms();
      let (a, b) = (indices[&a], indices[&b]);
      rows.entry(a).or_default().push(b);
      rows.entry(b).or_default().push(a);
    }
    let heads: Vec<_> = rows
      .iter()
      .filter(|(_, row)| row.len() == 3)
      .map(|(&node, _)| node)
      .collect();
    if heads.len() != 2 || rows.values().any(|row| !matches!(row.len(), 2 | 3)) {
      return Ok(None);
    }
    let (start, end) = (heads[0], heads[1]);
    let mut paths = Vec::new();
    let mut visited = BTreeSet::from([start, end]);
    for &neighbor in &rows[&start] {
      let mut path = vec![start];
      let (mut previous, mut node) = (start, neighbor);
      while node != end {
        work.charge(1)?;
        if !visited.insert(node) {
          return Err(Error::Invariant(block.atoms()[0]));
        }
        path.push(node);
        let next = rows[&node]
          .iter()
          .copied()
          .find(|&other| other != previous)
          .ok_or(Error::Invariant(block.atoms()[0]))?;
        previous = node;
        node = next;
      }
      path.push(end);
      paths.push(path);
    }
    paths.sort_by(|a, b| a.len().cmp(&b.len()).then(a.cmp(b)));
    if paths[0].len() == 2 {
      return Ok(None);
    }
    if visited.len() != block.atoms().len() {
      return Err(Error::Invariant(block.atoms()[0]));
    }
    let paths = paths.try_into().map_err(|_| Error::Invariant(block.atoms()[0]))?;
    Ok(Some(Self { paths }))
  }

  /// Returns the outer walk's vertex count for stable system seeding priority.
  pub(super) fn perimeter_len(&self) -> usize {
    self.paths[1].len() + self.paths[2].len() - 2
  }

  /// Places two outer arcs and a strictly shorter interior bridge atomically.
  ///
  /// # Parameters
  /// - `atoms`, `old`, `component`: shared global dense namespace and fixed anchors.
  /// - `heading`: direction into the new block when entering through an articulation.
  /// - `work`: shared discovery/geometry budget, including candidate snapshots.
  /// - `source`: original block locator for an unsupported result.
  ///
  /// # Returns
  /// Complete positions and attachment headings. Three bridgehead separations,
  /// two interior sides and six rigid orientations are scored deterministically.
  /// All arcs close on the same two endpoints with unit chords; known anchors
  /// are translated exactly, never relaxed or overwritten.
  ///
  /// # Errors
  /// Equal shortest paths currently need a more general flexible-chain solver;
  /// they are rejected rather than superimposed. Multiple fixed block anchors,
  /// inconsistent geometry and exhausted work also return explicit errors.
  pub(super) fn embed(
    &self,
    atoms: &[usize],
    old: &[Option<Point>],
    component: &[usize],
    heading: f64,
    work: &mut Work,
    source: AtomId,
  ) -> Result<Vec<(usize, Point, f64)>, Error> {
    let unsupported = || Error::RingSystem {
      atom: source,
      cycles: 2,
    };
    if self.paths[0].len() == self.paths[1].len() {
      return Err(unsupported());
    }
    work.charge(atoms.len())?;
    let anchors: Vec<_> = atoms.iter().copied().filter(|&node| old[node].is_some()).collect();
    if anchors.len() > 1 {
      return Err(unsupported());
    }
    let mut best = None;
    let mut best_cost = f64::INFINITY;
    for ratio in [0.85, 0.70, 0.95] {
      let distance = (self.paths[0].len() - 1) as f64 * ratio;
      for interior_side in [1., -1.] {
        let mut local = BTreeMap::new();
        for (i, path) in self.paths.iter().enumerate() {
          let sign = if i == 0 {
            interior_side
          } else if i == 1 {
            1.
          } else {
            -1.
          };
          let arc = arc(path.len() - 1, distance, sign, work)?;
          for (&node, point) in path.iter().zip(arc) {
            local.insert(node, point);
          }
        }
        let anchor = anchors.first().copied();
        let origin = anchor.map_or(Point::new(0., 0.), |node| local[&node]);
        let target = anchor.and_then(|node| old[node]).unwrap_or(Point::new(0., 0.));
        let center = Point::new(
          local.values().map(|p| p.x).sum::<f64>() / local.len() as f64,
          local.values().map(|p| p.y).sum::<f64>() / local.len() as f64,
        );
        let base = if anchor.is_some() {
          heading - (center.y - origin.y).atan2(center.x - origin.x)
        } else {
          0.
        };
        for rotation in [0., PI / 3., -PI / 3., 2. * PI / 3., -2. * PI / 3., PI] {
          work.charge(old.len() + atoms.len())?;
          let angle = base + rotation;
          let (sin, cos) = angle.sin_cos();
          let mut points = old.to_vec();
          for (&node, point) in &local {
            let (x, y) = (point.x - origin.x, point.y - origin.y);
            points[node] = Some(old[node].unwrap_or(target.translated(x * cos - y * sin, x * sin + y * cos)));
          }
          let cost = score(atoms, &points, old, component, work)?;
          if cost + 1e-10 < best_cost {
            best_cost = cost;
            best = Some(points);
          }
        }
      }
    }
    let points = best.ok_or_else(unsupported)?;
    self.validate(&points, source, work)?;
    atoms
      .iter()
      .map(|&node| {
        Ok((
          node,
          points[node].ok_or(Error::Invariant(source))?,
          self.outward(node, &points, source, work)?,
        ))
      })
      .collect()
  }

  /// Verifies all path closures and unit edges before any coordinates escape.
  fn validate(&self, points: &[Option<Point>], source: AtomId, work: &mut Work) -> Result<(), Error> {
    for path in &self.paths {
      work.charge(path.len())?;
      for edge in path.windows(2) {
        let a = points[edge[0]].ok_or(Error::Invariant(source))?;
        let b = points[edge[1]].ok_or(Error::Invariant(source))?;
        if !a.is_finite() || !b.is_finite() || (a.distance(b) - 1.).abs() > 1e-8 {
          return Err(Error::RingSystem {
            atom: source,
            cycles: 2,
          });
        }
      }
    }
    Ok(())
  }

  /// Uses actual incident bond rays, not regular-face interior angles.
  fn outward(&self, node: usize, points: &[Option<Point>], source: AtomId, work: &mut Work) -> Result<f64, Error> {
    let origin = points[node].ok_or(Error::Invariant(source))?;
    let mut neighbors = BTreeSet::new();
    for path in &self.paths {
      work.charge(path.len())?;
      for edge in path.windows(2) {
        if edge[0] == node {
          neighbors.insert(edge[1]);
        }
        if edge[1] == node {
          neighbors.insert(edge[0]);
        }
      }
    }
    let mut angles = neighbors
      .into_iter()
      .map(|other| {
        let p = points[other].ok_or(Error::Invariant(source))?;
        Ok((p.y - origin.y).atan2(p.x - origin.x))
      })
      .collect::<Result<Vec<_>, Error>>()?;
    angles.sort_by(f64::total_cmp);
    let mut best = (0., 0.);
    for i in 0..angles.len() {
      let gap = (angles[(i + 1) % angles.len()] - angles[i]).rem_euclid(TAU);
      if gap > best.0 + 1e-10 {
        best = (gap, angles[i] + gap / 2.);
      }
    }
    Ok(best.1)
  }
}

/// Solves a circular chain with `edges` unit chords and a prescribed endpoint gap.
///
/// For total angle in (0, 2π), chord/span is monotone from `edges` to zero.
/// Fixed-count bisection bounds work and avoids a stochastic relaxation. Endpoint
/// values are installed exactly to keep independently generated paths identical.
fn arc(edges: usize, distance: f64, sign: f64, work: &mut Work) -> Result<Vec<Point>, Error> {
  work.charge(64 + edges)?;
  let (mut low, mut high) = (0., TAU / edges as f64);
  for _ in 0..64 {
    let angle = (low + high) / 2.;
    let span = (edges as f64 * angle / 2.).sin() / (angle / 2.).sin();
    if span > distance {
      low = angle;
    } else {
      high = angle;
    }
  }
  let step = (low + high) / 2.;
  let radius = 1. / (2. * (step / 2.).sin());
  let half = edges as f64 * step / 2.;
  let mut result = Vec::with_capacity(edges + 1);
  result.push(Point::new(0., 0.));
  for i in 1..edges {
    let angle = half - i as f64 * step;
    result.push(Point::new(
      distance / 2. - radius * angle.sin(),
      sign * radius * (angle.cos() - half.cos()),
    ));
  }
  result.push(Point::new(distance, 0.));
  Ok(result)
}
