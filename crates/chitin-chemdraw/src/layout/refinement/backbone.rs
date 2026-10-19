//! Bounded, component-local routing of long backbones through rigid ring islands.
//!
//! A bridge reflection moves its entire downstream side, never individual ring
//! vertices. Lengths and internal angles survive; a separate saturated-carbon
//! port repair permits at most thirty degrees of host-angle adjustment. No
//! chemical facts, perspective orders, or artificial bend vertices are introduced.

use super::{Budget, Model, Point, Quality};
use crate::{geometry::Bounds, layout::LayoutOptions};
use std::{
  collections::{BTreeSet, VecDeque},
  f64::consts::PI,
};

/// One genuine source bridge on the principal shortest-path spine.
struct Step {
  /// Index of the downstream endpoint in the ordered spine.
  index: usize,
  /// All atoms on its downstream side, including the fixed hinge itself.
  side: Vec<usize>,
  /// Atoms fixed before this cut.
  fixed: Vec<usize>,
  /// Atoms committed before the next cut, including off-spine substituents.
  released: Vec<usize>,
  /// Fixed-prefix edges, with real source endpoints.
  prefix_edges: Vec<[usize; 2]>,
  /// Newly committed edges checked against the fixed prefix.
  joining_edges: Vec<[usize; 2]>,
}

/// Complete candidate coordinates and the current row of the guide, not chemistry.
struct Pose {
  /// Complete component snapshot, including substituents outside the spine.
  points: Vec<Point>,
  /// Accumulated guide deviation and committed-prefix contact penalties.
  cost: f64,
  /// Horizontal travel direction on the current row.
  east: bool,
  /// Whether the guide is currently turning toward the next lower row.
  descending: bool,
  /// Entry height used to enforce clearance between neighboring rows.
  row_y: f64,
}

/// Tries finite serpentine guides after ordinary collision repair.
///
/// # Parameters
/// - `model`, `points`: one complete, matching component and its current pose.
/// - `options`: shared spacing policy; disabled refinement never calls this stage.
/// - `budget`: the dedicated routing allowance, shared across all components
///   and complete-layout candidates, never reset per guide.
///
/// # Behavior
/// Only large components can participate; global routing additionally requires
/// a long topological spine and many movable bridges.
/// Ordinary collision repair has its own allowance. An interrupted trial has
/// no effect; only complete stereo-valid poses with no extra skeleton conflicts
/// and a better compactness score can replace the original. Perspective cores
/// retain their authored orientation and are excluded entirely.
pub(super) fn route(model: &Model, points: &mut Vec<Point>, options: &LayoutOptions, budget: &mut Budget) {
  if points.len() < 150 || !model.perspective.is_empty() {
    return;
  }
  let allowance = budget.0.saturating_sub(250_000);
  let mut work = Budget(allowance);
  let candidate = search(model, points, options, &mut work);
  budget.0 -= allowance - work.0;
  if let Some(candidate) = candidate {
    *points = candidate;
  }
  ports(model, points, options, budget);
}

/// Uses two breadth-first sweeps, a deterministic diameter approximation on rings.
fn spine(model: &Model, work: &mut Budget) -> Option<Vec<usize>> {
  fn sweep(model: &Model, start: usize, work: &mut Budget) -> Option<(usize, Vec<usize>)> {
    if !work.charge(model.ids.len() + model.edges.len() * 2) {
      return None;
    }
    let mut parents = vec![usize::MAX; model.ids.len()];
    let mut queue = VecDeque::from([start]);
    let mut last = start;
    parents[start] = start;
    while let Some(row) = queue.pop_front() {
      last = row;
      for &edge in &model.rows[row] {
        let [a, b] = model.edges[edge].ends;
        let next = if a == row { b } else { a };
        if parents[next] == usize::MAX {
          parents[next] = row;
          queue.push_back(next);
        }
      }
    }
    Some((last, parents))
  }
  let (start, _) = sweep(model, 0, work)?;
  let (mut end, parents) = sweep(model, start, work)?;
  let mut path = vec![end];
  while end != start {
    end = parents[end];
    path.push(end);
  }
  path.reverse();
  Some(path)
}

/// Extracts nested downstream sides only across topologically verified single bridges.
fn steps(model: &Model, path: &[usize], work: &mut Budget) -> Option<Vec<Step>> {
  let mut result = Vec::new();
  for index in 1..path.len() - 1 {
    let [parent, hinge] = [path[index - 1], path[index]];
    let cut = *model.rows[parent]
      .iter()
      .find(|&&e| model.edges[e].ends.contains(&hinge))?;
    if !model.edges[cut].movable {
      continue;
    }
    if !work.charge(model.ids.len() + model.edges.len() * 2) {
      return None;
    }
    let mut seen = vec![false; model.ids.len()];
    let mut side = vec![hinge];
    seen[hinge] = true;
    let mut cursor = 0;
    while cursor < side.len() {
      let row = side[cursor];
      cursor += 1;
      for &edge in &model.rows[row] {
        if edge == cut {
          continue;
        }
        let [a, b] = model.edges[edge].ends;
        let next = if a == row { b } else { a };
        if !seen[next] {
          seen[next] = true;
          side.push(next);
        }
      }
    }
    result.push(Step {
      index,
      side,
      fixed: Vec::new(),
      released: Vec::new(),
      prefix_edges: Vec::new(),
      joining_edges: Vec::new(),
    });
  }
  for i in 0..result.len() {
    if !work.charge(3 * model.ids.len() + model.edges.len()) {
      return None;
    }
    let mut current = vec![false; model.ids.len()];
    let mut suffix = vec![false; model.ids.len()];
    for &row in &result[i].side {
      current[row] = true;
    }
    if let Some(next) = result.get(i + 1) {
      for &row in &next.side {
        suffix[row] = true;
      }
    }
    result[i].fixed = (0..current.len()).filter(|&row| !current[row]).collect();
    result[i].released = (0..current.len()).filter(|&row| current[row] && !suffix[row]).collect();
    for edge in &model.edges {
      let [a, b] = edge.ends;
      if !current[a] && !current[b] {
        result[i].prefix_edges.push(edge.ends);
      } else if !suffix[a] && !suffix[b] {
        result[i].joining_edges.push(edge.ends);
      }
    }
  }
  Some(result)
}

/// Ranks complete plans; the unchanged baseline is always a valid fallback.
fn search(model: &Model, initial: &[Point], options: &LayoutOptions, work: &mut Budget) -> Option<Vec<Point>> {
  let path = spine(model, work)?;
  if path.len() < 64 {
    return None;
  }
  let steps = steps(model, &path, work)?;
  if steps.len() < 24 {
    return None;
  }
  let before = model.measure(initial, options.minimum_spacing, work)?;
  let mut best_cost = score(initial, &before)?;
  let mut best = compact(model, initial, &path, &steps, options, work);
  if let Some(points) = &best {
    let Some(q) = model.measure(points, options.minimum_spacing, work) else {
      return best;
    };
    best_cost = score(points, &q)?;
  }
  // Different row lengths handle ring-heavy and mostly acyclic spines without
  // a molecule-name or atom-number template. All guides use the same allowance.
  for width in [16., 24., 32.] {
    let Some(poses) = guided(initial, &path, &steps, width, 6., work) else {
      break;
    };
    for mut pose in poses {
      if !work.charge(model.double_stereo.len()) {
        return best;
      }
      if !model.preserves_double_stereo(&pose.points) {
        continue;
      }
      // A guide chooses the main run, not exact hydroxyl exit slots. Repair its
      // complete substituents using a bounded slice of this stage's allowance.
      let allowance = work.0.min(150_000);
      let mut repair = Budget(allowance);
      super::improve(model, &mut pose.points, options, &mut repair);
      work.0 -= allowance - repair.0;
      let Some(quality) = model.measure(&pose.points, options.minimum_spacing, work) else {
        return best;
      };
      if quality.crossings.len() > before.crossings.len() || quality.crowds.len() > before.crowds.len() {
        continue;
      }
      let cost = score(&pose.points, &quality)?;
      if cost + 1e-8 < best_cost {
        best_cost = cost;
        best = Some(pose.points);
      }
    }
  }
  best
}

/// Combines skeleton conflicts with rectangular footprint, rather than atom compression.
fn score(points: &[Point], quality: &Quality) -> Option<f64> {
  let bounds = Bounds::from_points(points.iter().copied()).ok()??;
  Some(
    10000. * quality.crossings.len() as f64
      + 1000. * quality.crowds.len() as f64
      + 10. * quality.cost
      + (bounds.width() + 2.) * (bounds.height() + 2.)
      + 0.25 * bounds.width().max(bounds.height()).powi(2),
  )
}

/// Improves complete spine-cut poses before attempting stricter row guides.
fn compact(
  model: &Model,
  initial: &[Point],
  path: &[usize],
  steps: &[Step],
  options: &LayoutOptions,
  work: &mut Budget,
) -> Option<Vec<Point>> {
  let allowance = work.0.min(800_000);
  let mut local = Budget(allowance);
  let mut points = initial.to_vec();
  let Some(mut quality) = model.measure(&points, options.minimum_spacing, &mut local) else {
    work.0 -= allowance - local.0;
    return None;
  };
  let mut cost = score(&points, &quality)?;
  let mut changed = false;
  'passes: for _ in 0..2 {
    let mut improved = false;
    for step in steps {
      if !local.charge(points.len() + step.side.len() + model.double_stereo.len()) {
        break 'passes;
      }
      let hinge = points[path[step.index]];
      let parent = points[path[step.index - 1]];
      let (sin, cos) = (2. * (hinge.y - parent.y).atan2(hinge.x - parent.x)).sin_cos();
      let mut candidate = points.clone();
      for &row in &step.side {
        if row == path[step.index] {
          continue;
        }
        let (x, y) = (points[row].x - hinge.x, points[row].y - hinge.y);
        candidate[row] = hinge.translated(cos * x + sin * y, sin * x - cos * y);
      }
      if !model.preserves_double_stereo(&candidate) {
        continue;
      }
      let Some(q) = model.measure(&candidate, options.minimum_spacing, &mut local) else {
        break 'passes;
      };
      if q.crossings.len() > quality.crossings.len() || q.crowds.len() > quality.crowds.len() {
        continue;
      }
      let Some(value) = score(&candidate, &q) else {
        continue;
      };
      if value + 1e-8 < cost {
        points = candidate;
        quality = q;
        cost = value;
        improved = true;
        changed = true;
      }
    }
    if !improved {
      break;
    }
  }
  work.0 -= allowance - local.0;
  changed.then_some(points)
}

/// Adjusts only external single-bridge ports of neutral saturated ring carbons.
///
/// Ring geometry stays rigid. Every host-angle change is bounded to thirty
/// degrees from stage entry and must preserve ray order and source alkene
/// relations; aromatic, carbonyl and heteroatom hosts never participate.
fn ports(model: &Model, points: &mut Vec<Point>, options: &LayoutOptions, work: &mut Budget) {
  let Some(mut quality) = model.measure(points, options.minimum_spacing, work) else {
    return;
  };
  let initial = points.clone();
  for edge in 0..model.edges.len() {
    if !model.edges[edge].movable {
      continue;
    }
    let Some((side, _, host)) = model.branch(edge, work) else {
      return;
    };
    if !model.ring_ports[host] {
      continue;
    }
    if !work.charge(quality.crowds.len() * side.len() * 2) {
      return;
    }
    if !quality
      .crowds
      .iter()
      .any(|&(a, b)| side.contains(&a) != side.contains(&b))
    {
      continue;
    }
    let mut best = None;
    let mut cost = quality.cost;
    for turn in [PI / 12., -PI / 12., PI / 6., -PI / 6.] {
      if !work.charge(points.len() + side.len() + model.rows[host].len().pow(2) + model.double_stereo.len()) {
        return;
      }
      let mut candidate = points.clone();
      let origin = points[host];
      let (sin, cos) = turn.sin_cos();
      for &row in &side {
        let (x, y) = (points[row].x - origin.x, points[row].y - origin.y);
        candidate[row] = origin.translated(cos * x - sin * y, sin * x + cos * y);
      }
      if !model.preserves_host_angles(host, &initial, &candidate) || !model.preserves_double_stereo(&candidate) {
        continue;
      }
      let Some(q) = model.measure(&candidate, options.minimum_spacing, work) else {
        return;
      };
      if q.crossings.len() > quality.crossings.len() || q.crowds.len() > quality.crowds.len() {
        continue;
      }
      if q.cost + 1e-8 < cost {
        cost = q.cost;
        best = Some((candidate, q));
      }
    }
    if let Some((candidate, q)) = best {
      *points = candidate;
      quality = q;
    }
  }
}

/// Keeps a small beam of full poses while choosing bridge-side reflections.
fn guided(
  initial: &[Point],
  path: &[usize],
  steps: &[Step],
  width: f64,
  gap: f64,
  work: &mut Budget,
) -> Option<Vec<Pose>> {
  if !work.charge(initial.len()) {
    return None;
  }
  let origin = initial[path[0]];
  let ray = initial[path[1]];
  let angle = -(ray.y - origin.y).atan2(ray.x - origin.x) - PI / 6.;
  let (sin, cos) = angle.sin_cos();
  let points = initial
    .iter()
    .map(|p| {
      let (x, y) = (p.x - origin.x, p.y - origin.y);
      Point::new(cos * x - sin * y, sin * x + cos * y)
    })
    .collect();
  let mut beam = vec![Pose {
    points,
    cost: 0.,
    east: true,
    descending: false,
    row_y: 0.,
  }];
  for (order, step) in steps.iter().enumerate() {
    let i = step.index;
    let look = steps.get(order + 1).map_or(path.len() - 1, |s| s.index);
    let mut next = Vec::new();
    for pose in beam {
      for reflect in [false, true] {
        if !work.charge(
          initial.len()
            + step.side.len()
            + step.released.len() * step.fixed.len()
            + step.joining_edges.len() * step.prefix_edges.len(),
        ) {
          return None;
        }
        let mut points = pose.points.clone();
        let hinge = points[path[i]];
        if reflect {
          let parent = points[path[i - 1]];
          let angle = 2. * (hinge.y - parent.y).atan2(hinge.x - parent.x);
          let (sin, cos) = angle.sin_cos();
          for &row in &step.side {
            if row == path[i] {
              continue;
            }
            let (x, y) = (points[row].x - hinge.x, points[row].y - hinge.y);
            points[row] = hinge.translated(cos * x + sin * y, sin * x - cos * y);
          }
        }
        let mut east = pose.east;
        let mut descending = pose.descending;
        let mut row_y = pose.row_y;
        if !descending && ((east && hinge.x > width - 2.) || (!east && hinge.x < 2.)) {
          descending = true;
        }
        if descending && hinge.y < row_y - gap {
          descending = false;
          east = !east;
          row_y = hinge.y;
        }
        let target = if descending {
          Point::new(0., -1.)
        } else {
          Point::new(if east { 1. } else { -1. }, 0.)
        };
        let ahead = points[path[look]];
        let length = ahead.distance(hinge).max(1e-9);
        let direction = ((ahead.x - hinge.x) * target.x + (ahead.y - hinge.y) * target.y) / length;
        let mut cost = pose.cost + (1. - direction) * (look - i) as f64;
        // Already routed spine segments are obstacles; the as-yet uncommitted
        // suffix is allowed to change in later reflections.
        for &row in &step.released {
          for &fixed in &step.fixed {
            cost += 100. * (1. - points[row].distance(points[fixed])).max(0.).powi(2);
          }
        }
        for &[a, b] in &step.joining_edges {
          for &[c, d] in &step.prefix_edges {
            if [a, b].contains(&c) || [a, b].contains(&d) {
              continue;
            }
            if super::super::quality::crosses(points[a], points[b], points[c], points[d]) {
              cost += 500.;
            }
          }
        }
        next.push(Pose {
          points,
          cost,
          east,
          descending,
          row_y,
        });
      }
    }
    next.sort_by(|a, b| a.cost.total_cmp(&b.cost));
    let mut unique = BTreeSet::new();
    next.retain(|pose| {
      let a = pose.points[path[i]];
      let b = pose.points[path[look]];
      unique.insert((
        (a.x * 1e6).round() as i64,
        (a.y * 1e6).round() as i64,
        (b.x * 1e6).round() as i64,
        (b.y * 1e6).round() as i64,
        pose.east,
        pose.descending,
      ))
    });
    next.truncate(3);
    beam = next;
  }
  Some(beam)
}
