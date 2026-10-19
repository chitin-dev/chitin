//! Shared geometry scoring and work accounting for discrete and continuous search.

use crate::geometry::Point;
use std::collections::{BTreeMap, BTreeSet};

/// Remaining work allowance; callers charge before each bounded scan.
pub(super) struct Budget(pub(super) usize);

impl Budget {
  /// Charges work without allowing the allowance to underflow.
  pub(super) fn charge(&mut self, units: usize) -> bool {
    if self.0 < units {
      self.0 = 0;
      false
    } else {
      self.0 -= units;
      true
    }
  }
}

/// Complete geometry score, with component-local diagnostic row indices.
#[derive(Default)]
pub(super) struct Quality {
  /// Atom, atom-edge and proper-crossing penalties in normalized drawing units.
  pub(super) cost: f64,
  /// Atom pairs closer than the caller's reporting threshold.
  pub(super) crowds: Vec<(usize, usize)>,
  /// Nonincident edge indices with a proper crossing.
  pub(super) crossings: BTreeSet<(usize, usize)>,
}

/// Measures complete unit-bond geometry with unit-cell broad-phase pruning.
///
/// # Parameters
/// - `points`, `segments`: complete component-local rows and skeleton edges.
/// - `threshold`: diagnostic center spacing, independent of score weights.
/// - `budget`: work allowance; final mandatory inspection uses an unlimited one.
///
/// # Returns
/// A fully scored result, or `None` before any unbudgeted work. Inputs must be
/// finite unit-space coordinates inside the caller's spatial range guard.
/// Sparse drawings use local cells; dense overlaps can still be quadratic.
pub(super) fn measure(
  points: &[Point],
  segments: &[[usize; 2]],
  threshold: f64,
  budget: &mut Budget,
) -> Option<Quality> {
  if !budget.charge(points.len() + segments.len()) {
    return None;
  }
  let mut atoms = BTreeMap::<(i64, i64), Vec<usize>>::new();
  let mut edges = BTreeMap::<(i64, i64), Vec<usize>>::new();
  for (i, &point) in points.iter().enumerate() {
    atoms.entry(cell(point)).or_default().push(i);
  }
  for (i, edge) in segments.iter().enumerate() {
    let [a, b] = edge.map(|node| cell(points[node]));
    for x in a.0.min(b.0)..=a.0.max(b.0) {
      for y in a.1.min(b.1)..=a.1.max(b.1) {
        edges.entry((x, y)).or_default().push(i);
      }
    }
  }
  let mut quality = Quality::default();
  for (i, &point) in points.iter().enumerate() {
    let (x, y) = cell(point);
    let mut close_edges = BTreeSet::<usize>::new();
    for dx in -1..=1 {
      for dy in -1..=1 {
        if let Some(others) = atoms.get(&(x + dx, y + dy)) {
          for &j in others.iter().filter(|&&j| j > i) {
            if !budget.charge(1) {
              return None;
            }
            let distance = point.distance(points[j]);
            quality.cost += (0.95 - distance).max(0.).powi(2);
            if distance < threshold {
              quality.crowds.push((i, j));
            }
          }
        }
        if let Some(candidates) = edges.get(&(x + dx, y + dy)) {
          if !budget.charge(candidates.len()) {
            return None;
          }
          close_edges.extend(candidates.iter().copied());
        }
      }
    }
    for &edge in &close_edges {
      if !budget.charge(1) {
        return None;
      }
      let [a, b] = segments[edge];
      if a != i && b != i {
        quality.cost += 4. * (0.25 - segment_distance(point, points[a], points[b])).max(0.).powi(2);
      }
    }
  }
  let mut tested = BTreeSet::new();
  for bucket in edges.values() {
    for (offset, &i) in bucket.iter().enumerate() {
      for &j in &bucket[offset + 1..] {
        if !budget.charge(1) {
          return None;
        }
        if !tested.insert((i, j)) {
          continue;
        }
        let [a, b] = segments[i];
        let [c, d] = segments[j];
        if a != c && a != d && b != c && b != d && crosses(points[a], points[b], points[c], points[d]) {
          quality.cost += 8.;
          quality.crossings.insert((i, j));
        }
      }
    }
  }
  Some(quality)
}

/// Returns a validated finite point's spatial cell.
fn cell(point: Point) -> (i64, i64) {
  (point.x.floor() as i64, point.y.floor() as i64)
}

/// Computes clearance from a finite, nondegenerate skeleton segment.
pub(super) fn segment_distance(point: Point, a: Point, b: Point) -> f64 {
  let (dx, dy) = (b.x - a.x, b.y - a.y);
  let t = (((point.x - a.x) * dx + (point.y - a.y) * dy) / (dx * dx + dy * dy)).clamp(0., 1.);
  point.distance(a.translated(t * dx, t * dy))
}

/// Tests proper intersections, excluding collinearity and endpoint tangencies.
pub(super) fn crosses(a: Point, b: Point, c: Point, d: Point) -> bool {
  let side = |p: Point, q: Point, r: Point| (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
  side(a, b, c) * side(a, b, d) < -1e-12 && side(c, d, a) * side(c, d, b) < -1e-12
}
