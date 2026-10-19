//! Nonlattice macrocycle outlines with exact unit edges and bounded turns.
//!
//! A regular end cap changes perimeter parity; hexagonal expansions add four
//! vertices at a time. Only the outer walk is retained: auxiliary face edges
//! are construction geometry, never extra bonds in the chemical graph.

use std::{
  collections::{BTreeMap, BTreeSet},
  f64::consts::{PI, TAU},
};

use super::super::geometry::separated;
use super::{Error, Point, Work};

/// Constructs straight and alternating-strip candidates of the requested size.
///
/// # Parameters
/// * `n`: macrocycle size of at least nine, irrespective of parity.
/// * `work`: shared allowance for all construction and intersection checks.
///
/// # Returns
/// Complete, counterclockwise, simple unit-edge outlines. Five- and seven-sided
/// end caps cover odd sizes; six- and eight-sided caps diversify even sizes.
/// No closing edge is fitted and no near-linear parity shortcut is inserted.
///
/// # Errors
/// Propagates work exhaustion and unrepresentable intermediate coordinates.
pub(super) fn generate(n: usize, work: &mut Work) -> Result<Vec<Vec<Point>>, Error> {
  let cap = 5 + (n - 5) % 4;
  let steps = (n - cap) / 4;
  let radius = 1. / (2. * (PI / cap as f64).sin());
  let mut result = Vec::new();
  for alternating in [false, true] {
    work.charge(cap)?;
    let mut points: Vec<_> = (0..cap)
      .map(|i| {
        let angle = TAU * i as f64 / cap as f64;
        Point::new(radius * angle.cos(), radius * angle.sin())
      })
      .collect();
    let mut edge = 0;
    for step in 0..steps {
      points = expand(&points, edge, work)?;
      // The opposite new edge continues straight. Alternating the two
      // flanking edges makes a different turn pattern without closing a spiral.
      edge += if !alternating {
        2
      } else if step.is_multiple_of(2) {
        1
      } else {
        3
      };
    }
    if valid(&points, work)? {
      result.push(points);
    }
  }
  Ok(result)
}

/// Builds a bounded family of mixed-turn closed walks for short macrocycles.
///
/// # Parameters
/// - `n`: the exact source-vertex count; sizes outside 9–24 use the strip family.
/// - `work`: shared allowance, including every rejected geometric construction.
///
/// # Returns
/// At most six distinct outlines. Five-, six-, seven- and eight-sided auxiliary
/// faces contribute turns and parity, but only their outer boundary survives.
/// The graph never gains auxiliary vertices, edges, or implied rings.
///
/// # Errors
/// Work exhaustion propagates before an unvalidated walk can escape.
pub(super) fn mixed(n: usize, work: &mut Work) -> Result<Vec<Vec<Point>>, Error> {
  if !(9..=24).contains(&n) {
    return Ok(Vec::new());
  }
  let mut levels = BTreeMap::<usize, Vec<Vec<Point>>>::new();
  let mut signatures = BTreeMap::<usize, BTreeSet<Vec<i32>>>::new();
  for size in 5..=8 {
    work.charge(size)?;
    let radius = 1. / (2. * (PI / size as f64).sin());
    levels.insert(
      size,
      vec![
        (0..size)
          .map(|i| {
            let angle = TAU * i as f64 / size as f64;
            Point::new(radius * angle.cos(), radius * angle.sin())
          })
          .collect(),
      ],
    );
  }
  for size in 5..n {
    let Some(outlines) = levels.remove(&size) else {
      continue;
    };
    for outline in outlines {
      for sides in 5..=8 {
        let count = size + sides - 2;
        if count > n || levels.get(&count).is_some_and(|rows| rows.len() >= 6) {
          continue;
        }
        for edge in [0, size / 3, 2 * size / 3] {
          let candidate = attach(&outline, edge, sides, work)?;
          if !valid(&candidate, work)? {
            continue;
          }
          work.charge(count * count)?;
          if (0..count).any(|i| candidate[i + 1..].iter().any(|&p| candidate[i].distance(p) < 0.55)) {
            continue;
          }
          let turns: Vec<_> = (0..count)
            .map(|i| {
              let a = candidate[(i + count - 1) % count];
              let c = candidate[i];
              let b = candidate[(i + 1) % count];
              let cross = (c.x - a.x) * (b.y - c.y) - (c.y - a.y) * (b.x - c.x);
              let dot = (c.x - a.x) * (b.x - c.x) + (c.y - a.y) * (b.y - c.y);
              (cross.atan2(dot) * 1e5).round() as i32
            })
            .collect();
          let key = (0..count)
            .map(|offset| (0..count).map(|i| turns[(i + offset) % count]).collect::<Vec<_>>())
            .min()
            .unwrap_or_default();
          if signatures.entry(count).or_default().insert(key) {
            let rows = levels.entry(count).or_default();
            rows.push(candidate);
            if rows.len() >= 6 {
              break;
            }
          }
        }
      }
    }
  }
  Ok(levels.remove(&n).unwrap_or_default())
}

/// Replaces one boundary edge by an exterior regular-face arc of unit bonds.
fn attach(points: &[Point], edge: usize, sides: usize, work: &mut Work) -> Result<Vec<Point>, Error> {
  work.charge(points.len() + sides)?;
  let a = points[edge];
  let b = points[(edge + 1) % points.len()];
  let axis = Point::new(b.x - a.x, b.y - a.y);
  let mut result = Vec::with_capacity(points.len() + sides - 2);
  result.extend_from_slice(&points[..=edge]);
  let mut point = a;
  for i in 0..sides - 2 {
    let angle = -PI + TAU * (i + 1) as f64 / sides as f64;
    let (sin, cos) = angle.sin_cos();
    point = point.translated(axis.x * cos - axis.y * sin, axis.x * sin + axis.y * cos);
    result.push(point);
  }
  result.extend_from_slice(&points[edge + 1..]);
  Ok(result)
}

/// Replaces one unit boundary edge by the five-edge exterior of a hexagon.
///
/// The old endpoints are retained exactly. Rotations of the old unit axis
/// construct four new vertices, so the shared edge disappears from the boundary
/// but no molecular edge is stretched. The resulting walk is validated later.
fn expand(points: &[Point], edge: usize, work: &mut Work) -> Result<Vec<Point>, Error> {
  work.charge(points.len() + 4)?;
  let a = points[edge];
  let b = points[(edge + 1) % points.len()];
  let axis = Point::new(b.x - a.x, b.y - a.y);
  let mut result = Vec::with_capacity(points.len() + 4);
  result.extend_from_slice(&points[..=edge]);
  let mut point = a;
  for turn in [-2., -1., 0., 1.] {
    let (sin, cos) = (turn * PI / 3.).sin_cos();
    point = point.translated(axis.x * cos - axis.y * sin, axis.x * sin + axis.y * cos);
    result.push(point);
  }
  result.extend_from_slice(&points[edge + 1..]);
  Ok(result)
}

/// Rejects malformed, crossing, pinched, or near-linear construction walks.
///
/// Tests include the closing edge and all nonincident edge pairs. The work is
/// quadratic but charged before each scan; an interrupted scan cannot publish
/// a partially validated candidate. This is geometric validity, not a label or
/// general chemical-conformation guarantee.
pub(super) fn valid(points: &[Point], work: &mut Work) -> Result<bool, Error> {
  let n = points.len();
  if n < 3 {
    return Ok(false);
  }
  for i in 0..n {
    work.charge(1)?;
    let a = points[i];
    let b = points[(i + 1) % n];
    let prev = points[(i + n - 1) % n];
    if !a.is_finite() || (a.distance(b) - 1.).abs() > 1e-8 {
      return Ok(false);
    }
    let cosine = (prev.x - a.x) * (b.x - a.x) + (prev.y - a.y) * (b.y - a.y);
    // Unit edges make this the actual cosine. Keep visible bends and avoid
    // acute needle corners; the cap families stay between 105 and 135 degrees.
    if !(-0.9..=0.5).contains(&cosine) {
      return Ok(false);
    }
    for j in i + 1..n {
      if j == i + 1 || (i == 0 && j == n - 1) {
        continue;
      }
      work.charge(1)?;
      let c = points[j];
      let d = points[(j + 1) % n];
      // Endpoint/segment checks also catch collinear overlap and tangencies,
      // which a proper-crossing test intentionally excludes.
      if !separated(a, b, c, d) {
        return Ok(false);
      }
    }
  }
  Ok(true)
}

#[cfg(test)]
mod tests {
  use super::*;
  type TestResult = Result<(), Box<dyn std::error::Error>>;

  #[test]
  fn mixed_turn_walks_should_close_both_parities_without_near_straight_vertices() -> TestResult {
    for n in 9..=24 {
      let mut work = Work::new(2_000_000);
      let outlines = mixed(n, &mut work)?;
      assert!(!outlines.is_empty(), "size {n}");
      assert!(outlines.len() <= 6);
      for points in outlines {
        assert_eq!(points.len(), n);
        assert!(valid(&points, &mut work)?);
        assert!((0..n).any(|i| {
          let a = points[(i + n - 1) % n];
          let c = points[i];
          let b = points[(i + 1) % n];
          (c.x - a.x) * (b.y - c.y) - (c.y - a.y) * (b.x - c.x) < 0.
        }));
      }
    }
    Ok(())
  }

  #[test]
  fn end_caps_should_cover_both_parities_with_exact_closed_edges() -> TestResult {
    for n in 12..=65 {
      let candidates = generate(n, &mut Work::new(2_000_000))?;
      assert!(!candidates.is_empty(), "size {n}");
      for points in candidates {
        assert_eq!(points.len(), n);
        assert!(valid(&points, &mut Work::new(2_000_000))?);
      }
    }
    Ok(())
  }

  #[test]
  fn validation_should_reject_crossings_tangencies_and_bad_closures() -> TestResult {
    for points in [
      vec![
        Point::new(0., 0.),
        Point::new(1., 1.),
        Point::new(0., 1.),
        Point::new(1., 0.),
      ],
      vec![
        Point::new(0., 0.),
        Point::new(1., 0.),
        Point::new(2., 0.),
        Point::new(1., 0.),
      ],
      vec![
        Point::new(0., 0.),
        Point::new(1., 0.),
        Point::new(1., 1.),
        Point::new(0., 2.),
      ],
    ] {
      assert!(!valid(&points, &mut Work::new(1000))?);
    }
    Ok(())
  }
}
