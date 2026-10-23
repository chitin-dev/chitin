//! Continuous unit-bond closure of discrete, source-constrained turn sequences.
//!
//! Relative turn signs encode ring-neighbor stereo. Reachable winding sums guide
//! deterministic starts; a minimum-norm angular solve removes endpoint error.
//! No source vertex is added and no chemical bond is stretched to close a ring.

use std::{
  collections::BTreeSet,
  f64::consts::{PI, TAU},
};

use chitin_chem::DoubleBondRelation;

use super::{Error, Point, Requirement, Work, caps};

/// Coupled turn signs, with source-local vertex rows and relative parity.
type Group = Vec<(usize, i32)>;

/// Finds connected parity groups without assigning unknown stereochemistry.
fn groups(n: usize, requirements: &[Requirement], work: &mut Work) -> Result<Option<Vec<Group>>, Error> {
  work.charge(n + 2 * requirements.len())?;
  let mut links = vec![Vec::new(); n];
  for requirement in requirements {
    let [a, b, _, _] = requirement.slots;
    let parity = if requirement.expected == DoubleBondRelation::SameSide {
      1
    } else {
      -1
    };
    links[a].push((b, parity));
    links[b].push((a, parity));
  }
  let mut signs = vec![0; n];
  let mut groups = Vec::new();
  for root in 0..n {
    if signs[root] != 0 {
      continue;
    }
    signs[root] = 1;
    let mut stack = vec![root];
    let mut group = Vec::new();
    while let Some(row) = stack.pop() {
      work.charge(1 + links[row].len())?;
      group.push((row, signs[row]));
      for &(other, parity) in &links[row] {
        let expected = signs[row] * parity;
        if signs[other] == 0 {
          signs[other] = expected;
          stack.push(other);
        } else if signs[other] != expected {
          return Ok(None);
        }
      }
    }
    groups.push(group);
  }
  Ok(Some(groups))
}

/// Checks visible bends and the discrete sign chosen for every source vertex.
fn valid_turns(headings: &[f64], signs: &[i32]) -> bool {
  (0..headings.len()).all(|i| {
    let previous = headings[(i + headings.len() - 1) % headings.len()];
    let turn = (headings[i] - previous + PI).rem_euclid(TAU) - PI;
    turn * f64::from(signs[i]) >= PI / 6. - 1e-9 && turn.abs() <= PI / 2. + 1e-9
  })
}

/// Solves endpoint closure by changing headings, preserving exact unit lengths.
fn close(signs: &[i32], fixed: &[Option<f64>], work: &mut Work) -> Result<Option<Vec<Point>>, Error> {
  let n = signs.len();
  let free = fixed.iter().filter(|angle| angle.is_none()).count();
  if free == 0 {
    return Ok(None);
  }
  let initial: Vec<_> = signs
    .iter()
    .zip(fixed)
    .map(|(&sign, &angle)| angle.unwrap_or(f64::from(sign) * PI / 3.))
    .collect();
  let correction = (TAU - initial.iter().sum::<f64>()) / free as f64;
  let mut headings = vec![0.; n];
  for i in 1..n {
    headings[i] = headings[i - 1] + initial[i] + if fixed[i].is_none() { correction } else { 0. };
  }
  let valid = |angles: &[f64]| {
    if fixed.iter().all(Option::is_none) {
      return valid_turns(angles, signs);
    }
    (0..n).all(|i| {
      let turn = (angles[i] - angles[(i + n - 1) % n] + PI).rem_euclid(TAU) - PI;
      fixed[i].map_or_else(
        || turn * f64::from(signs[i]) >= PI / 6. - 1e-9 && turn.abs() <= PI / 2. + 1e-9,
        |angle| (turn - angle).abs() < 1e-8,
      )
    })
  };
  if !valid(&headings) {
    return Ok(None);
  }
  let movable = heading_groups(fixed);
  let residual = |angles: &[f64]| {
    angles
      .iter()
      .fold(Point::new(0., 0.), |p, &angle| p.translated(angle.cos(), angle.sin()))
  };
  for _ in 0..64 {
    work.charge(n)?;
    let error = residual(&headings);
    let norm = error.x.hypot(error.y);
    if norm < 1e-10 {
      let mut point = Point::new(0., 0.);
      let points: Vec<_> = headings
        .iter()
        .map(|&angle| {
          let previous = point;
          point = point.translated(angle.cos(), angle.sin());
          previous
        })
        .collect();
      return Ok(Some(points));
    }
    let (mut xx, mut xy, mut yy) = (0., 0., 0.);
    let derivatives: Vec<_> = movable
      .iter()
      .map(|group| {
        group
          .iter()
          .fold((0., 0.), |(x, y), &i| (x - headings[i].sin(), y + headings[i].cos()))
      })
      .collect();
    for &(x, y) in &derivatives {
      xx += x * x;
      xy += x * y;
      yy += y * y;
    }
    let determinant: f64 = xx * yy - xy * xy;
    if determinant.abs() < 1e-12 {
      return Ok(None);
    }
    let u = (yy * error.x - xy * error.y) / determinant;
    let v = (xx * error.y - xy * error.x) / determinant;
    let mut accepted = false;
    for power in 0..8 {
      work.charge(3 * n)?;
      let factor = 0.5_f64.powi(power);
      let mut trial = headings.clone();
      for (group, &(x, y)) in movable.iter().zip(&derivatives) {
        let change = -factor * (x * u + y * v);
        for &i in group {
          trial[i] += change;
        }
      }
      if valid(&trial) && residual(&trial).distance(Point::new(0., 0.)) < norm {
        headings = trial;
        accepted = true;
        break;
      }
    }
    if !accepted {
      return Ok(None);
    }
  }
  Ok(None)
}

/// Couples headings across fixed bends, excluding the group fixing rigid rotation.
fn heading_groups(fixed: &[Option<f64>]) -> Vec<Vec<usize>> {
  let n = fixed.len();
  let mut seen = vec![false; n];
  let mut groups = Vec::new();
  for root in 0..n {
    if seen[root] {
      continue;
    }
    seen[root] = true;
    let mut group = Vec::new();
    let mut pending = vec![root];
    while let Some(i) = pending.pop() {
      group.push(i);
      for (other, joined) in [
        ((i + n - 1) % n, fixed[i].is_some()),
        ((i + 1) % n, fixed[(i + 1) % n].is_some()),
      ] {
        if joined && !seen[other] {
          seen[other] = true;
          pending.push(other);
        }
      }
    }
    if !group.contains(&0) {
      groups.push(group);
    }
  }
  groups
}

/// Generates deterministic parity assignments near one positive boundary winding.
///
/// # Returns
/// Up to six simple closed outlines, with all supplied reference sides verified.
/// Failed angular solves and self-intersecting closures are discarded completely.
///
/// # Errors
/// Topology discovery charges the shared allowance. An exhausted optional start
/// retains previously completed candidates, never an unfinished walk.
pub(super) fn outlines(n: usize, requirements: &[Requirement], work: &mut Work) -> Result<Vec<Vec<Point>>, Error> {
  fixed_outlines(n, requirements, &[], work)
}

/// Searches complete outlines while preserving exact signed shared-path bends.
///
/// # Returns
/// Source-constrained, simple closures; inconsistent fixed turns yield no poses.
///
/// # Errors
/// Work accounting includes rejected parity assignments and angular projections.
pub(super) fn fixed_outlines(
  n: usize,
  requirements: &[Requirement],
  turns: &[(usize, f64)],
  work: &mut Work,
) -> Result<Vec<Vec<Point>>, Error> {
  let mut fixed = vec![None; n];
  for &(row, angle) in turns {
    if row >= n || !angle.is_finite() || angle.abs() < 1e-9 || angle.abs() >= PI {
      return Ok(Vec::new());
    }
    if fixed[row].is_some_and(|old: f64| (old - angle).abs() > 1e-9) {
      return Ok(Vec::new());
    }
    fixed[row] = Some(angle);
  }
  let Some(groups) = groups(n, requirements, work)? else {
    return Ok(Vec::new());
  };
  let mut orientations = Vec::with_capacity(groups.len());
  for group in &groups {
    let mut orientation = None;
    for &(row, sign) in group {
      if let Some(angle) = fixed[row] {
        let required = if angle > 0. { sign } else { -sign };
        if orientation.is_some_and(|old| old != required) {
          return Ok(Vec::new());
        }
        orientation = Some(required);
      }
    }
    orientations.push(orientation);
  }
  let weights: Vec<i32> = groups
    .iter()
    .map(|group| group.iter().map(|&(_, sign)| sign).sum())
    .collect();
  let mut reachable = vec![BTreeSet::new(); groups.len() + 1];
  reachable[groups.len()].insert(0);
  for i in (0..groups.len()).rev() {
    work.charge(2 * reachable[i + 1].len())?;
    let weight = weights[i];
    reachable[i] = reachable[i + 1]
      .iter()
      .flat_map(|&value| {
        [1, -1]
          .into_iter()
          .filter(|&sign| orientations[i].is_none_or(|fixed| fixed == sign))
          .map(move |sign| value + sign * weight)
      })
      .collect();
  }
  let Some(&target) = reachable[0]
    .iter()
    .filter(|&&turn| turn > 0)
    .min_by_key(|&&turn| ((turn - 6).abs(), turn))
  else {
    return Ok(Vec::new());
  };
  let mut candidates: Vec<(f64, Vec<Point>)> = Vec::new();
  for variant in 0_u64..192 {
    if work.remaining() < 4 * n {
      break;
    }
    let proposal = work.optional(0, |work| {
      work.charge(n + groups.len())?;
      let mut signs = vec![0; n];
      let mut remaining = target;
      let mut state = variant.wrapping_add(1).wrapping_mul(0x9e3779b97f4a7c15);
      for (i, group) in groups.iter().enumerate() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let preferred = orientations[i].unwrap_or(if variant == 0 || state & 1 == 0 { 1 } else { -1 });
        let orientation = if reachable[i + 1].contains(&(remaining - preferred * weights[i])) {
          preferred
        } else {
          -preferred
        };
        remaining -= orientation * weights[i];
        for &(row, sign) in group {
          signs[row] = sign * orientation;
        }
      }
      let Some(points) = close(&signs, &fixed, work)? else {
        return Ok(None);
      };
      if !caps::valid(&points, work)?
        || requirements
          .iter()
          .any(|r| crate::stereo_geometry::relation(r.slots.map(|row| points[row])) != Some(r.expected))
      {
        return Ok(None);
      }
      let cost: f64 = (0..n)
        .map(|i| {
          let a = points[(i + n - 1) % n];
          let b = points[i];
          let c = points[(i + 1) % n];
          let turn = ((b.x - a.x) * (c.y - b.y) - (b.y - a.y) * (c.x - b.x))
            .atan2((b.x - a.x) * (c.x - b.x) + (b.y - a.y) * (c.y - b.y));
          (turn.abs() - PI / 3.).powi(2)
        })
        .sum();
      Ok(Some((cost, points)))
    })?;
    let Some(proposal) = proposal else { break };
    if let Some((cost, points)) = proposal
      && !candidates
        .iter()
        .any(|(_, old)| old.iter().zip(&points).all(|(a, b)| a.distance(*b) < 1e-6))
    {
      candidates.push((cost, points));
      candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
      candidates.truncate(6);
    }
  }
  Ok(candidates.into_iter().map(|(_, points)| points).collect())
}

#[cfg(test)]
mod tests {
  use super::*;

  type TestResult = Result<(), Box<dyn std::error::Error>>;

  #[test]
  fn fixed_bends_across_the_closing_edge_should_remain_exact() -> TestResult {
    let fixed = [(0, -TAU / 7.), (25, -TAU / 7.)];
    let candidates = fixed_outlines(26, &[], &fixed, &mut Work::new(1_500_000))?;
    assert!(!candidates.is_empty());
    for points in candidates {
      for &(i, expected) in &fixed {
        let [a, b, c] = [points[(i + 25) % 26], points[i], points[(i + 1) % 26]];
        let actual = ((b.x - a.x) * (c.y - b.y) - (b.y - a.y) * (c.x - b.x))
          .atan2((b.x - a.x) * (c.x - b.x) + (b.y - a.y) * (c.y - b.y));
        assert!((actual - expected).abs() < 1e-8);
      }
      assert!(caps::valid(&points, &mut Work::new(100_000))?);
    }
    Ok(())
  }

  #[test]
  fn contradictory_fixed_turns_should_not_publish_a_candidate() -> TestResult {
    assert!(fixed_outlines(14, &[], &[(2, PI / 3.), (2, -PI / 3.)], &mut Work::new(100_000))?.is_empty());
    Ok(())
  }

  #[test]
  fn odd_turn_sequences_should_close_without_stretching_the_final_edge() -> TestResult {
    let signs: Vec<_> = (0..21).map(|i| if i % 3 == 1 { -1 } else { 1 }).collect();
    let points = close(&signs, &vec![None; signs.len()], &mut Work::new(100_000))?.ok_or("no closure")?;
    assert_eq!(points.len(), 21);
    assert!(caps::valid(&points, &mut Work::new(100_000))?);
    Ok(())
  }

  #[test]
  fn angular_projection_should_remove_a_nonperiodic_endpoint_residual() -> TestResult {
    let mut signs: Vec<_> = (0..21).map(|i| if i % 3 == 1 { -1 } else { 1 }).collect();
    signs.swap(0, 1);
    let points = close(&signs, &vec![None; signs.len()], &mut Work::new(100_000))?.ok_or("no closure")?;
    for i in 0..points.len() {
      assert!((points[i].distance(points[(i + 1) % points.len()]) - 1.).abs() < 1e-8);
    }
    Ok(())
  }

  #[test]
  fn interrupted_angular_projection_should_not_return_a_partial_polygon() {
    assert!(matches!(close(&[1; 21], &[None; 21], &mut Work::new(0)), Ok(None)));
    let signs: Vec<_> = (0..21).map(|i| if i % 3 == 1 { -1 } else { 1 }).collect();
    assert!(matches!(
      close(&signs, &[None; 21], &mut Work::new(0)),
      Err(Error::RingSearchBudgetExceeded)
    ));
  }
}
