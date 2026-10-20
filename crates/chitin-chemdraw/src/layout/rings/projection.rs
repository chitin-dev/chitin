//! Bounded spatial drawing seeds and projections for compact cage blocks.
//!
//! A distance-geometry seed plus a short relaxation gives a drawing-only 3D shape;
//! a deterministic sphere of view directions is scored in 2D, and the selected
//! projection records rear-bond gaps from the view depth. Branches remain with
//! the ordinary 2D expansion; this module owns only the rigid cage block. Dense
//! cages retain the rigid camera projection and depth-cue both surfaces instead
//! of warping the shell into a planar boundary. Neither seed nor
//! projection is a chemically validated conformer or a stereochemical assignment.

use std::collections::BTreeMap;

use chitin_chem::{AtomId, BondOrder, MolecularGraph, perception::RingBlock};

use super::{BondCrossing, Error, Placement, Point, Work};
use crate::layout::quality;
use std::f64::consts::PI;

/// Largest cage accepted by this model, in block atoms.
const MAX_ATOMS: usize = 120;
/// Deterministic sphere samples tried for one conformer.
const VIEWPOINTS: usize = 192;
/// Distinct projections retained as block variants.
const POSES: usize = 4;
/// Relaxation sweeps applied to the 3D seed.
const RELAX: usize = 160;
mod polyhedron;
mod spatial;

/// One selected projection and its authored rear-bond gaps.
struct Pose {
  /// Block-local 2D coordinates.
  points: Vec<Point>,
  /// Proper crossings ordered by their interpolated camera depth.
  crossings: Vec<BondCrossing>,
  /// Depth in the same uniformly scaled camera frame as the projected points.
  depths: Vec<f64>,
}

/// A compact cage embedded through spatial or face-backed projection.
pub(super) struct Projection {
  /// Global layout rows in stable source order.
  atoms: Vec<usize>,
  /// Block-local skeleton edges.
  edges: Vec<[usize; 2]>,
  /// Selected viewpoints, best first.
  poses: Vec<Pose>,
}

impl Projection {
  /// Recognizes a rigid cage that no planar polygon family can own.
  ///
  /// # Parameters
  /// - `graph`, `block`, `indices`: one source namespace and its dense rows.
  /// - `work`: shared ring allowance for the trial.
  ///
  /// # Returns
  /// `None` for low-connectivity or oversized blocks, cyclic E/Z constraints,
  /// and failed projection validation. Existing cage templates and bridges
  /// keep priority. Planar boundary rings retain their polygon/folded models.
  ///
  /// # Errors
  /// Propagates graph inconsistencies and work exhaustion before any pose exists.
  pub(super) fn new(
    graph: &MolecularGraph,
    block: &RingBlock,
    indices: &BTreeMap<AtomId, usize>,
    work: &mut Work,
  ) -> Result<Option<Self>, Error> {
    if block.cycle_rank() < 3 || !(6..=MAX_ATOMS).contains(&block.atoms().len()) {
      return Ok(None);
    }
    if graph
      .double_bond_stereos()
      .any(|(_, stereo)| block.atoms().contains(&stereo.ends()[0]))
    {
      return Ok(None);
    }
    let n = block.atoms().len();
    let local: BTreeMap<_, _> = block.atoms().iter().enumerate().map(|(i, &id)| (id, i)).collect();
    let mut rows = vec![Vec::new(); n];
    let mut edges = Vec::new();
    let mut bonds = Vec::new();
    let mut orders = Vec::new();
    for &id in block.bonds() {
      let bond = graph.bond(id).ok_or(chitin_chem::GraphError::MissingBond(id))?;
      let [a, b] = bond.atoms();
      let (a, b) = (
        local.get(&a).copied().ok_or(chitin_chem::GraphError::MissingAtom(a))?,
        local.get(&b).copied().ok_or(chitin_chem::GraphError::MissingAtom(b))?,
      );
      rows[a].push(b);
      rows[b].push(a);
      edges.push([a.min(b), a.max(b)]);
      bonds.push(id);
      orders.push(bond.order());
    }
    // Sequential relaxation must not depend on bond allocation order or stored
    // endpoint direction. Keep IDs attached while canonicalizing dense edges.
    let mut records: Vec<_> = edges
      .into_iter()
      .zip(bonds)
      .zip(orders)
      .map(|((ends, id), order)| (ends, id, order))
      .collect();
    records.sort_by_key(|record| record.0);
    let edges: Vec<_> = records.iter().map(|record| record.0).collect();
    let bonds: Vec<_> = records.iter().map(|record| record.1).collect();
    let orders: Vec<_> = records.iter().map(|record| record.2).collect();
    for row in &mut rows {
      row.sort_unstable();
    }
    // Integer single/double bonds keep their source semantics in depiction.
    // Explicit E/Z is rejected above, and multiple-bond crossings are never
    // converted to single-bond gaps. Low-degree cycles retain planar/folded models.
    let min_degree = rows.iter().map(Vec::len).min().unwrap_or(0);
    if orders
      .iter()
      .any(|order| !matches!(order, BondOrder::Single | BondOrder::Double))
      || min_degree < 3
      || has_bridge(&rows, &edges, work)?
    {
      return Ok(None);
    }
    work.charge(block.atoms().len() * 3)?;
    // Tetrahedral centers need three non-collinear projected bonds; a view that
    // stacks two of them cannot carry a wedge or hash later in depiction.
    let tetrahedral: Vec<(usize, Vec<usize>)> = (0..n)
      .filter(|&row| graph.tetrahedral(block.atoms()[row]).is_some())
      .map(|row| (row, rows[row].clone()))
      .collect();
    let Some(points) = conformer(&rows, &edges, &orders, work)? else {
      return Ok(None);
    };
    // Dense shells retain their camera projection. A planar boundary solve must
    // not replace the spatial seed while still being labelled a perspective.
    if n >= 24 {
      // The dense depth-cue path does not yet own tetrahedral depiction. Keep
      // that boundary explicit rather than deriving wedges from camera depth.
      if !tetrahedral.is_empty() {
        return Ok(None);
      }
      return Ok(
        spatial::project(block, &edges, &bonds, &orders, &points, work)?.map(|poses| Self {
          atoms: block.atoms().iter().map(|id| indices[id]).collect(),
          edges,
          poses,
        }),
      );
    }
    let mut candidates = Vec::new();
    for direction in sphere(VIEWPOINTS) {
      if work.remaining() < 20_000 {
        break;
      }
      work.charge(n * 3 + edges.len())?;
      let up = if direction[2].abs() > 0.9 {
        [0., 1., 0.]
      } else {
        [0., 0., 1.]
      };
      let right = normalize(cross(up, direction))?;
      let up = cross(direction, right);
      let projected: Vec<Point> = points
        .iter()
        .map(|point| Point::new(dot(*point, right), dot(*point, up)))
        .collect();
      if tetrahedral
        .iter()
        .any(|(center, neighbors)| degenerate(&projected, *center, neighbors))
      {
        continue;
      }
      let quality = work.measure(&projected, &edges)?;
      // Multiple bonds remain completely drawn; do not publish an ambiguous
      // perspective that would require cutting either of their two strokes.
      if quality
        .crossings
        .iter()
        .any(|&(a, b)| !single(&orders, a) || !single(&orders, b))
      {
        continue;
      }
      let lengths: Vec<f64> = edges
        .iter()
        .map(|&[a, b]| projected[a].distance(projected[b]))
        .collect();
      // A strongly foreshortened bond cannot be drawn once element labels clip
      // it, so reject viewpoints closer than about half a nominal bond.
      if lengths.iter().any(|&length| length < 0.4) {
        continue;
      }
      let distortion: f64 = lengths.iter().map(|length| (length - 1.).powi(2)).sum();
      let area = area(&projected);
      candidates.push((
        quality.crossings.len(),
        quality.crowds.len(),
        (distortion * 1e6).round() as i64,
        -(area * 1e6).round() as i64,
        projected,
        points.iter().map(|point| dot(*point, direction)).collect::<Vec<_>>(),
        quality,
      ));
    }
    if candidates.is_empty() {
      if let Some(poses) = polyhedron::project(graph, block, &local, &rows, &edges, &points, work)? {
        return Ok(Some(Self {
          atoms: block.atoms().iter().map(|id| indices[id]).collect(),
          edges,
          poses,
        }));
      }
      return Ok(None);
    }
    candidates.sort_by_key(|candidate| (candidate.0, candidate.1, candidate.2, candidate.3));
    // Retain a readable candidate instead of weakening the crowding threshold.
    if candidates[0].1 * 2 > n {
      return Ok(None);
    }
    let mut poses: Vec<Pose> = Vec::new();
    let mut signatures = Vec::new();
    for (_, _, _, _, projected, depth, quality) in candidates {
      let signature = (
        (projected.iter().map(|point| point.x).fold(f64::INFINITY, f64::min) * 1e4).round() as i64,
        (projected.iter().map(|point| point.y).fold(f64::INFINITY, f64::min) * 1e4).round() as i64,
      );
      if signatures.contains(&signature) {
        continue;
      }
      signatures.push(signature);
      let mut crossings = Vec::new();
      for &(i, j) in &quality.crossings {
        let [a, b] = edges[i];
        let [c, d] = edges[j];
        if !single(&orders, i) || !single(&orders, j) {
          continue;
        }
        if [a, b, c, d]
          .iter()
          .any(|&row| graph.tetrahedral(block.atoms()[row]).is_some())
        {
          continue;
        }
        // Compare depths at the actual intersection, not at the bond midpoints.
        let [pa, pb, pc, pd] = [projected[a], projected[b], projected[c], projected[d]];
        let u = [pb.x - pa.x, pb.y - pa.y];
        let v = [pd.x - pc.x, pd.y - pc.y];
        let offset = [pc.x - pa.x, pc.y - pa.y];
        let cross = |a: [f64; 2], b: [f64; 2]| a[0] * b[1] - a[1] * b[0];
        let determinant = cross(u, v);
        let t = cross(offset, v) / determinant;
        let s = cross(offset, u) / determinant;
        let first = depth[a] + t * (depth[b] - depth[a]);
        let second = depth[c] + s * (depth[d] - depth[c]);
        if (first - second).abs() < 1e-9 {
          continue;
        }
        let (front, back) = if first > second {
          (bonds[i], bonds[j])
        } else {
          (bonds[j], bonds[i])
        };
        crossings.push(BondCrossing { front, back });
      }
      poses.push(Pose {
        points: projected,
        crossings,
        depths: Vec::new(),
      });
      if poses.len() >= POSES {
        break;
      }
    }
    if poses.is_empty() {
      return Ok(None);
    }
    Ok(Some(Self {
      atoms: block.atoms().iter().map(|id| indices[id]).collect(),
      edges,
      poses,
    }))
  }

  /// Returns the block atom count for stable system priority.
  pub(super) fn len(&self) -> usize {
    self.atoms.len()
  }

  /// Returns the finite number of selected viewpoints.
  pub(super) fn variants(&self) -> usize {
    self.poses.len()
  }

  /// Places one selected projection and preserves an existing articulation.
  ///
  /// # Errors
  /// Rejects more than one pre-placed anchor, as the rigid projection has no
  /// remaining degrees of freedom, and propagates work exhaustion.
  pub(super) fn embed(
    &self,
    old: &[Option<Point>],
    work: &mut Work,
    source: AtomId,
    rank: usize,
    variant: usize,
  ) -> Result<Placement, Error> {
    work.charge(self.atoms.len())?;
    let anchors: Vec<_> = self
      .atoms
      .iter()
      .enumerate()
      .filter_map(|(i, &row)| old[row].map(|point| (i, point)))
      .collect();
    if anchors.len() > 1 {
      return Err(Error::RingSystem {
        atom: source,
        cycles: rank,
      });
    }
    let mut points = self.poses[variant % self.poses.len()].points.clone();
    if let Some(&(index, target)) = anchors.first() {
      let origin = points[index];
      for point in &mut points {
        *point = point.translated(target.x - origin.x, target.y - origin.y);
      }
      points[index] = target;
    }
    let mut rows = vec![Vec::new(); points.len()];
    for &[a, b] in &self.edges {
      rows[a].push(b);
      rows[b].push(a);
    }
    let atoms = self
      .atoms
      .iter()
      .enumerate()
      .map(|(i, &row)| (row, points[i], heading(&points, &rows, i)))
      .collect();
    Ok(Placement {
      atoms,
      crossings: self.poses[variant % self.poses.len()].crossings.clone(),
      depths: self.poses[variant % self.poses.len()]
        .depths
        .iter()
        .enumerate()
        .map(|(i, &depth)| (self.atoms[i], depth))
        .collect(),
    })
  }
}

/// Reports whether two projected bonds at one center are nearly collinear.
fn degenerate(points: &[Point], center: usize, neighbors: &[usize]) -> bool {
  let origin = points[center];
  let mut angles: Vec<f64> = neighbors
    .iter()
    .map(|&other| (points[other].y - origin.y).atan2(points[other].x - origin.x))
    .collect();
  angles.sort_by(f64::total_cmp);
  for pair in angles.windows(2) {
    if (pair[1] - pair[0]).abs() < 0.35 {
      return true;
    }
  }
  angles.len() > 1 && 2. * PI - (angles[angles.len() - 1] - angles[0]) < 0.35
}

/// Returns the largest free angular sector at one block vertex.
fn heading(points: &[Point], rows: &[Vec<usize>], index: usize) -> f64 {
  let origin = points[index];
  let mut angles: Vec<f64> = rows[index]
    .iter()
    .map(|&other| (points[other].y - origin.y).atan2(points[other].x - origin.x))
    .collect();
  if angles.is_empty() {
    return 0.;
  }
  angles.sort_by(f64::total_cmp);
  let mut gap = (0., 0.);
  for offset in 0..angles.len() {
    let next = (offset + 1) % angles.len();
    let size = (angles[next] - angles[offset]).rem_euclid(2. * PI);
    if size > gap.0 {
      gap = (size, angles[offset] + size / 2.);
    }
  }
  gap.1
}

/// Reports whether every vertex is reachable after deleting one edge.
fn has_bridge(rows: &[Vec<usize>], edges: &[[usize; 2]], work: &mut Work) -> Result<bool, Error> {
  for &[a, b] in edges {
    work.charge(rows.len() + rows.iter().map(Vec::len).sum::<usize>())?;
    if !reachable(rows, a, b, (a, b)) {
      return Ok(true);
    }
  }
  Ok(false)
}

/// Reports whether one row reaches another without the deleted edge.
fn reachable(rows: &[Vec<usize>], start: usize, goal: usize, blocked: (usize, usize)) -> bool {
  let mut seen = vec![false; rows.len()];
  seen[start] = true;
  let mut pending = vec![start];
  while let Some(node) = pending.pop() {
    for &other in &rows[node] {
      if (node == blocked.0 && other == blocked.1) || (node == blocked.1 && other == blocked.0) {
        continue;
      }
      if other == goal {
        return true;
      }
      if !seen[other] {
        seen[other] = true;
        pending.push(other);
      }
    }
  }
  false
}

/// Builds a relaxed 3D seed from complete graph distances.
fn conformer(
  rows: &[Vec<usize>],
  edges: &[[usize; 2]],
  orders: &[BondOrder],
  work: &mut Work,
) -> Result<Option<Vec<[f64; 3]>>, Error> {
  let n = rows.len();
  // Include the dense matrix/vector iterations, not only distance storage.
  work.charge(n * n * (4 + 3 * 96 + 6) + n * (n + edges.len() * 2))?;
  let distances = distances(rows)?;
  let mut means = vec![0.; n];
  let mut total = 0.;
  for (index, row) in distances.iter().enumerate() {
    means[index] = row.iter().map(|value| value * value).sum::<f64>() / n as f64;
    total += means[index];
  }
  let total = total / n as f64;
  let entry = |i: usize, j: usize| -0.5 * (distances[i][j] * distances[i][j] - means[i] - means[j] + total);
  let mut axes: Vec<Vec<f64>> = Vec::new();
  for axis in 0..3 {
    let mut vector: Vec<f64> = (0..n)
      .map(|i| (((i + 1) * (axis + 2)) as f64 * std::f64::consts::GOLDEN_RATIO).sin())
      .collect();
    normalize_vector(&mut vector);
    for _ in 0..96 {
      for previous in &axes {
        deflate(&mut vector, previous);
      }
      let mut next = vec![0.; n];
      for (i, value) in next.iter_mut().enumerate() {
        *value = vector.iter().enumerate().map(|(j, second)| entry(i, j) * second).sum();
      }
      for previous in &axes {
        deflate(&mut next, previous);
      }
      if !normalize_vector(&mut next) {
        break;
      }
      vector = next;
    }
    axes.push(vector);
  }
  let values: Vec<f64> = axes
    .iter()
    .map(|vector| {
      let value: f64 = vector
        .iter()
        .enumerate()
        .map(|(i, first)| {
          first
            * vector
              .iter()
              .enumerate()
              .map(|(j, second)| entry(i, j) * second)
              .sum::<f64>()
        })
        .sum();
      value.max(0.).sqrt()
    })
    .collect();
  // A degenerate spatial seed cannot define a stable camera. Spectral rank
  // here is only a drawing-seed test, not a proof of graph planarity.
  if values[2] <= values[0] * 1e-3 || values[1] <= 1e-12 {
    return Ok(None);
  }
  let mut points: Vec<[f64; 3]> = (0..n)
    .map(|i| [axes[0][i] * values[0], axes[1][i] * values[1], axes[2][i] * values[2]])
    .collect();
  // Keep the global polyhedral shape: scale the MDS seed so its mean edge is
  // one bond and pull every pair weakly toward that target. Edge projection
  // alone relaxes a cage toward a featureless ball.
  let raw_mean = edges.iter().map(|&[a, b]| distance3(points[a], points[b])).sum::<f64>() / edges.len() as f64;
  let scale = if raw_mean > 1e-9 { 1. / raw_mean } else { 1. };
  let targets: Vec<[f64; 3]> = points
    .iter()
    .map(|point| [point[0] * scale, point[1] * scale, point[2] * scale])
    .collect();
  points = targets.clone();
  // Unsaturated centers prefer a trigonal drawing neighborhood. These are soft
  // shape constraints, not inferred conformer geometry for strained small cages.
  let mut angles = Vec::new();
  work.charge(n * edges.len())?;
  for (center, neighbors) in rows.iter().enumerate() {
    let planar = edges
      .iter()
      .zip(orders)
      .any(|(ends, order)| ends.contains(&center) && *order == BondOrder::Double);
    for (i, &a) in neighbors.iter().enumerate() {
      for &b in &neighbors[i + 1..] {
        if rows[a].contains(&b) {
          continue;
        }
        angles.push((a, b, if planar { 3_f64.sqrt() } else { (8_f64 / 3.).sqrt() }));
      }
    }
  }
  for _ in 0..RELAX {
    work.charge(n * n * 2 + edges.len() + angles.len())?;
    for i in 0..n {
      for j in i + 1..n {
        let (dx, dy, dz) = (
          points[j][0] - points[i][0],
          points[j][1] - points[i][1],
          points[j][2] - points[i][2],
        );
        let length = (dx * dx + dy * dy + dz * dz).sqrt();
        let target = distance3(targets[i], targets[j]);
        if length < 1e-9 || target < 1e-9 {
          continue;
        }
        let correction = (length - target) / length * 0.02;
        points[i][0] += dx * correction;
        points[i][1] += dy * correction;
        points[i][2] += dz * correction;
        points[j][0] -= dx * correction;
        points[j][1] -= dy * correction;
        points[j][2] -= dz * correction;
      }
    }
    for &[a, b] in edges {
      let (dx, dy, dz) = (
        points[b][0] - points[a][0],
        points[b][1] - points[a][1],
        points[b][2] - points[a][2],
      );
      let length = (dx * dx + dy * dy + dz * dz).sqrt();
      if length < 1e-9 {
        continue;
      }
      let correction = (length - 1.) / length * 0.5;
      points[a][0] += dx * correction;
      points[a][1] += dy * correction;
      points[a][2] += dz * correction;
      points[b][0] -= dx * correction;
      points[b][1] -= dy * correction;
      points[b][2] -= dz * correction;
    }
    for &(i, j, target) in &angles {
      let (dx, dy, dz) = (
        points[j][0] - points[i][0],
        points[j][1] - points[i][1],
        points[j][2] - points[i][2],
      );
      let length = (dx * dx + dy * dy + dz * dz).sqrt();
      if length < 1e-9 {
        continue;
      }
      let correction = (length - target) / length * 0.06;
      points[i][0] += dx * correction;
      points[i][1] += dy * correction;
      points[i][2] += dz * correction;
      points[j][0] -= dx * correction;
      points[j][1] -= dy * correction;
      points[j][2] -= dz * correction;
    }
    for i in 0..n {
      for j in i + 1..n {
        let (dx, dy, dz) = (
          points[j][0] - points[i][0],
          points[j][1] - points[i][1],
          points[j][2] - points[i][2],
        );
        let length = (dx * dx + dy * dy + dz * dz).sqrt();
        if length < 1e-9 || length >= 0.9 {
          continue;
        }
        let push = (0.9 - length) / length * 0.25;
        points[i][0] -= dx * push;
        points[i][1] -= dy * push;
        points[i][2] -= dz * push;
        points[j][0] += dx * push;
        points[j][1] += dy * push;
        points[j][2] += dz * push;
      }
    }
  }
  Ok(Some(points))
}

/// Returns complete graph distances in block-local order.
fn distances(rows: &[Vec<usize>]) -> Result<Vec<Vec<f64>>, Error> {
  let n = rows.len();
  let mut result = vec![vec![f64::INFINITY; n]; n];
  for (start, row) in result.iter_mut().enumerate() {
    row[start] = 0.;
    let mut pending = std::collections::VecDeque::from([start]);
    while let Some(node) = pending.pop_front() {
      let next = row[node] + 1.;
      for &other in &rows[node] {
        if next < row[other] {
          row[other] = next;
          pending.push_back(other);
        }
      }
    }
    if row.iter().any(|value| !value.is_finite()) {
      return Err(Error::CoordinateRange);
    }
  }
  Ok(result)
}

/// Returns deterministic points on the unit sphere.
fn sphere(count: usize) -> Vec<[f64; 3]> {
  let golden = PI * (3. - 5_f64.sqrt());
  (0..count)
    .map(|i| {
      let z = 1. - 2. * (i as f64 + 0.5) / count as f64;
      let radius = (1. - z * z).max(0.).sqrt();
      let angle = golden * i as f64;
      [radius * angle.cos(), radius * angle.sin(), z]
    })
    .collect()
}

/// Returns the cross product of two finite 3D vectors.
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
  [
    a[1] * b[2] - a[2] * b[1],
    a[2] * b[0] - a[0] * b[2],
    a[0] * b[1] - a[1] * b[0],
  ]
}

/// Returns the dot product of two 3D vectors.
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
  a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Returns the Euclidean distance between two 3D points.
fn distance3(a: [f64; 3], b: [f64; 3]) -> f64 {
  ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Normalizes one 3D vector, rejecting a degenerate direction.
fn normalize(mut vector: [f64; 3]) -> Result<[f64; 3], Error> {
  let length = dot(vector, vector).sqrt();
  if length < 1e-9 {
    return Err(Error::CoordinateRange);
  }
  vector[0] /= length;
  vector[1] /= length;
  vector[2] /= length;
  Ok(vector)
}

/// Normalizes one vector in place; returns false for a degenerate vector.
fn normalize_vector(vector: &mut [f64]) -> bool {
  let length = vector.iter().map(|value| value * value).sum::<f64>().sqrt();
  if length < 1e-12 {
    return false;
  }
  for value in vector.iter_mut() {
    *value /= length;
  }
  true
}

/// Removes the component of one vector along another unit vector.
fn deflate(vector: &mut [f64], axis: &[f64]) {
  let projection: f64 = vector.iter().zip(axis).map(|(a, b)| a * b).sum();
  for (value, base) in vector.iter_mut().zip(axis) {
    *value -= projection * base;
  }
}

/// Returns the axis-aligned area of one projection.
fn area(points: &[Point]) -> f64 {
  let mut min = Point::new(f64::INFINITY, f64::INFINITY);
  let mut max = Point::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
  for point in points {
    min = Point::new(min.x.min(point.x), min.y.min(point.y));
    max = Point::new(max.x.max(point.x), max.y.max(point.y));
  }
  ((max.x - min.x).max(0.)) * ((max.y - min.y).max(0.))
}

/// Reports whether one local edge is a single bond.
fn single(orders: &[BondOrder], edge: usize) -> bool {
  orders.get(edge) == Some(&BondOrder::Single)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn dense_shell_should_finish_inside_the_default_shared_allowance() -> Result<(), Box<dyn std::error::Error>> {
    let graph = chitin_chem::io::smiles::read_smiles("c12c3c4c5c1c6c7c8c2c9c1c3c2c3c4c4c%10c5c5c6c6c7c7c%11c8c9c8c9c1c2c1c2c3c4c3c4c%10c5c5c6c6c7c7c%11c8c8c9c1c1c2c3c2c4c5c6c3c7c8c1c23")?.into_graph();
    let graph = crate::chemical::integer_bonds(&graph)?;
    let topology = chitin_chem::perception::analyze_rings(&graph)?;
    let block = topology.blocks().first().ok_or("missing shell block")?;
    let indices = graph.atoms().enumerate().map(|(i, (id, _))| (id, i)).collect();
    let mut work = Work::new(3_850_000);
    let result = Projection::new(&graph, block, &indices, &mut work)?;
    assert!(
      result.is_some(),
      "no shell pose with {} work remaining",
      work.remaining()
    );
    Ok(())
  }
}
