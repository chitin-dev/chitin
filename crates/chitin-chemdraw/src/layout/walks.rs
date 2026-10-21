//! Finite unit-step walks with exact two-circle endpoint closure.

use super::{LayoutError as Error, rings::Work};
use crate::geometry::Point;
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
  let mut moving = vec![false; old.len()];
  for &row in &path[1..path.len() - 1] {
    moving[row] = true;
  }
  guided(
    path,
    old,
    edges,
    linear,
    Guidance {
      moving: &moving,
      guide: old,
      width: 6,
      protected: &vec![false; old.len()],
      free_heading: false,
    },
    work,
  )
}

/// Drawing-only beam hints; none of these preferences can relax source lengths.
pub(super) struct Guidance<'a> {
  /// Path interiors and their attached branches are absent from fixed obstacles.
  pub(super) moving: &'a [bool],
  /// Desired centers used only for ranking incomplete walks.
  pub(super) guide: &'a [Point],
  /// Maximum retained distinct frontier states at each source vertex.
  pub(super) width: usize,
  /// Covalent multiple-bond centers retain their current interior bend exactly.
  pub(super) protected: &'a [bool],
  /// Solve initial heading analytically when the fixed endpoints permit rotation.
  pub(super) free_heading: bool,
}

/// Searches anchored walks while treating movable pendant branches as a unit.
///
/// The caller restores attached branches and checks the complete hard-distance
/// graph before accepting a result. Guides are not coordinates to publish, and
/// the beam never interpolates a stretched final bond to reach its endpoint.
///
/// # Parameters
/// - `path`, `old`: distinct source rows and the immutable complete reference frame.
/// - `edges`, `linear`: full skeleton obstacles and required linear-center flags.
/// - `hints`: equally sized row masks/guides and the bounded beam policy.
/// - `work`: shared allowance, charged before expansions and complete snapshots.
///
/// # Returns
/// Complete snapshots with fixed endpoints and unit path edges. Pendant rows
/// remain in their reference frame until the owner applies its rigid transform.
///
/// # Errors
/// Budget exhaustion abandons the trial family, never publishing a partial walk.
pub(super) fn guided(
  path: &[usize],
  old: &[Point],
  edges: &[[usize; 2]],
  linear: &[bool],
  hints: Guidance<'_>,
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
      let protected_turn;
      let turns: &[f64] = if index == 1 && hints.free_heading {
        &[0.]
      } else if index == 1 {
        &[0., PI / 6., -PI / 6., PI / 3., -PI / 3.]
      } else if linear[path[index - 1]] {
        &[0.]
      } else if hints.protected[path[index - 1]] {
        let row = path[index - 1];
        let a = old[path[index - 2]];
        let b = old[path[index]];
        let p = old[row];
        let cosine = ((a.x - p.x) * (b.x - p.x) + (a.y - p.y) * (b.y - p.y)) / (a.distance(p) * b.distance(p));
        let turn = PI - cosine.clamp(-1., 1.).acos();
        protected_turn = [turn, -turn];
        &protected_turn
      } else {
        &[PI / 3., -PI / 3., PI / 4., -PI / 4.]
      };
      for &turn in turns {
        work.charge(path.len() + old.len() + edges.len())?;
        let direction = base + turn;
        let point = current.translated(direction.cos(), direction.sin());
        let remaining = (path.len() - 1 - index) as f64;
        let reach = if hints.free_heading {
          (start.distance(point) - start.distance(end)).abs()
        } else {
          point.distance(end)
        };
        if reach > remaining + 1e-9 {
          continue;
        }
        if !hints.free_heading && !clear(path, index, &walk.points, point, old, edges, hints.moving) {
          continue;
        }
        if hints.free_heading
          && walk
            .points
            .iter()
            .take(walk.points.len().saturating_sub(1))
            .any(|&p| point.distance(p) < 0.55)
        {
          continue;
        }
        let mut points = walk.points.clone();
        points.push(point);
        let cost = walk.cost
          + 0.02 * point.distance(hints.guide[path[index]]).powi(2)
          + 0.01 * (reach - remaining * if hints.free_heading { 0.5 } else { 0.8 }).powi(2);
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
    next.truncate(hints.width);
    beam = next;
    if beam.is_empty() {
      return Ok(Vec::new());
    }
  }
  let mut result = Vec::new();
  for walk in beam {
    if hints.free_heading {
      result.extend(rotated_closure(path, &walk.points, old, edges, linear, &hints, work)?);
    }
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
      if !clear(path, path.len() - 2, &walk.points, point, old, edges, hints.moving) {
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

/// Closes a complete rigid prefix by solving its global orientation analytically.
///
/// The last movable source vertex lies on two intersecting circles: one around
/// the initial anchor at the prefix's actual endpoint radius, and one unit from
/// the final anchor. Rotating the entire prefix preserves all preceding bends,
/// including a protected carbonyl immediately before the last flexible joint.
fn rotated_closure(
  path: &[usize],
  prefix: &[Point],
  old: &[Point],
  edges: &[[usize; 2]],
  linear: &[bool],
  hints: &Guidance<'_>,
  work: &mut Work,
) -> Result<Vec<Vec<Point>>, Error> {
  let n = path.len();
  if prefix.len() < 2 {
    return Ok(Vec::new());
  }
  let start = old[path[0]];
  let end = old[path[n - 1]];
  let previous = prefix[prefix.len() - 2];
  let current = prefix[prefix.len() - 1];
  let heading = (current.y - previous.y).atan2(current.x - previous.x);
  let row = path[n - 3];
  let a = old[path[n - 4]];
  let b = old[path[n - 2]];
  let center = old[row];
  let cosine = ((a.x - center.x) * (b.x - center.x) + (a.y - center.y) * (b.y - center.y))
    / (a.distance(center) * b.distance(center));
  let turn = if linear[row] {
    0.
  } else if hints.protected[row] {
    PI - cosine.clamp(-1., 1.).acos()
  } else {
    PI / 3.
  };
  let mut result = Vec::new();
  for sign in [-1., 1.] {
    let point = current.translated((heading + sign * turn).cos(), (heading + sign * turn).sin());
    let radius = start.distance(point);
    let distance = start.distance(end);
    if radius < 1e-8 || distance < 1e-8 {
      continue;
    }
    let cosine = (radius * radius + distance * distance - 1.) / (2. * radius * distance);
    if !(-1. - 1e-10..=1. + 1e-10).contains(&cosine) {
      continue;
    }
    let axis = (end.y - start.y).atan2(end.x - start.x);
    let direction = (point.y - start.y).atan2(point.x - start.x);
    for side in [-1., 1.] {
      work.charge(old.len() + n * (old.len() + edges.len()))?;
      let angle = axis + side * cosine.clamp(-1., 1.).acos() - direction;
      let (sin, cos) = angle.sin_cos();
      let mut trial = old.to_vec();
      let mut placed = vec![start];
      let mut valid = true;
      for (i, p) in prefix.iter().copied().chain([point]).enumerate().skip(1) {
        let q = start.translated(
          (p.x - start.x) * cos - (p.y - start.y) * sin,
          (p.x - start.x) * sin + (p.y - start.y) * cos,
        );
        if !clear(path, i, &placed, q, old, edges, hints.moving) {
          valid = false;
          break;
        }
        trial[path[i]] = q;
        placed.push(q);
      }
      if valid
        && angles_valid(path, &trial, linear)
        && path
          .windows(2)
          .all(|pair| (trial[pair[0]].distance(trial[pair[1]]) - 1.).abs() < 1e-8)
      {
        result.push(trial);
      }
    }
  }
  Ok(result)
}

/// Rejects nonincident center/edge contacts against the immutable rest of the block.
fn clear(
  path: &[usize],
  index: usize,
  prefix: &[Point],
  point: Point,
  old: &[Point],
  edges: &[[usize; 2]],
  moving: &[bool],
) -> bool {
  let current = prefix[prefix.len() - 1];
  for (row, &other) in old.iter().enumerate() {
    if moving[row] {
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
    if moving[a] || moving[b] {
      continue;
    }
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

  #[test]
  fn free_heading_closure_should_preserve_a_protected_bend_at_nonlattice_anchors() -> TestResult {
    let old = [
      Point::new(0., 0.),
      Point::new(1., 0.),
      Point::new(1.5, 3_f64.sqrt() / 2.),
      Point::new(-1.8, 0.6),
    ];
    let path = [0, 1, 2, 3];
    let edges = [[0, 1], [1, 2], [2, 3]];
    let ordinary = candidates(&path, &old, &edges, &[false; 4], &mut Work::new(100_000))?;
    assert!(ordinary.is_empty());
    let trials = guided(
      &path,
      &old,
      &edges,
      &[false; 4],
      Guidance {
        moving: &[false, true, true, false],
        guide: &old,
        width: 16,
        protected: &[false, true, false, false],
        free_heading: true,
      },
      &mut Work::new(100_000),
    )?;
    let closed: Vec<_> = trials
      .into_iter()
      .filter(|points| (points[0].distance(points[2]) - 3_f64.sqrt()).abs() < 1e-8)
      .collect();
    assert!(!closed.is_empty());
    for trial in closed {
      assert_eq!(trial[0], old[0]);
      assert_eq!(trial[3], old[3]);
      assert!(
        path
          .windows(2)
          .all(|pair| (trial[pair[0]].distance(trial[pair[1]]) - 1.).abs() < 1e-8)
      );
    }
    Ok(())
  }
}
