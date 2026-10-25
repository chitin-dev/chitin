//! Descriptive planar whitespace and footprint, independent of layout objectives.
//!
//! Rings here are simple bounded faces of the drawn cyclic blocks, not an SSSR
//! or all chemical cycles. A crossing block has no trusted planar face inventory.

use crate::{
  metrics::{point_segment, segments_contact},
  quality::{Metrics, Observation, State},
};
use anyhow::{Result, ensure};
use chitin_chem::{AtomId, MolecularGraph, perception::analyze_rings};
use chitin_chemdraw::geometry::Point;
use serde::{Deserialize, Serialize};
use std::{
  cmp::Ordering,
  collections::{BTreeMap, BTreeSet, BinaryHeap},
};

/// Maximum absolute radius error in units of the median positive bond length.
pub const CAVITY_TOLERANCE: f64 = 0.02;
/// Per-face cell evaluations are bounded so QA cannot monopolize a corpus worker.
pub const CAVITY_CELL_BUDGET: usize = 8192;

/// One simple bounded face; atom ordinals are zero-based source graph iteration order.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RingFace {
  /// Counterclockwise contour, without repeating the initial source atom ordinal.
  pub atoms: Vec<usize>,
  /// Shoelace area divided by L², excluding labels and stroke width.
  pub area: f64,
  /// Sum of contour skeleton segment lengths divided by L.
  pub perimeter: f64,
  /// Descriptive 4πA/P²; no preferred roundness or failure threshold is implied.
  pub isoperimetric_ratio: f64,
  /// A lower bound paired with a certified search upper bound, both divided by L.
  pub cavity_radius: f64,
  /// Remaining best-first search upper bound, including unvisited cells.
  pub cavity_upper_bound: f64,
  /// Whether the upper/lower radius gap meets the versioned absolute tolerance.
  pub cavity_converged: bool,
}

/// Retains contour provenance and incomplete domains instead of inventing passing zeroes.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct RingGeometry {
  /// Valid bounded faces; these remain auditable even when a global maximum is unknown.
  pub faces: Vec<RingFace>,
  /// Cyclic blocks with crossing, degenerate or incomplete planar embeddings.
  pub excluded_blocks: usize,
  /// Requested absolute cavity error bound in units of L.
  pub cavity_tolerance: f64,
}

/// Adds three descriptive measurements without altering coordinates or readability flags.
///
/// # Parameters
/// - `graph`, `positions`: immutable source topology and final skeleton coordinates.
/// - `scale`: median strictly positive skeleton bond length.
/// - `perspective`: excludes authored projection geometry from these planar metrics.
/// - `output`: receives observations and face provenance, never changed coordinates.
///
/// Every component's bonds obstruct cavities in that
/// component; unrelated salt packing does not obstruct a ring or enlarge its hull.
///
/// # Returns
/// Maximum cavity radius, maximum face roundness and component-wise normalized
/// footprint. Incomplete embeddings/searches remain unavailable with a reason.
///
/// # Errors
/// Propagates graph errors and rejects nonfinite normalized geometry.
pub fn measure(
  graph: &MolecularGraph,
  positions: &BTreeMap<AtomId, Point>,
  scale: Option<f64>,
  perspective: bool,
  output: &mut Metrics,
) -> Result<()> {
  let keys = [
    "maximum_ring_cavity_radius",
    "ring_isoperimetric_ratio",
    "normalized_footprint",
  ];
  let Some(scale) = scale.filter(|_| !perspective) else {
    for key in keys {
      output.measurements.insert(
        key.into(),
        Observation::unavailable(
          State::NotApplicable,
          0,
          if perspective {
            "planar whitespace excludes perspective"
          } else {
            "no positive skeleton bond length"
          },
        ),
      );
    }
    return Ok(());
  };
  let origin = positions.values().next().copied().unwrap_or(Point::new(0., 0.));
  let points: BTreeMap<_, _> = positions
    .iter()
    .map(|(&id, p)| (id, Point::new((p.x - origin.x) / scale, (p.y - origin.y) / scale)))
    .collect();
  ensure!(
    points.values().all(|p| p.x.is_finite() && p.y.is_finite()),
    "nonfinite normalized ring coordinates"
  );
  let ordinals: BTreeMap<_, _> = graph.atoms().enumerate().map(|(i, (id, _))| (id, i)).collect();
  let ConnectedComponents {
    groups: components,
    membership,
  } = components(graph)?;
  let obstacles: Vec<Vec<_>> = components
    .iter()
    .enumerate()
    .map(|(index, _)| {
      graph
        .bonds()
        .filter_map(|(_, bond)| {
          let [a, b] = bond.atoms();
          (membership[&a] == index).then_some((points[&a], points[&b]))
        })
        .collect()
    })
    .collect();
  let footprint = components
    .iter()
    .map(|component| convex_area(component.iter().map(|id| points[id]).collect()))
    .sum::<f64>()
    / graph.atom_count() as f64;
  ensure!(footprint.is_finite(), "nonfinite normalized footprint");
  output.measurements.insert(
    "normalized_footprint".into(),
    Observation::measured(footprint, graph.atom_count(), None),
  );
  let topology = analyze_rings(graph)?;
  let mut details = RingGeometry {
    cavity_tolerance: CAVITY_TOLERANCE,
    ..Default::default()
  };
  for block in topology.blocks().iter().filter(|block| block.cycle_rank() > 0) {
    let edges: Vec<_> = block
      .bonds()
      .iter()
      .filter_map(|edge| graph.bond(*edge).map(|bond| bond.atoms()))
      .collect();
    if !simple_embedding(&edges, &points) {
      details.excluded_blocks += 1;
      continue;
    }
    let faces = bounded_faces(&edges, &points);
    if faces.len() != block.cycle_rank() {
      details.excluded_blocks += 1;
      continue;
    }
    for face in faces {
      let polygon: Vec<_> = face.iter().map(|id| points[id]).collect();
      let area = signed_area(&polygon);
      let perimeter = polygon
        .iter()
        .zip(polygon.iter().cycle().skip(1))
        .map(|(a, b)| a.distance(*b))
        .sum::<f64>();
      ensure!(
        area.is_finite() && perimeter.is_finite(),
        "nonfinite ring area or perimeter"
      );
      let (radius, upper) = cavity(&polygon, &obstacles[membership[&face[0]]], CAVITY_CELL_BUDGET);
      ensure!(radius.is_finite() && upper.is_finite(), "nonfinite cavity bounds");
      details.faces.push(RingFace {
        atoms: face.iter().map(|id| ordinals[id]).collect(),
        area,
        perimeter,
        isoperimetric_ratio: (4. * std::f64::consts::PI * (area / perimeter) / perimeter).clamp(0., 1.),
        cavity_radius: radius,
        cavity_upper_bound: upper,
        cavity_converged: upper - radius <= CAVITY_TOLERANCE,
      });
    }
  }
  let count = details.faces.len();
  for (key, value, converged) in [
    (
      "maximum_ring_cavity_radius",
      details.faces.iter().map(|face| face.cavity_radius).fold(0., f64::max),
      details.faces.iter().all(|face| face.cavity_converged),
    ),
    (
      "ring_isoperimetric_ratio",
      details
        .faces
        .iter()
        .map(|face| face.isoperimetric_ratio)
        .fold(0., f64::max),
      true,
    ),
  ] {
    let observation = if details.excluded_blocks > 0 {
      Observation::unavailable(
        State::NotImplemented,
        count,
        "crossing, degenerate or incomplete cyclic embedding; global maximum unknown",
      )
    } else if count == 0 {
      Observation::unavailable(State::NotApplicable, 0, "no bounded ring faces")
    } else if !converged {
      Observation::unavailable(
        State::NotImplemented,
        count,
        "cavity search exhausted its cell budget; inspect per-face bounds",
      )
    } else {
      Observation::measured(value, count, None)
    };
    output.measurements.insert(key.into(), observation);
  }
  output.ring_geometry = Some(details);
  Ok(())
}

/// Component identities keep footprints and obstruction sets independent of salt placement.
struct ConnectedComponents {
  groups: Vec<Vec<AtomId>>,
  membership: BTreeMap<AtomId, usize>,
}

/// Assigns each source atom to exactly one connected obstruction/footprint domain.
fn components(graph: &MolecularGraph) -> Result<ConnectedComponents> {
  let mut remaining: BTreeSet<_> = graph.atoms().map(|(id, _)| id).collect();
  let mut groups = Vec::new();
  let mut membership = BTreeMap::new();
  while let Some(first) = remaining.pop_first() {
    let mut stack = vec![first];
    let mut group = Vec::new();
    while let Some(id) = stack.pop() {
      membership.insert(id, groups.len());
      group.push(id);
      for (next, _) in graph.neighbors(id)? {
        if remaining.remove(&next) {
          stack.push(next);
        }
      }
    }
    groups.push(group);
  }
  Ok(ConnectedComponents { groups, membership })
}

/// Closed segment contacts disqualify a block; coincident adjacent rays are also invalid.
fn simple_embedding(edges: &[[AtomId; 2]], points: &BTreeMap<AtomId, Point>) -> bool {
  for (i, &[a, b]) in edges.iter().enumerate() {
    if points[&a].distance(points[&b]) <= 1e-9 {
      return false;
    }
    for &[c, d] in &edges[i + 1..] {
      if ![a, b].iter().any(|id| [c, d].contains(id)) {
        if segments_contact(points[&a], points[&b], points[&c], points[&d], 1e-9) {
          return false;
        }
      } else {
        let shared = if [c, d].contains(&a) { a } else { b };
        let other_a = if shared == a { b } else { a };
        let other_b = if shared == c { d } else { c };
        if point_segment(points[&other_a], points[&shared], points[&other_b]) <= 1e-9
          || point_segment(points[&other_b], points[&shared], points[&other_a]) <= 1e-9
        {
          return false;
        }
      }
    }
  }
  true
}

/// Clockwise successor around the reverse half-edge walks the face to its left.
fn bounded_faces(edges: &[[AtomId; 2]], points: &BTreeMap<AtomId, Point>) -> Vec<Vec<AtomId>> {
  let mut neighbors: BTreeMap<AtomId, Vec<AtomId>> = BTreeMap::new();
  for &[a, b] in edges {
    neighbors.entry(a).or_default().push(b);
    neighbors.entry(b).or_default().push(a);
  }
  for (center, row) in &mut neighbors {
    let origin = points[center];
    row.sort_by(|a, b| {
      (points[a].y - origin.y)
        .atan2(points[a].x - origin.x)
        .total_cmp(&(points[b].y - origin.y).atan2(points[b].x - origin.x))
        .then(a.cmp(b))
    });
  }
  let mut visited = BTreeSet::new();
  let mut faces = Vec::new();
  for &[a, b] in edges {
    for start in [(a, b), (b, a)] {
      if visited.contains(&start) {
        continue;
      }
      let mut current = start;
      let mut face = Vec::new();
      loop {
        if !visited.insert(current) {
          break;
        }
        face.push(current.0);
        let row = &neighbors[&current.1];
        let Some(reverse) = row.iter().position(|id| *id == current.0) else {
          break;
        };
        current = (current.1, row[(reverse + row.len() - 1) % row.len()]);
        if current == start {
          break;
        }
      }
      if current == start && face.iter().copied().collect::<BTreeSet<_>>().len() == face.len() {
        let polygon: Vec<_> = face.iter().map(|id| points[id]).collect();
        if signed_area(&polygon) > 1e-12 {
          faces.push(face);
        }
      }
    }
  }
  faces
}

/// Origin-relative shoelace avoids translation-dependent cancellation.
fn signed_area(polygon: &[Point]) -> f64 {
  let Some(origin) = polygon.first() else {
    return 0.;
  };
  polygon
    .iter()
    .zip(polygon.iter().cycle().skip(1))
    .map(|(a, b)| (a.x - origin.x) * (b.y - origin.y) - (a.y - origin.y) * (b.x - origin.x))
    .sum::<f64>()
    / 2.
}

/// Monotone-chain hull area; collinear components correctly occupy zero planar area.
fn convex_area(mut points: Vec<Point>) -> f64 {
  points.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
  points.dedup();
  if points.len() < 3 {
    return 0.;
  }
  let cross = |a: Point, b: Point, c: Point| (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
  let mut hull: Vec<Point> = Vec::new();
  for p in &points {
    while hull.len() >= 2 && cross(hull[hull.len() - 2], hull[hull.len() - 1], *p) <= 0. {
      hull.pop();
    }
    hull.push(*p);
  }
  let lower = hull.len();
  for p in points.iter().rev().skip(1) {
    while hull.len() > lower && cross(hull[hull.len() - 2], hull[hull.len() - 1], *p) <= 0. {
      hull.pop();
    }
    hull.push(*p);
  }
  hull.pop();
  signed_area(&hull).abs()
}

/// Signed boundary clearance outside, and all-component skeleton clearance inside.
fn clearance(p: Point, polygon: &[Point], obstacles: &[(Point, Point)]) -> f64 {
  let mut inside = false;
  let mut boundary = f64::INFINITY;
  for (a, b) in polygon.iter().zip(polygon.iter().cycle().skip(1)) {
    boundary = boundary.min(point_segment(p, *a, *b));
    if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
      inside = !inside;
    }
  }
  if !inside {
    return -boundary;
  }
  obstacles
    .iter()
    .map(|&(a, b)| point_segment(p, a, b))
    .fold(boundary, f64::min)
}

/// A square's 1-Lipschitz upper bound on the largest clearance anywhere inside it.
#[derive(Clone, Copy)]
struct Cell {
  center: Point,
  half: f64,
  upper: f64,
}
impl PartialEq for Cell {
  fn eq(&self, other: &Self) -> bool {
    self.cmp(other) == Ordering::Equal
  }
}
impl Eq for Cell {}
impl PartialOrd for Cell {
  fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
    Some(self.cmp(other))
  }
}
impl Ord for Cell {
  fn cmp(&self, other: &Self) -> Ordering {
    self
      .upper
      .total_cmp(&other.upper)
      .then(self.center.x.total_cmp(&other.center.x))
      .then(self.center.y.total_cmp(&other.center.y))
      .then(self.half.total_cmp(&other.half))
  }
}

/// Deterministic best-first subdivision returns lower/upper bounds even on budget exhaustion.
fn cavity(polygon: &[Point], obstacles: &[(Point, Point)], budget: usize) -> (f64, f64) {
  let min_x = polygon.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
  let min_y = polygon.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
  let max_x = polygon.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
  let max_y = polygon.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
  let center = Point::new(min_x / 2. + max_x / 2., min_y / 2. + max_y / 2.);
  let half = (max_x - min_x).max(max_y - min_y) / 2.;
  let mut best = clearance(center, polygon, obstacles).max(0.);
  let mut queue = BinaryHeap::from([Cell {
    center,
    half,
    upper: best + half * 2_f64.sqrt(),
  }]);
  let mut cells = 1;
  while let Some(cell) = queue.peek().copied() {
    if cell.upper - best <= CAVITY_TOLERANCE || cells + 4 > budget {
      break;
    }
    queue.pop();
    let half = cell.half / 2.;
    for (dx, dy) in [(-half, -half), (-half, half), (half, -half), (half, half)] {
      let center = Point::new(cell.center.x + dx, cell.center.y + dy);
      let distance = clearance(center, polygon, obstacles);
      best = best.max(distance);
      queue.push(Cell {
        center,
        half,
        upper: distance + half * 2_f64.sqrt(),
      });
    }
    cells += 4;
  }
  (best, queue.peek().map_or(best, |cell| cell.upper.max(best)))
}

#[cfg(test)]
mod tests;
