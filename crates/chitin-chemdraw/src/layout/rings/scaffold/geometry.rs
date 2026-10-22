//! Joint port placement, genuine-atom path expansion and guarded final closure.

use super::{Error, PI, Point, Skeleton, TAU, Work};
use crate::layout::quality;

/// Positions the small reduced graph rather than every chemical atom at once.
fn junctions(model: &Skeleton, seed: usize, work: &mut Work) -> Result<Vec<Point>, Error> {
  let n = model.fragments.len();
  let radius = (model.edges.len() as f64).sqrt() * 0.65;
  let proposed = seed * 2 + 1;
  let stride = if proposed > 1 && n.is_multiple_of(proposed) {
    1
  } else {
    proposed
  };
  let mut centers: Vec<_> = (0..n)
    .map(|i| {
      let angle = TAU * ((i * stride) % n) as f64 / n as f64;
      Point::new(radius * angle.cos(), radius * angle.sin())
    })
    .collect();
  for _ in 0..400 {
    work.charge(n * n + model.paths.len() * 8)?;
    let mut forces = vec![Point::new(0., 0.); n];
    for a in 0..n {
      for b in a + 1..n {
        let distance = centers[a].distance(centers[b]).max(0.05);
        let force = 0.8 / distance.powi(2);
        let dx = (centers[a].x - centers[b].x) / distance * force;
        let dy = (centers[a].y - centers[b].y) / distance * force;
        forces[a] = forces[a].translated(dx, dy);
        forces[b] = forces[b].translated(-dx, -dy);
      }
    }
    for path in &model.paths {
      let [a, b] = path.ends;
      let steps = (path.atoms.len() - 1) as f64;
      let target = (steps.sqrt() * 1.5).min(steps * 0.65).max(1.);
      let distance = centers[a].distance(centers[b]).max(0.05);
      let factor = (distance - target) / distance * 0.15;
      let dx = (centers[b].x - centers[a].x) * factor;
      let dy = (centers[b].y - centers[a].y) * factor;
      forces[a] = forces[a].translated(dx, dy);
      forces[b] = forces[b].translated(-dx, -dy);
    }
    for (point, force) in centers.iter_mut().zip(forces) {
      *point = point.translated(force.x.clamp(-0.3, 0.3), force.y.clamp(-0.3, 0.3));
    }
  }
  Ok(centers)
}

/// Expands immutable fragment offsets into their jointly movable port locations.
fn ports(model: &Skeleton, centers: &[Point], n: usize, seed: usize) -> Vec<Point> {
  let mut points = vec![Point::new(0., 0.); n];
  for (i, fragment) in model.fragments.iter().enumerate() {
    let angle = (seed + i) as f64 * PI / 3.;
    for &(row, offset) in &fragment.atoms {
      points[row] = centers[i].translated(
        offset.x * angle.cos() - offset.y * angle.sin(),
        offset.x * angle.sin() + offset.y * angle.cos(),
      );
    }
  }
  points
}

/// Keeps every path reachable while moving whole islands, never their port atoms alone.
fn reachable(
  model: &Skeleton,
  centers: &mut [Point],
  n: usize,
  seed: usize,
  work: &mut Work,
) -> Result<Vec<Point>, Error> {
  for _ in 0..80 {
    work.charge(model.paths.len() * 8 + n)?;
    let points = ports(model, centers, n, seed);
    let mut complete = true;
    for path in &model.paths {
      let a = points[path.atoms[0]];
      let b = points[path.atoms[path.atoms.len() - 1]];
      let distance = a.distance(b);
      let maximum = (path.atoms.len() - 1) as f64 * 0.85;
      if distance > maximum {
        complete = false;
        let factor = (distance - maximum) / distance * 0.5;
        let dx = (b.x - a.x) * factor;
        let dy = (b.y - a.y) * factor;
        centers[path.ends[0]] = centers[path.ends[0]].translated(dx, dy);
        centers[path.ends[1]] = centers[path.ends[1]].translated(-dx, -dy);
      }
    }
    if complete {
      return Ok(points);
    }
  }
  Ok(ports(model, centers, n, seed))
}

/// Samples one bowed path with alternating skeletal turns at actual atom sites.
fn bow(a: Point, b: Point, count: usize, amplitude: f64, sign: f64) -> Vec<Point> {
  let distance = a.distance(b).max(1e-8);
  let normal = Point::new(-(b.y - a.y) / distance, (b.x - a.x) / distance);
  (0..count)
    .map(|i| {
      let t = i as f64 / (count - 1) as f64;
      let zig = if i == 0 || i + 1 == count {
        0.
      } else if i % 2 == 0 {
        0.25
      } else {
        -0.25
      };
      let shift = sign * (amplitude * (PI * t).sin() + zig);
      Point::new(
        a.x + (b.x - a.x) * t + normal.x * shift,
        a.y + (b.y - a.y) * t + normal.y * shift,
      )
    })
    .collect()
}

/// Expands paths in independent lanes before any exact distance projection.
fn expand(model: &Skeleton, points: &mut Vec<Point>, seed: usize, work: &mut Work) -> Result<(), Error> {
  let mut placed = model.owned.clone();
  let mut order: Vec<_> = (0..model.paths.len()).collect();
  order.sort_by_key(|&i| (model.paths[i].atoms.len(), i));
  if seed % 2 == 1 {
    order.reverse();
  }
  for index in order {
    let path = &model.paths[index];
    let a = points[path.atoms[0]];
    let b = points[path.atoms[path.atoms.len() - 1]];
    let mut best = None;
    for sign in [-1., 1.] {
      let (mut low, mut high) = (0., path.atoms.len() as f64);
      for _ in 0..24 {
        work.charge(path.atoms.len())?;
        let middle = (low + high) / 2.;
        let candidate = bow(a, b, path.atoms.len(), middle, sign);
        let length: f64 = candidate.windows(2).map(|pair| pair[0].distance(pair[1])).sum();
        if length > (path.atoms.len() - 1) as f64 {
          high = middle;
        } else {
          low = middle;
        }
      }
      let candidate = bow(a, b, path.atoms.len(), (low + high) / 2., sign);
      let mut complete = points.clone();
      for (&row, &point) in path.atoms.iter().zip(&candidate) {
        complete[row] = point;
      }
      let mut active = placed.clone();
      for &row in &path.atoms {
        active[row] = true;
      }
      let active_edges: Vec<_> = model
        .edges
        .iter()
        .copied()
        .filter(|&[a, b]| active[a] && active[b])
        .collect();
      let mut cost = 0.;
      for &row in &path.atoms[1..path.atoms.len() - 1] {
        work.charge(points.len() + active_edges.len())?;
        for (other, &point) in complete.iter().enumerate() {
          if other != row && active[other] {
            cost += (0.9 - complete[row].distance(point)).max(0.).powi(2) * 4.;
          }
        }
        for &[a, b] in &active_edges {
          if a != row && b != row {
            cost += (0.6 - quality::segment_distance(complete[row], complete[a], complete[b]))
              .max(0.)
              .powi(2)
              * 6.;
          }
        }
      }
      if best.as_ref().is_none_or(|(old, _)| cost < *old) {
        best = Some((cost, complete));
      }
    }
    if let Some((_, candidate)) = best {
      *points = candidate;
    }
    for &row in &path.atoms {
      placed[row] = true;
    }
  }
  Ok(())
}

/// Projects one source-edge or rigid-fragment chord equation symmetrically.
fn project(points: &mut [Point], [a, b]: [usize; 2], target: f64, strength: f64) {
  let distance = points[a].distance(points[b]);
  if distance < 1e-10 {
    return;
  }
  let factor = (distance - target) / distance * 0.5 * strength;
  let dx = (points[b].x - points[a].x) * factor;
  let dy = (points[b].y - points[a].y) * factor;
  points[a] = points[a].translated(dx, dy);
  points[b] = points[b].translated(-dx, -dy);
}

/// Couples all path endpoints while protecting short-ring shape and local turns.
fn close(model: &Skeleton, points: &mut [Point], work: &mut Work) -> Result<bool, Error> {
  for iteration in 0..2400 {
    work.charge(model.edges.len() + model.chords.len() + points.len())?;
    if iteration < 240 {
      for path in &model.paths {
        for window in path.atoms.windows(3) {
          project(points, [window[0], window[2]], 3_f64.sqrt(), 0.08);
        }
      }
      if iteration % 12 == 0 {
        work.charge(points.len() * points.len() + points.len() * model.edges.len())?;
        for a in 0..points.len() {
          for b in a + 1..points.len() {
            if points[a].distance(points[b]) < 0.8 {
              project(points, [a, b], 0.8, 0.2);
            }
          }
          for &[b, c] in &model.edges {
            if a == b || a == c {
              continue;
            }
            let length = points[b].distance(points[c]);
            if length < 1e-8 {
              continue;
            }
            let t = (((points[a].x - points[b].x) * (points[c].x - points[b].x)
              + (points[a].y - points[b].y) * (points[c].y - points[b].y))
              / (length * length))
              .clamp(0., 1.);
            let nearest = Point::new(
              points[b].x + (points[c].x - points[b].x) * t,
              points[b].y + (points[c].y - points[b].y) * t,
            );
            let distance = points[a].distance(nearest);
            if distance > 1e-8 && distance < 0.45 {
              let factor = (0.45 - distance) / distance * 0.12;
              let dx = (points[a].x - nearest.x) * factor;
              let dy = (points[a].y - nearest.y) * factor;
              points[a] = points[a].translated(dx, dy);
              points[b] = points[b].translated(-dx * (1. - t), -dy * (1. - t));
              points[c] = points[c].translated(-dx * t, -dy * t);
            }
          }
        }
      }
    }
    for &[a, b] in &model.edges {
      project(points, [a, b], 1., 1.);
    }
    for &(pair, target) in &model.chords {
      project(points, pair, target, 1.);
    }
    if iteration >= 240
      && iteration % 8 == 0
      && model
        .edges
        .iter()
        .all(|&[a, b]| (points[a].distance(points[b]) - 1.).abs() < 1e-8)
      && model
        .chords
        .iter()
        .all(|&([a, b], target)| (points[a].distance(points[b]) - target).abs() < 1e-8)
    {
      return Ok(true);
    }
  }
  Ok(false)
}

/// Ranks complete source-edge-closed alternatives, not the first feasible sketch.
pub(super) fn candidates(model: &Skeleton, work: &mut Work) -> Result<Vec<Vec<Point>>, Error> {
  let n = model.owned.len();
  let mut result = Vec::new();
  for seed in 0..3 {
    let trial = (|| {
      let mut centers = junctions(model, seed, work)?;
      let mut points = reachable(model, &mut centers, n, seed, work)?;
      expand(model, &mut points, seed, work)?;
      if !close(model, &mut points, work)? {
        return Ok(None);
      }
      let quality = work.measure(&points, &model.edges)?;
      let contacts = quality.crowds.len() + quality.crossings.len() + quality.atom_edges.len();
      Ok(Some((contacts, quality.cost, points)))
    })();
    match trial {
      Ok(Some(candidate)) => result.push(candidate),
      Ok(None) => {}
      Err(Error::RingSearchBudgetExceeded) if !result.is_empty() => break,
      Err(error) => return Err(error),
    }
  }
  result.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
  Ok(result.into_iter().map(|(_, _, points)| points).collect())
}
