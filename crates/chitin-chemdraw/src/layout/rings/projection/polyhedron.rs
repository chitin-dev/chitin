//! Supporting-face perspective followed by a bounded, fixed-boundary planar solve.
//!
//! This is a planar fallback for small cages, not a spatial camera projection.
//! A short cycle on a supporting plane supplies a convex outer boundary; remaining atoms
//! relax inside it. Every accepted drawing retains every source edge and must
//! pass geometric separation and intersection checks. No molecular coordinates,
//! bond orders, aromatic facts or stereochemical descriptors are modified.

use std::{collections::BTreeMap, f64::consts::TAU};

use chitin_chem::{AtomId, MolecularGraph, perception::RingBlock};

use super::{Error, POSES, Point, Pose, Work, cross, dot, normalize, quality};
use crate::layout::rings::constraints::Constraints;

/// Maximum supporting faces solved; the shared job budget remains authoritative.
const FACE_TRIALS: usize = 8;

/// Projects and unfolds a compact shell without inventing foreground bond cuts.
///
/// # Parameters
/// - `graph`, `block`, `indices`: resolved source bonds and block-local rows.
/// - `rows`, `edges`, `seed`: complete adjacency and drawing-only spatial seed.
/// - `work`: shared allowance covering cycle discovery, solves and validation.
///
/// # Returns
/// Finite planar shell poses with convex outer faces and sufficient clearance,
/// or `None` when no tested face yields a safe complete embedding. Short-cycle
/// discovery is shared with ordinary rings; it is not aromaticity perception.
///
/// # Errors
/// Propagates graph inconsistencies and exhausted work, never a partial embedding.
pub(super) fn project(
  graph: &MolecularGraph,
  block: &RingBlock,
  indices: &BTreeMap<AtomId, usize>,
  rows: &[Vec<usize>],
  edges: &[[usize; 2]],
  seed: &[[f64; 3]],
  work: &mut Work,
) -> Result<Option<Vec<Pose>>, Error> {
  let mut cycles = Constraints::cycles(graph, block, indices, work)?;
  // Larger short faces leave more room for the inner shell at the same edge
  // length. A finite trial cap must not be consumed by equivalent small faces.
  cycles.sort_by_key(|cycle| std::cmp::Reverse(cycle.len()));
  let mut poses = Vec::new();
  let mut trials = 0;
  for cycle in cycles.iter().filter(|cycle| (3..=8).contains(&cycle.len())) {
    let Some(mut points) = perspective(seed, cycle, work)? else {
      continue;
    };
    trials += 1;
    // Use a regular convex frame in the projected face's traversal order. This
    // removes spatial-seed face warping without pulling shared corners apart.
    let radius = 1. / (2. * (std::f64::consts::PI / cycle.len() as f64).sin());
    for (i, &row) in cycle.iter().enumerate() {
      let angle = TAU * i as f64 / cycle.len() as f64;
      points[row] = Point::new(radius * angle.cos(), radius * angle.sin());
    }
    if harmonic(&mut points, cycle, rows, work)? {
      let mut best: Option<Vec<Point>> = None;
      // A harmonic shell concentrates its far surface near the center. Spread
      // that surface before scaling, then independently validate every straight
      // source edge: radial redistribution alone does not preserve planarity.
      for power in [1., 0.8, 0.65, 0.5] {
        let mut trial = redistribute(&points, cycle, radius, power, work)?;
        if !normalize_clearance(&mut trial, edges, work)? {
          continue;
        }
        let inspection = work.measure(&trial, edges)?;
        if inspection.crowds.is_empty()
          && inspection.crossings.is_empty()
          && best
            .as_ref()
            .is_none_or(|old| longest(&trial, edges) < longest(old, edges))
        {
          best = Some(trial);
        }
      }
      if let Some(points) = best {
        poses.push(Pose {
          points,
          crossings: Vec::new(),
          depths: Vec::new(),
        });
      }
    }
    if trials >= FACE_TRIALS || poses.len() >= POSES {
      break;
    }
  }
  // Shorter maximum edges make the outer frame less dominant at the same
  // guaranteed minimum clearance. This ranking is deterministic, not chemical.
  poses.sort_by(|a, b| longest(&a.points, edges).total_cmp(&longest(&b.points, edges)));
  Ok((!poses.is_empty()).then_some(poses))
}

/// Redistributes interior radii inside the exact regular-polygon boundary.
///
/// The radius limit depends on direction, not only the circumcircle; otherwise
/// expanding the far surface could push atoms through a face side. Boundary
/// atoms stay bitwise fixed. This is a drawing candidate only, always followed
/// by source-edge intersection and nonincident-clearance validation.
fn redistribute(
  points: &[Point],
  boundary: &[usize],
  radius: f64,
  power: f64,
  work: &mut Work,
) -> Result<Vec<Point>, Error> {
  work.charge(points.len() * (boundary.len() + 8))?;
  let step = TAU / boundary.len() as f64;
  Ok(
    points
      .iter()
      .enumerate()
      .map(|(row, &point)| {
        let distance = point.x.hypot(point.y);
        if boundary.contains(&row) || distance < 1e-12 {
          return point;
        }
        let angle = point.y.atan2(point.x).rem_euclid(TAU);
        let normal = ((angle / step).floor() + 0.5) * step;
        let limit = radius * (step / 2.).cos() / (angle - normal).cos();
        let factor = ((distance / limit).min(1.).powf(power) * limit) / distance;
        Point::new(point.x * factor, point.y * factor)
      })
      .collect(),
  )
}

/// Solves the fixed-boundary graph Laplacian with a bounded dense factorization.
///
/// # Parameters
/// - `points`, `boundary`: private projection and immutable convex frame rows.
/// - `rows`: symmetric block adjacency; interior components must reach the frame.
/// - `work`: charged cubic factorization and triangular-solve allowance.
///
/// # Returns
/// `true` after both coordinates solve the same positive-definite system.
/// Degenerate or disconnected interior systems return `false`. A direct solve
/// avoids accepting an unconverged drawing or repeatedly spending a fixed sweep
/// allowance on shells with slow boundary propagation.
///
/// # Errors
/// Exhausted work rejects the private trial before factorization starts.
fn harmonic(points: &mut [Point], boundary: &[usize], rows: &[Vec<usize>], work: &mut Work) -> Result<bool, Error> {
  let interior: Vec<_> = (0..rows.len()).filter(|i| !boundary.contains(i)).collect();
  let m = interior.len();
  work.charge(m * m * m + m * m * 4 + rows.len() * 4)?;
  let mut indices = vec![usize::MAX; rows.len()];
  for (i, &row) in interior.iter().enumerate() {
    indices[row] = i;
  }
  let mut matrix = vec![vec![0.; m]; m];
  let mut rhs = vec![Point::new(0., 0.); m];
  for (i, &row) in interior.iter().enumerate() {
    matrix[i][i] = rows[row].len() as f64;
    for &neighbor in &rows[row] {
      let j = indices[neighbor];
      if j == usize::MAX {
        rhs[i].x += points[neighbor].x;
        rhs[i].y += points[neighbor].y;
      } else {
        matrix[i][j] -= 1.;
      }
    }
  }
  for i in 0..m {
    for j in 0..=i {
      let value = matrix[i][j] - (0..j).map(|k| matrix[i][k] * matrix[j][k]).sum::<f64>();
      if i == j {
        if !value.is_finite() || value <= 1e-12 {
          return Ok(false);
        }
        matrix[i][j] = value.sqrt();
      } else {
        matrix[i][j] = value / matrix[j][j];
      }
    }
  }
  for i in 0..m {
    let x = (0..i).map(|j| matrix[i][j] * rhs[j].x).sum::<f64>();
    let y = (0..i).map(|j| matrix[i][j] * rhs[j].y).sum::<f64>();
    rhs[i] = Point::new((rhs[i].x - x) / matrix[i][i], (rhs[i].y - y) / matrix[i][i]);
  }
  for i in (0..m).rev() {
    let x = (i + 1..m).map(|j| matrix[j][i] * rhs[j].x).sum::<f64>();
    let y = (i + 1..m).map(|j| matrix[j][i] * rhs[j].y).sum::<f64>();
    rhs[i] = Point::new((rhs[i].x - x) / matrix[i][i], (rhs[i].y - y) / matrix[i][i]);
  }
  for (i, &row) in interior.iter().enumerate() {
    points[row] = rhs[i];
  }
  Ok(true)
}

/// Seeds a face-backed perspective only when its plane supports the spatial shell.
///
/// Near-planarity tolerance accommodates relaxed drawing seeds, not arbitrary
/// nonconvex ring faces. Geometric validation after the planar solve remains
/// mandatory; a supporting-plane test alone is not a planarity certificate.
fn perspective(seed: &[[f64; 3]], cycle: &[usize], work: &mut Work) -> Result<Option<Vec<Point>>, Error> {
  work.charge(seed.len() * 3 + cycle.len() * 5)?;
  let mut center = [0.; 3];
  for &row in cycle {
    for (axis, value) in center.iter_mut().enumerate() {
      *value += seed[row][axis] / cycle.len() as f64;
    }
  }
  let sub = |a: [f64; 3], b: [f64; 3]| std::array::from_fn(|i| a[i] - b[i]);
  let mut normal = [0.; 3];
  for (&a, &b) in cycle.iter().zip(cycle.iter().cycle().skip(1)) {
    let value = cross(sub(seed[a], center), sub(seed[b], center));
    for i in 0..3 {
      normal[i] += value[i];
    }
  }
  if dot(normal, normal) < 1e-12 {
    return Ok(None);
  }
  let mut normal = normalize(normal)?;
  let centroid: [f64; 3] = std::array::from_fn(|axis| seed.iter().map(|p| p[axis]).sum::<f64>() / seed.len() as f64);
  if dot(normal, sub(center, centroid)) < 0. {
    normal = normal.map(|value| -value);
  }
  let heights: Vec<_> = seed.iter().map(|&point| dot(sub(point, center), normal)).collect();
  if heights
    .iter()
    .enumerate()
    .any(|(row, &h)| if cycle.contains(&row) { h.abs() > 0.2 } else { h > 0.05 })
  {
    return Ok(None);
  }
  let right = normalize(sub(seed[cycle[0]], center))?;
  let up = normalize(cross(normal, right))?;
  let eye = heights.iter().copied().fold(0_f64, f64::max) + 0.3;
  Ok(Some(
    seed
      .iter()
      .zip(heights)
      .map(|(&point, height)| {
        let offset = sub(point, center);
        let factor = eye / (eye - height);
        Point::new(factor * dot(offset, right), factor * dot(offset, up))
      })
      .collect(),
  ))
}

/// Scales a complete embedding for center, edge and nonincident-label clearance.
///
/// Perspective does not promise equal bonds. Only uniform scaling is allowed
/// here: scaling individual edges would invalidate ring closure. Near-zero
/// separation rejects collapsed or touching embeddings rather than amplifying
/// them into an unbounded diagram. All points remain under a finite extent cap.
fn normalize_clearance(points: &mut [Point], edges: &[[usize; 2]], work: &mut Work) -> Result<bool, Error> {
  work.charge(points.len() * (points.len() + edges.len()) + edges.len())?;
  let shortest = edges
    .iter()
    .map(|&[a, b]| points[a].distance(points[b]))
    .fold(f64::INFINITY, f64::min);
  let mut centers = f64::INFINITY;
  let mut clearance = f64::INFINITY;
  for (i, &point) in points.iter().enumerate() {
    for &other in &points[i + 1..] {
      centers = centers.min(point.distance(other));
    }
    for &[a, b] in edges {
      if i != a && i != b {
        clearance = clearance.min(quality::segment_distance(point, points[a], points[b]));
      }
    }
  }
  if shortest < 1e-8 || centers < 1e-8 || clearance < 1e-8 {
    return Ok(false);
  }
  let scale = (0.85 / shortest).max(0.8 / centers).max(0.28 / clearance);
  if !scale.is_finite() || scale > 40. || longest(points, edges) / shortest > 16. {
    return Ok(false);
  }
  for point in points {
    point.x *= scale;
    point.y *= scale;
  }
  Ok(true)
}

/// Returns the maximum projected edge length for deterministic pose ranking.
fn longest(points: &[Point], edges: &[[usize; 2]]) -> f64 {
  edges
    .iter()
    .map(|&[a, b]| points[a].distance(points[b]))
    .fold(0_f64, f64::max)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn boundary_solve_should_preserve_anchors_and_satisfy_the_interior_equation() -> Result<(), Error> {
    let mut points = vec![
      Point::new(1., 0.),
      Point::new(0., 1.),
      Point::new(-1., 0.),
      Point::new(0., -1.),
      Point::new(0.3, 0.4),
    ];
    let fixed = points[..4].to_vec();
    let rows = vec![
      vec![1, 3, 4],
      vec![0, 2, 4],
      vec![1, 3, 4],
      vec![0, 2, 4],
      vec![0, 1, 2, 3],
    ];
    let mut work = Work::new(10_000);
    assert!(harmonic(&mut points, &[0, 1, 2, 3], &rows, &mut work)?);
    assert_eq!(&points[..4], fixed.as_slice());
    assert!(points[4].distance(Point::new(0., 0.)) < 1e-12);
    Ok(())
  }

  #[test]
  fn exhausted_boundary_solve_should_leave_private_coordinates_untouched() {
    let mut points = vec![Point::new(1., 0.), Point::new(3., 4.)];
    let before = points.clone();
    let mut work = Work::new(1);
    assert!(matches!(
      harmonic(&mut points, &[0], &[vec![1], vec![0]], &mut work),
      Err(Error::RingSearchBudgetExceeded)
    ));
    assert_eq!(points, before);
  }

  #[test]
  fn disconnected_interior_should_not_publish_a_singular_embedding() -> Result<(), Error> {
    let mut points = vec![Point::new(1., 0.), Point::new(3., 4.)];
    let before = points.clone();
    let mut work = Work::new(10_000);
    assert!(!harmonic(&mut points, &[0], &[vec![], vec![]], &mut work)?);
    assert_eq!(points, before);
    Ok(())
  }

  #[test]
  fn shell_validation_should_reject_a_nonincident_atom_touching_a_bond() -> Result<(), Error> {
    let mut points = vec![
      Point::new(-1., 0.),
      Point::new(1., 0.),
      Point::new(0., 0.),
      Point::new(0., 1.),
    ];
    let mut work = Work::new(10_000);
    assert!(!normalize_clearance(&mut points, &[[0, 1], [2, 3]], &mut work)?);
    Ok(())
  }
}
