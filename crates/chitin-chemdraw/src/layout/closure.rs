//! Bounded whole-graph distance closure with mandatory alkene-side constraints.
//!
//! This is a coverage fallback, not a preferred ring template or a perspective
//! assignment. All edges remain present; unresolved visual contacts are reported.

use super::{Layout2d, LayoutDiagnostic, LayoutError, LayoutOptions};
use crate::{geometry::Point, stereo_geometry::relation};
use chitin_chem::{AtomId, DoubleBondRelation, MolecularGraph};
use std::collections::BTreeMap;

/// A directed half-plane constraint; the fourth reference is never discarded.
struct Side {
  /// Axis endpoints and one adjacent reference in dense source order.
  rows: [usize; 3],
  /// Required sign of the oriented area.
  sign: f64,
}

/// Projects a signed triangle area away from the collinear boundary.
fn side(points: &mut [Point], constraint: &Side) {
  let [a, b, r] = constraint.rows;
  let [pa, pb, pr] = [a, b, r].map(|i| points[i]);
  let area = (pb.x - pa.x) * (pr.y - pa.y) - (pb.y - pa.y) * (pr.x - pa.x);
  let missing = 0.3 - constraint.sign * area;
  if missing <= 0. {
    return;
  }
  let gradients = [
    Point::new(pb.y - pr.y, pr.x - pb.x),
    Point::new(pr.y - pa.y, pa.x - pr.x),
    Point::new(pa.y - pb.y, pb.x - pa.x),
  ];
  let norm: f64 = gradients.iter().map(|p| p.x * p.x + p.y * p.y).sum();
  if norm < 1e-12 {
    return;
  }
  let step = (missing / norm).min(0.5) * constraint.sign;
  for (row, gradient) in constraint.rows.into_iter().zip(gradients) {
    points[row] = points[row].translated(step * gradient.x, step * gradient.y);
  }
}

/// Restores all source edge distances while satisfying every supplied E/Z fact.
///
/// # Parameters
/// - `graph`, `layout`: complete immutable chemistry and detached geometry.
/// - `budget`: shared charged projection allowance; no unbounded retry loop.
///
/// # Returns
/// Whether a finite candidate satisfies all alkene relations and restores each
/// entry edge length to relative tolerance 1e-8. Failure leaves layout untouched.
///
/// # Errors
/// Invalid graph references propagate; exhaustion returns `false` without
/// publishing a partially closed ring or dropping an alkene annotation.
pub(super) fn repair(graph: &MolecularGraph, layout: &mut Layout2d, budget: &mut usize) -> Result<bool, LayoutError> {
  solve(graph, layout, budget, false)
}

/// Shares the constrained solver between metric-preserving repair and unit seeds.
fn solve(graph: &MolecularGraph, layout: &mut Layout2d, budget: &mut usize, unit: bool) -> Result<bool, LayoutError> {
  let setup = graph.atom_count().saturating_mul(2) + graph.bond_count() + graph.double_bond_stereos().count() * 16;
  let Some(remaining) = budget.checked_sub(setup) else {
    return Ok(false);
  };
  *budget = remaining;
  let ids: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
  let rows: BTreeMap<_, _> = ids.iter().enumerate().map(|(i, &id)| (id, i)).collect();
  let mut points: Vec<_> = ids.iter().map(|id| layout.positions[id]).collect();
  let edges: Vec<_> = graph
    .bonds()
    .map(|(_, bond)| {
      let [a, b] = bond.atoms().map(|id| rows[&id]);
      ([a, b], if unit { 1. } else { points[a].distance(points[b]) })
    })
    .collect();
  let mut sides = Vec::new();
  for (_, stereo) in graph.double_bond_stereos() {
    let [a, b] = stereo.ends().map(|id| rows[&id]);
    let [ra, rb] = stereo.references().map(|id| rows[&id]);
    let area = (points[b].x - points[a].x) * (points[ra].y - points[a].y)
      - (points[b].y - points[a].y) * (points[ra].x - points[a].x);
    let sign = if area < 0. { -1. } else { 1. };
    sides.push(Side { rows: [a, b, ra], sign });
    sides.push(Side {
      rows: [a, b, rb],
      sign: sign
        * if stereo.relation() == DoubleBondRelation::SameSide {
          1.
        } else {
          -1.
        },
    });
  }
  let cost = edges.len().saturating_mul(2) + sides.len().saturating_mul(8) + ids.len();
  for pass in 0..4000 {
    let Some(remaining) = budget.checked_sub(cost) else {
      return Ok(false);
    };
    *budget = remaining;
    for constraint in &sides {
      side(&mut points, constraint);
    }
    for &([a, b], target) in &edges {
      let length = points[a].distance(points[b]);
      if length < 1e-12 {
        continue;
      }
      let factor = (length - target) / length * 0.5;
      let dx = (points[b].x - points[a].x) * factor;
      let dy = (points[b].y - points[a].y) * factor;
      points[a] = points[a].translated(dx, dy);
      points[b] = points[b].translated(-dx, -dy);
    }
    if pass % 8 == 0
      && edges
        .iter()
        .all(|&([a, b], target)| (points[a].distance(points[b]) - target).abs() < 1e-8 * target.max(1.))
      && graph.double_bond_stereos().all(|(_, stereo)| {
        let [a, b] = stereo.ends();
        let [ra, rb] = stereo.references();
        relation([a, b, ra, rb].map(|id| points[rows[&id]])) == Some(stereo.relation())
      })
    {
      let positions = ids.iter().copied().zip(points).collect();
      let mut candidate = Layout2d::from_positions(positions)?;
      candidate.diagnostics = layout.diagnostics.clone();
      // Moving a whole ring invalidates any previous camera-depth explanation.
      candidate.diagnostics.push(LayoutDiagnostic::ConstraintClosure);
      *layout = candidate;
      return Ok(true);
    }
  }
  Ok(false)
}

/// Produces bounded deterministic global seeds only after preferred embedding fails.
///
/// # Errors
/// Preserves graph, atom and work limits. No successful result can contain a
/// wrong/collinear supplied alkene relation or a missing source position.
pub(super) fn generate(graph: &MolecularGraph, options: &LayoutOptions) -> Result<Layout2d, LayoutError> {
  let mut budget = options.fallback_budget;
  budget = budget
    .checked_sub(graph.atom_count().saturating_mul(2) + graph.bond_count())
    .ok_or(LayoutError::FallbackBudgetExceeded)?;
  let ids: Vec<AtomId> = graph.atoms().map(|(id, _)| id).collect();
  let rows: BTreeMap<_, _> = ids.iter().enumerate().map(|(i, &id)| (id, i)).collect();
  let edges: Vec<_> = graph
    .bonds()
    .map(|(_, bond)| bond.atoms().map(|id| rows[&id]))
    .collect();
  for seed in 0..8 {
    let radius = (ids.len() as f64).sqrt().max(1.);
    let mut points: Vec<_> = (0..ids.len())
      .map(|i| {
        let angle = (i * (104729 + seed * 7919) + seed * 13007) as f64 * 2.399963229728653;
        let r = radius * ((i + 1) as f64 / ids.len().max(1) as f64).sqrt();
        Point::new(r * angle.cos(), r * angle.sin())
      })
      .collect();
    // Soft repulsion separates nonbonded atoms before exact source-edge closure.
    for _ in 0..120 {
      let cost = ids.len().saturating_mul(ids.len()) / 2 + edges.len();
      budget = budget.checked_sub(cost).ok_or(LayoutError::FallbackBudgetExceeded)?;
      for a in 0..points.len() {
        for b in a + 1..points.len() {
          let distance = points[a].distance(points[b]);
          if distance > 1e-8 && distance < 0.8 {
            let factor = (0.8 - distance) / distance * 0.1;
            let dx = (points[b].x - points[a].x) * factor;
            let dy = (points[b].y - points[a].y) * factor;
            points[a] = points[a].translated(-dx, -dy);
            points[b] = points[b].translated(dx, dy);
          }
        }
      }
      for &[a, b] in &edges {
        let distance = points[a].distance(points[b]).max(1e-8);
        let factor = (distance - 1.) / distance * 0.25;
        let dx = (points[b].x - points[a].x) * factor;
        let dy = (points[b].y - points[a].y) * factor;
        points[a] = points[a].translated(dx, dy);
        points[b] = points[b].translated(-dx, -dy);
      }
    }
    // Install exact unit targets before the coupled closure stage.
    let mut candidate = Layout2d::from_positions(ids.iter().copied().zip(points).collect())?;
    if close_unit(graph, &mut candidate, &mut budget)? {
      super::refinement::run(
        graph,
        &mut candidate,
        options,
        &mut super::quality::Budget(options.refinement_budget),
        &mut super::quality::Budget(options.optimization.work_budget),
        &mut super::quality::Budget(options.backbone_budget),
        super::refinement::AttachmentPolicy::Rigid,
      )?;
      return Ok(candidate);
    }
  }
  Err(LayoutError::FallbackBudgetExceeded)
}

/// Uses an auxiliary target snapshot without changing any source bond order.
fn close_unit(graph: &MolecularGraph, layout: &mut Layout2d, budget: &mut usize) -> Result<bool, LayoutError> {
  // Exact closure uses the same projector; unit targets are supplied separately
  // by normalizing all edges in its metric table, not by altering the graph.
  solve(graph, layout, budget, true)
}
