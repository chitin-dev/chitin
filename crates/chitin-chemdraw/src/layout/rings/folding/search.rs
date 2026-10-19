//! Finite unit-step walks with exact two-circle endpoint closure.

use super::{Error, Point, Work};
use crate::layout::quality::{crosses, segment_distance};
use std::{collections::BTreeSet, f64::consts::PI};

/// A partial walk stores real path-vertex coordinates, never extra bend points.
struct Walk {
  /// Prefix through the current genuine source vertex.
  points: Vec<Point>,
  /// Clearance/guide cost used only to prune incomplete alternatives.
  cost: f64,
}

/// Searches a bounded beam; unreachable endpoints never produce stretched bonds.
///
/// # Parameters
/// * `path`: one connected source walk with distinct, immovable endpoints.
/// * `old`, `edges`, `linear`: complete block coordinates and immutable guards.
/// * `work`: shared allowance charged before every expansion and geometry scan.
///
/// # Returns
/// Complete snapshots. All path edges have unit length and all other positions
/// are unchanged. Empty output is a normal infeasible discrete search result.
///
/// # Errors
/// Budget exhaustion abandons the search rather than returning an unscored walk.
pub(super) fn candidates(
  path: &[usize],
  old: &[Point],
  edges: &[[usize; 2]],
  linear: &[bool],
  work: &mut Work,
) -> Result<Vec<Vec<Point>>, Error> {
  let end = old[path[path.len() - 1]];
  let start = old[path[0]];
  let angle = (old[path[1]].y - start.y).atan2(old[path[1]].x - start.x);
  let mut beam = vec![Walk {
    points: vec![start],
    cost: 0.,
  }];
  for index in 1..path.len() - 2 {
    let mut next = Vec::new();
    for walk in &beam {
      let current = walk.points[index - 1];
      let base = if index == 1 {
        angle
      } else {
        let previous = walk.points[index - 2];
        (current.y - previous.y).atan2(current.x - previous.x)
      };
      let turns: &[f64] = if index == 1 {
        &[0., PI / 6., -PI / 6., PI / 3., -PI / 3.]
      } else if linear[path[index - 1]] {
        &[0.]
      } else {
        &[PI / 3., -PI / 3., PI / 4., -PI / 4.]
      };
      for &turn in turns {
        work.charge(path.len() + old.len() + edges.len())?;
        let direction = base + turn;
        let point = current.translated(direction.cos(), direction.sin());
        if point.distance(end) > (path.len() - 1 - index) as f64 + 1e-9 {
          continue;
        }
        if !clear(path, index, &walk.points, point, old, edges) {
          continue;
        }
        let mut points = walk.points.clone();
        points.push(point);
        let remaining = (path.len() - 1 - index) as f64;
        let cost = walk.cost
          + 0.02 * point.distance(old[path[index]]).powi(2)
          + 0.01 * (point.distance(end) - remaining * 0.8).powi(2);
        next.push(Walk { points, cost });
      }
    }
    next.sort_by(|a, b| a.cost.total_cmp(&b.cost));
    let mut unique = BTreeSet::new();
    next.retain(|walk| {
      let last = walk.points[index];
      let previous = walk.points[index - 1];
      unique.insert((
        (last.x * 1e6).round() as i64,
        (last.y * 1e6).round() as i64,
        ((last.y - previous.y).atan2(last.x - previous.x) * 1e6).round() as i64,
      ))
    });
    next.truncate(6);
    beam = next;
    if beam.is_empty() {
      return Ok(Vec::new());
    }
  }
  let mut result = Vec::new();
  for walk in beam {
    let a = *walk.points.last().ok_or(Error::CoordinateRange)?;
    let distance = a.distance(end);
    if !(1e-8..=2. + 1e-10).contains(&distance) {
      continue;
    }
    let middle = Point::new((a.x + end.x) / 2., (a.y + end.y) / 2.);
    let height = (1. - distance * distance / 4.).max(0.).sqrt();
    for sign in [-1., 1.] {
      work.charge(old.len() + path.len() + edges.len())?;
      let point = middle.translated(
        -(end.y - a.y) / distance * height * sign,
        (end.x - a.x) / distance * height * sign,
      );
      if !clear(path, path.len() - 2, &walk.points, point, old, edges) {
        continue;
      }
      let mut candidate = old.to_vec();
      for (&row, &p) in path.iter().zip(&walk.points) {
        candidate[row] = p;
      }
      candidate[path[path.len() - 2]] = point;
      if path
        .windows(2)
        .any(|pair| (candidate[pair[0]].distance(candidate[pair[1]]) - 1.).abs() > 1e-9)
      {
        continue;
      }
      if !angles_valid(path, &candidate, linear) {
        continue;
      }
      result.push(candidate);
    }
  }
  Ok(result)
}

/// Rejects nonincident center/edge contacts against the immutable rest of the block.
fn clear(path: &[usize], index: usize, prefix: &[Point], point: Point, old: &[Point], edges: &[[usize; 2]]) -> bool {
  let current = prefix[prefix.len() - 1];
  for (row, &other) in old.iter().enumerate() {
    if path[1..path.len() - 1].contains(&row) {
      continue;
    }
    if point.distance(other) < 0.5 {
      return false;
    }
  }
  if prefix
    .iter()
    .take(prefix.len().saturating_sub(1))
    .any(|&other| point.distance(other) < 0.55)
  {
    return false;
  }
  for &[a, b] in edges {
    if path
      .windows(2)
      .any(|pair| (pair[0] == a && pair[1] == b) || (pair[0] == b && pair[1] == a))
    {
      continue;
    }
    if ![a, b].contains(&path[index - 1])
      && (crosses(current, point, old[a], old[b]) || segment_distance(point, old[a], old[b]) < 0.08)
    {
      return false;
    }
  }
  true
}

/// Protects visible bends and required linearity, including the analytic closure.
fn angles_valid(path: &[usize], points: &[Point], linear: &[bool]) -> bool {
  path.windows(3).all(|triple| {
    let [a, c, b] = [points[triple[0]], points[triple[1]], points[triple[2]]];
    let cosine = ((a.x - c.x) * (b.x - c.x) + (a.y - c.y) * (b.y - c.y)) / (a.distance(c) * b.distance(c));
    if linear[triple[1]] {
      (cosine + 1.).abs() < 1e-8
    } else {
      (-0.94..=0.5).contains(&cosine)
    }
  })
}

/// Prefers recognizable polygon-derived turns without changing chemical facts.
pub(super) fn turn_cost(path: &[usize], points: &[Point], linear: &[bool]) -> f64 {
  path
    .windows(3)
    .map(|triple| {
      let [a, c, b] = [points[triple[0]], points[triple[1]], points[triple[2]]];
      let cosine = ((a.x - c.x) * (b.x - c.x) + (a.y - c.y) * (b.y - c.y)) / (a.distance(c) * b.distance(c));
      let error = if linear[triple[1]] {
        (cosine + 1.).powi(2)
      } else {
        [108_f64, 120., 128.57142857142858, 135.]
          .iter()
          .map(|angle| (cosine - angle.to_radians().cos()).powi(2))
          .fold(f64::INFINITY, f64::min)
      };
      0.2 * error
    })
    .sum()
}

#[cfg(test)]
mod tests {
  use super::*;
  type TestResult = Result<(), Box<dyn std::error::Error>>;

  #[test]
  fn unreachable_fixed_endpoints_should_not_stretch_the_walk() -> TestResult {
    let old = [Point::new(0., 0.), Point::new(1., 0.), Point::new(3., 0.)];
    let result = candidates(&[0, 1, 2], &old, &[[0, 1], [1, 2]], &[false; 3], &mut Work::new(1000))?;
    assert!(result.is_empty());
    Ok(())
  }

  #[test]
  fn analytic_closure_should_retain_a_required_linear_center() -> TestResult {
    let old = [Point::new(0., 0.), Point::new(1., 0.), Point::new(2., 0.)];
    let result = candidates(
      &[0, 1, 2],
      &old,
      &[[0, 1], [1, 2]],
      &[false, true, false],
      &mut Work::new(1000),
    )?;
    assert!(!result.is_empty());
    for candidate in result {
      assert_eq!(candidate[0], old[0]);
      assert_eq!(candidate[2], old[2]);
      assert!(candidate[1].distance(old[1]) < 1e-10);
      assert!(angles_valid(&[0, 1, 2], &candidate, &[false, true, false]));
    }
    Ok(())
  }

  #[test]
  fn bent_closure_should_not_override_a_linear_chemical_center() -> TestResult {
    let old = [Point::new(0., 0.), Point::new(0.5, 0.8), Point::new(1., 0.)];
    let result = candidates(
      &[0, 1, 2],
      &old,
      &[[0, 1], [1, 2]],
      &[false, true, false],
      &mut Work::new(1000),
    )?;
    assert!(result.is_empty());
    Ok(())
  }
}
