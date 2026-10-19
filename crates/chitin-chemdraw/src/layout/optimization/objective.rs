//! Drawing penalties and analytical search directions, separate from constraints.

use super::constraints::Model;
use crate::{
  geometry::Point,
  layout::quality::{self, Budget, Quality, segment_distance},
};
use std::f64::consts::TAU;

/// One completely measured private search state.
pub(super) struct Evaluation {
  /// Shared geometry penalties and diagnostic contacts.
  pub(super) quality: Quality,
  /// Geometry, soft-angle and crossing-depth penalties combined.
  pub(super) total: f64,
  /// Objective derivative for every dense vertex; pinned rows stay zero.
  pub(super) gradient: Vec<Point>,
}

/// Computes deterministic objective values and directions within the work budget.
///
/// Pair scans are quadratic in this initial implementation, not described as a
/// force-field calculation or globally optimal solver. Every scan is charged;
/// no partially evaluated result can pass the acceptance boundary.
pub(super) fn evaluate(model: &Model, points: &[Point], budget: &mut Budget) -> Option<Evaluation> {
  let quality = quality::measure(points, &model.edges, 0.55, budget)?;
  if !budget.charge(points.len()) {
    return None;
  }
  let mut result = Evaluation {
    total: quality.cost,
    quality,
    gradient: vec![Point::new(0., 0.); points.len()],
  };
  for (i, &p) in points.iter().enumerate() {
    for (j, &q) in points.iter().enumerate().skip(i + 1) {
      if !budget.charge(1) {
        return None;
      }
      let distance = p.distance(q);
      if distance >= 0.95 {
        continue;
      }
      if distance <= 1e-12 && !budget.charge(model.rows[i].len() + model.rows[j].len()) {
        return None;
      }
      let (ux, uy) = if distance > 1e-12 {
        ((p.x - q.x) / distance, (p.y - q.y) / distance)
      } else {
        tie_direction(model, points, i, j)
      };
      let force = 2. * (distance - 0.95);
      add(&mut result.gradient[i], force * ux, force * uy);
      add(&mut result.gradient[j], -force * ux, -force * uy);
    }
    for &[a, b] in &model.edges {
      if !budget.charge(1) {
        return None;
      }
      if i == a || i == b {
        continue;
      }
      let distance = segment_distance(p, points[a], points[b]);
      if distance < 0.25 {
        segment_force(points, &mut result.gradient, i, a, b, 8. * (distance - 0.25));
      }
    }
  }
  // A constant crossing count has no gradient. Pull its nearest endpoint
  // through the opposite edge with a depth term; all source constraints and
  // no-new-crossing checks still apply after projection. Intermediate states
  // may be private: only a nonworse geometry score can replace the input.
  for &(i, j) in &result.quality.crossings {
    if !budget.charge(4) {
      return None;
    }
    let [a, b] = model.edges[i];
    let [c, d] = model.edges[j];
    let mut best = (f64::INFINITY, [a, c, d]);
    for rows in [[a, c, d], [b, c, d], [c, a, b], [d, a, b]] {
      let [p, x, y] = rows;
      let distance = segment_distance(points[p], points[x], points[y]);
      if distance < best.0 {
        best = (distance, rows);
      }
    }
    result.total += 8. * (best.0 + 0.25).powi(2);
    let [p, a, b] = best.1;
    segment_force(points, &mut result.gradient, p, a, b, 16. * (best.0 + 0.25));
  }
  for angle in &model.angles {
    if !budget.charge(1) {
      return None;
    }
    if angle.hard {
      continue;
    }
    let [a, c, b] = angle.rows;
    let (u, v) = (
      Point::new(points[a].x - points[c].x, points[a].y - points[c].y),
      Point::new(points[b].x - points[c].x, points[b].y - points[c].y),
    );
    let (lu, lv) = (u.x.hypot(u.y), v.x.hypot(v.y));
    let cosine = ((u.x * v.x + u.y * v.y) / (lu * lv)).clamp(-1., 1.);
    let delta = cosine - angle.reference.cos();
    let narrow = (cosine - 0.5).max(0.);
    let straight = (-3_f64.sqrt() / 2. - cosine).max(0.);
    result.total += 0.02 * delta * delta + 0.2 * (narrow * narrow + straight * straight);
    let derivative = 0.04 * delta + 0.4 * narrow - 0.4 * straight;
    let ga = Point::new(
      derivative * (v.x / (lu * lv) - cosine * u.x / (lu * lu)),
      derivative * (v.y / (lu * lv) - cosine * u.y / (lu * lu)),
    );
    let gb = Point::new(
      derivative * (u.x / (lu * lv) - cosine * v.x / (lv * lv)),
      derivative * (u.y / (lu * lv) - cosine * v.y / (lv * lv)),
    );
    add(&mut result.gradient[a], ga.x, ga.y);
    add(&mut result.gradient[b], gb.x, gb.y);
    add(&mut result.gradient[c], -ga.x - gb.x, -ga.y - gb.y);
  }
  for (i, g) in result.gradient.iter_mut().enumerate() {
    if model.pinned[i] {
      *g = Point::new(0., 0.);
    }
  }
  if !result.total.is_finite() || result.gradient.iter().any(|g| !g.is_finite()) {
    return None;
  }
  Some(result)
}

/// Adds one derivative contribution without overloading the public point type.
fn add(point: &mut Point, x: f64, y: f64) {
  point.x += x;
  point.y += y;
}

/// Differentiates point-to-segment distance, including endpoint barycentric terms.
fn segment_force(points: &[Point], gradient: &mut [Point], p: usize, a: usize, b: usize, derivative: f64) {
  let (dx, dy) = (points[b].x - points[a].x, points[b].y - points[a].y);
  let t = (((points[p].x - points[a].x) * dx + (points[p].y - points[a].y) * dy) / (dx * dx + dy * dy)).clamp(0., 1.);
  let closest = points[a].translated(t * dx, t * dy);
  let distance = points[p].distance(closest);
  let (ux, uy) = if distance > 1e-12 {
    (
      (points[p].x - closest.x) / distance,
      (points[p].y - closest.y) / distance,
    )
  } else {
    let length = dx.hypot(dy);
    (-dy / length, dx / length)
  };
  let (gx, gy) = (derivative * ux, derivative * uy);
  add(&mut gradient[p], gx, gy);
  add(&mut gradient[a], -(1. - t) * gx, -(1. - t) * gy);
  add(&mut gradient[b], -t * gx, -t * gy);
}

/// Breaks coincident-center ties using host positions first, then a stable index ray.
fn tie_direction(model: &Model, points: &[Point], i: usize, j: usize) -> (f64, f64) {
  let mean = |row: usize| {
    let mut center = Point::new(0., 0.);
    for &other in &model.rows[row] {
      center.x += points[other].x;
      center.y += points[other].y;
    }
    if !model.rows[row].is_empty() {
      center.x /= model.rows[row].len() as f64;
      center.y /= model.rows[row].len() as f64;
    }
    center
  };
  let (a, b) = (mean(i), mean(j));
  let length = a.distance(b);
  if length > 1e-12 {
    ((a.x - b.x) / length, (a.y - b.y) / length)
  } else {
    let angle = ((i % 17) * 7 + (j % 17) * 11) as f64 * TAU / 17.;
    (angle.cos(), angle.sin())
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use chitin_chem::{io::smiles::read_smiles, perception::analyze_rings};
  use std::collections::BTreeSet;
  type TestResult = Result<(), Box<dyn std::error::Error>>;

  #[test]
  fn analytical_directions_should_match_finite_differences_away_from_contacts() -> TestResult {
    let graph = read_smiles("OCCO")?.into_graph();
    let ids: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
    let y = (1. - 0.45_f64.powi(2)).sqrt();
    let points = vec![
      Point::new(-0.05, y),
      Point::new(-0.5, 0.),
      Point::new(0.5, 0.),
      Point::new(0.05, y),
    ];
    let mut model = Model::topology(&graph, &ids, &BTreeSet::new())?;
    assert!(model.prepare(
      &graph,
      &points,
      analyze_rings(&graph)?.bridges(),
      &mut Budget(1_000_000)
    ));
    let evaluation = evaluate(&model, &points, &mut Budget(1_000_000)).ok_or("no objective")?;
    for row in 0..points.len() {
      for axis in 0..2 {
        let mut low = points.clone();
        let mut high = points.clone();
        if axis == 0 {
          low[row].x -= 1e-6;
          high[row].x += 1e-6;
        } else {
          low[row].y -= 1e-6;
          high[row].y += 1e-6;
        }
        let lo = evaluate(&model, &low, &mut Budget(1_000_000)).ok_or("no low")?.total;
        let hi = evaluate(&model, &high, &mut Budget(1_000_000)).ok_or("no high")?.total;
        let actual = if axis == 0 {
          evaluation.gradient[row].x
        } else {
          evaluation.gradient[row].y
        };
        assert!(
          ((hi - lo) / 2e-6 - actual).abs() < 1e-5,
          "row {row}, axis {axis}: numeric {}, analytical {actual}",
          (hi - lo) / 2e-6
        );
      }
    }
    Ok(())
  }
}
