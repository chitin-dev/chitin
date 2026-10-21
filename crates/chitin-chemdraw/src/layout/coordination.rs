//! Explicit coordination drawing over a detached covalent layout topology.
//!
//! Chelate fragments supply rigid candidates before flexible closure repair.
//! Their planar ordering is not an octahedral/square-planar or stereochemical
//! assignment. Only declared attachments differ from covalent unit geometry.

use super::{
  Layout2d, LayoutDiagnostic, LayoutError, LayoutOptions,
  quality::{self, Budget},
};
use crate::geometry::{Bounds, Point};
use chitin_chem::{
  BondOrder, MolecularGraph,
  coordination::{CoordinationError, CoordinationSite},
};
use std::collections::BTreeMap;

mod fragments;
mod poses;
mod routing;

/// Relative cost of covalent interior-angle errors in normalized drawing space.
const ANGLE_WEIGHT: f64 = 0.2;
/// Cost of mean squared extent, normalized so larger inputs do not dominate by size.
const EXTENT_WEIGHT: f64 = 0.4;

/// Independent search and length policy for an explicitly declared site.
#[derive(Debug, Clone, PartialEq)]
pub struct CoordinationOptions {
  /// Drawing-only attachment length in nominal covalent-bond units.
  pub attachment_length: f64,
  /// Relaxation steps per complete deterministic seed.
  pub iterations: usize,
  /// Shared work allowance for proposals, projections and complete validation.
  pub work_budget: usize,
  /// Separate optional work for anchored covalent-backbone routing; zero disables it.
  pub routing_budget: usize,
}

impl Default for CoordinationOptions {
  fn default() -> Self {
    Self {
      attachment_length: 1.4,
      iterations: 160,
      work_budget: 12_000_000,
      routing_budget: 4_000_000,
    }
  }
}

/// A rejected site or incomplete constrained layout, never a truncated drawing.
#[derive(Debug, thiserror::Error)]
pub enum CoordinationLayoutError {
  /// Source declaration no longer matches its graph.
  #[error(transparent)]
  Site(#[from] CoordinationError),
  /// Ordinary ligand layout failed.
  #[error(transparent)]
  Layout(#[from] LayoutError),
  /// Search dimensions, lengths or work limits are invalid.
  #[error("invalid coordination drawing options")]
  Options,
  /// Complete single sites require planar unit-length ligands without supplied stereo.
  #[error(
    "coordination layout requires a complete 2-to-8 attachment site with planar unit-length ligands and no supplied stereo"
  )]
  Unsupported,
  /// No complete finite seed satisfied all source bond-length constraints.
  #[error("coordination drawing could not close all source bonds within its work allowance")]
  Closure,
}

/// Generates a complete ligand-first drawing for one explicitly declared site.
///
/// # Parameters
/// - `graph`, `site`: original chemistry and an explicit association of source edges.
/// - `layout_options`: unchanged covalent-ligand policy and required atom limit.
/// - `options`: independent attachment distance and bounded joint repair allowance.
///
/// # Returns
/// Coordinates covering every input handle. Covalent edges retain unit lengths;
/// declared attachments use their separate target. Remaining contacts are still
/// reported, never converted to invented perspective gaps or stereo descriptors.
///
/// # Errors
/// Rejects foreign/stale declarations, partially declared centers, supplied stereo,
/// unsupported ligand topology and failed finite closure. No source edge is edited.
pub fn generate(
  graph: &MolecularGraph,
  site: &CoordinationSite,
  layout_options: &LayoutOptions,
  options: &CoordinationOptions,
) -> Result<Layout2d, CoordinationLayoutError> {
  site.validate(graph)?;
  if !options.attachment_length.is_finite()
    || !(1. ..=8.).contains(&options.attachment_length)
    || options.iterations == 0
    || options.work_budget == 0
  {
    return Err(CoordinationLayoutError::Options);
  }
  if !(2..=8).contains(&site.attachments().len())
    || graph.neighbors(site.center()).map_err(CoordinationError::from)?.len() != site.attachments().len()
    || graph.tetrahedral_centers().len() != 0
    || graph.double_bond_stereos().len() != 0
  {
    return Err(CoordinationLayoutError::Unsupported);
  }
  if graph.atom_count() > layout_options.max_atoms {
    return Err(
      LayoutError::AtomBudget {
        count: graph.atom_count(),
        limit: layout_options.max_atoms,
      }
      .into(),
    );
  }
  let mut ligand = graph.clone();
  ligand.remove_atom(site.center()).map_err(CoordinationError::from)?;
  let initial = super::generate(&ligand, layout_options)?;
  if !initial.bond_crossings().is_empty()
    || !initial.drawing_depths().is_empty()
    || ligand.bonds().any(|(_, bond)| {
      let [a, b] = bond.atoms();
      (initial.positions()[&a].distance(initial.positions()[&b]) - 1.).abs() > 1e-8
    })
  {
    return Err(CoordinationLayoutError::Unsupported);
  }
  let ids: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
  let rows: BTreeMap<_, _> = ids.iter().enumerate().map(|(i, &id)| (id, i)).collect();
  let center = rows[&site.center()];
  let edges: Vec<_> = graph
    .bonds()
    .map(|(_, bond)| bond.atoms().map(|id| rows[&id]))
    .collect();
  let lengths: Vec<_> = graph
    .bonds()
    .map(|(id, _)| {
      if site.attachments().iter().any(|&(edge, _)| edge == id) {
        options.attachment_length
      } else {
        1.
      }
    })
    .collect();
  let donors: Vec<_> = site.attachments().iter().map(|&(_, id)| rows[&id]).collect();
  let mean = Point::new(
    site
      .attachments()
      .iter()
      .map(|&(_, id)| initial.positions()[&id].x)
      .sum::<f64>()
      / donors.len() as f64,
    site
      .attachments()
      .iter()
      .map(|&(_, id)| initial.positions()[&id].y)
      .sum::<f64>()
      / donors.len() as f64,
  );
  let base: Vec<_> = ids.iter().map(|id| initial.position(*id).unwrap_or(mean)).collect();
  // A unit-length whole-complex embedding is optional: its failure must not
  // prevent ligand-first layout of the very coordination closures it cannot own.
  let connected_seed: Vec<_> = match super::generate(graph, layout_options) {
    Ok(connected) => ids.iter().map(|id| connected.positions()[id]).collect(),
    Err(_) => base.clone(),
  };
  // Multiple-bond centers keep their initial ligand-only angular geometry.
  // Coordination edges never enter this covalent local-geometry snapshot.
  let mut constraints: Vec<_> = edges.iter().copied().zip(lengths.iter().copied()).collect();
  for &id in &ids {
    if id == site.center() {
      continue;
    }
    let neighbors: Vec<_> = ligand.neighbors(id).map_err(CoordinationError::from)?.collect();
    if !neighbors
      .iter()
      .any(|&(_, edge)| ligand.bond(edge).is_some_and(|b| b.order() != BondOrder::Single))
    {
      continue;
    }
    for (i, &(a, _)) in neighbors.iter().enumerate() {
      for &(b, _) in &neighbors[i + 1..] {
        constraints.push(([rows[&a], rows[&b]], base[rows[&a]].distance(base[rows[&b]])));
      }
    }
  }
  let repair_allowance = options.work_budget / 4;
  let mut budget = Budget(options.work_budget - repair_allowance);
  let mut seeds = fragments::seeds(&ligand, site, &ids, &base, options.attachment_length, &mut budget)?;
  for attempt in 0..4 {
    let mut points = if attempt <= 1 {
      connected_seed.clone()
    } else {
      base.clone()
    };
    for (i, point) in points.iter_mut().enumerate() {
      if i != center && attempt > 1 {
        let theta = i as f64 * 2.399963229728653 + attempt as f64;
        *point = point.translated(0.6 * theta.cos(), 0.6 * theta.sin());
      }
    }
    let mut fixed = vec![false; ids.len()];
    fixed[center] = true;
    seeds.push(fragments::Seed {
      points,
      fixed,
      contract: attempt != 0,
    });
  }
  let per_seed = budget.0 / seeds.len();
  let per_repair = repair_allowance / 2 / seeds.len();
  let mut best = None;
  let mut structured = None;
  let bends = bend_constraints(&ligand, &rows, &base)?;
  for fragments::Seed {
    mut points,
    fixed,
    contract,
  } in seeds
  {
    let mut budget = Budget(per_seed);
    let closure_allowance = per_seed / 3;
    budget.0 -= closure_allowance;
    for _ in 0..options.iterations {
      if !budget.charge(points.len() * points.len() + points.len() * edges.len()) {
        break;
      }
      repel(&mut points, &edges, &fixed);
      bend(&mut points, &bends, &fixed);
      if contract {
        compact(&mut points, &fixed);
      }
      if !project(&mut points, &constraints, &fixed, 24, &mut budget) {
        break;
      }
    }
    budget.0 += closure_allowance;
    // Reserve an exact closure boundary: relaxed spring geometry is not output.
    if !project(&mut points, &constraints, &fixed, 2048, &mut budget)
      || constraints
        .iter()
        .any(|&(ends, target)| (points[ends[0]].distance(points[ends[1]]) - target).abs() > 1e-7)
    {
      continue;
    }
    let mut candidate =
      Layout2d::from_positions(ids.iter().copied().zip(points).collect()).map_err(LayoutError::from)?;
    super::refinement::run(
      graph,
      &mut candidate,
      layout_options,
      &mut Budget(per_repair),
      &mut Budget(0),
      &mut Budget(0),
      super::refinement::AttachmentPolicy::Rigid,
    )?;
    let points: Vec<_> = ids.iter().map(|id| candidate.positions()[id]).collect();
    if !closed(&points, &constraints) {
      continue;
    }
    let Some(inspection) = quality::measure(&points, &edges, layout_options.minimum_spacing, &mut budget) else {
      continue;
    };
    let cost = inspection.cost
      + 100. * inspection.crossings.len() as f64
      + 10. * inspection.crowds.len() as f64
      + composition(&points, &edges, center);
    if fixed.iter().filter(|&&fixed| fixed).count() > 1 && structured.as_ref().is_none_or(|(old, _)| cost < *old) {
      structured = Some((cost, points.clone()));
    }
    if best.as_ref().is_none_or(|(old, _)| cost < *old) {
      best = Some((cost, points));
    }
  }
  let Some((_, mut points)) = best else {
    return Err(CoordinationLayoutError::Closure);
  };
  poses::improve(
    &mut points,
    &constraints,
    &edges,
    center,
    layout_options.minimum_spacing,
    &mut Budget(repair_allowance / 2),
  );
  let mut route_budget = Budget(options.routing_budget);
  let router = routing::Model::new(graph, &ligand, &ids, center, &mut route_budget)?;
  let per_route = route_budget.0 / if structured.is_some() { 2 } else { 1 };
  router.improve(
    &mut points,
    &constraints,
    layout_options.minimum_spacing,
    &mut Budget(per_route),
  )?;
  if let Some((_, mut alternative)) = structured {
    router.improve(
      &mut alternative,
      &constraints,
      layout_options.minimum_spacing,
      &mut Budget(per_route),
    )?;
    let mut inspection = Budget(usize::MAX);
    let original = quality::measure(&points, &edges, layout_options.minimum_spacing, &mut inspection)
      .ok_or(CoordinationLayoutError::Closure)?;
    let other = quality::measure(&alternative, &edges, layout_options.minimum_spacing, &mut inspection)
      .ok_or(CoordinationLayoutError::Closure)?;
    if closed(&alternative, &constraints)
      && other.crossings.len() <= original.crossings.len()
      && other.crowds.len() <= original.crowds.len()
      && other.cost + composition(&alternative, &edges, center) < original.cost + composition(&points, &edges, center)
    {
      points = alternative;
    }
  }
  if !closed(&points, &constraints) {
    return Err(CoordinationLayoutError::Closure);
  }
  let inspection = quality::measure(&points, &edges, layout_options.minimum_spacing, &mut Budget(usize::MAX))
    .ok_or(CoordinationLayoutError::Closure)?;
  let mut result = Layout2d::from_positions(ids.iter().copied().zip(points).collect()).map_err(LayoutError::from)?;
  result.diagnostics = inspection
    .crowds
    .into_iter()
    .map(|(a, b)| LayoutDiagnostic::CrowdedAtoms { a: ids[a], b: ids[b] })
    .collect();
  let bonds: Vec<_> = graph.bonds().map(|(id, _)| id).collect();
  result.diagnostics.extend(
    inspection
      .crossings
      .into_iter()
      .map(|(a, b)| LayoutDiagnostic::CrossingBonds {
        a: bonds[a],
        b: bonds[b],
      }),
  );
  result.bounds = Bounds::from_points(result.positions.values().copied()).map_err(LayoutError::from)?;
  Ok(result)
}

/// Requires a complete finite closure after every optional graph-preserving stage.
fn closed(points: &[Point], constraints: &[([usize; 2], f64)]) -> bool {
  points.iter().all(|point| point.is_finite())
    && constraints
      .iter()
      .all(|&([a, b], target)| (points[a].distance(points[b]) - target).abs() <= 1e-7)
}

/// Captures covalent angle preferences without imposing ideals on metal rays.
fn bend_constraints(
  ligand: &MolecularGraph,
  rows: &BTreeMap<chitin_chem::AtomId, usize>,
  base: &[Point],
) -> Result<Vec<([usize; 2], f64)>, CoordinationLayoutError> {
  let mut result = Vec::new();
  for (id, _) in ligand.atoms() {
    let neighbors: Vec<_> = ligand.neighbors(id).map_err(CoordinationError::from)?.collect();
    if !(2..=3).contains(&neighbors.len()) {
      continue;
    }
    let linear = neighbors
      .iter()
      .any(|&(_, edge)| ligand.bond(edge).is_some_and(|b| b.order() == BondOrder::Triple));
    for (i, &(a, _)) in neighbors.iter().enumerate() {
      for &(b, _) in &neighbors[i + 1..] {
        let initial = base[rows[&a]].distance(base[rows[&b]]);
        let target = if linear || initial < 1.4 { initial } else { 3_f64.sqrt() };
        result.push(([rows[&a], rows[&b]], target));
      }
    }
  }
  Ok(result)
}

/// Restores familiar covalent bends before the mandatory source-edge projection.
fn bend(points: &mut [Point], constraints: &[([usize; 2], f64)], fixed: &[bool]) {
  for &([a, b], target) in constraints {
    let distance = points[a].distance(points[b]).max(1e-8);
    let scale = 0.12 * (distance - target) / distance;
    let (dx, dy) = ((points[b].x - points[a].x) * scale, (points[b].y - points[a].y) * scale);
    if !fixed[a] {
      points[a] = points[a].translated(dx, dy);
    }
    if !fixed[b] {
      points[b] = points[b].translated(-dx, -dy);
    }
  }
}

/// Proposes a small collective contraction; hard closure and contacts decide acceptance.
fn compact(points: &mut [Point], fixed: &[bool]) {
  let n = points.len() as f64;
  let mean = Point::new(
    points.iter().map(|p| p.x).sum::<f64>() / n,
    points.iter().map(|p| p.y).sum::<f64>() / n,
  );
  for (point, &fixed) in points.iter_mut().zip(fixed) {
    if !fixed {
      *point = point.translated(0.008 * (mean.x - point.x), 0.008 * (mean.y - point.y));
    }
  }
}

/// Separates centers and nonincident edges before exact distance projection.
fn repel(points: &mut [Point], edges: &[[usize; 2]], fixed: &[bool]) {
  let mut shifts = vec![Point::new(0., 0.); points.len()];
  for (i, &p) in points.iter().enumerate() {
    for (j, &q) in points.iter().enumerate().skip(i + 1) {
      let distance = p.distance(q).max(1e-8);
      if distance < 1.2 {
        let strength = 0.12 * (1.2 - distance) / distance;
        let (dx, dy) = ((p.x - q.x) * strength, (p.y - q.y) * strength);
        shifts[i] = shifts[i].translated(dx, dy);
        shifts[j] = shifts[j].translated(-dx, -dy);
      }
    }
    for &[a, b] in edges {
      if i == a || i == b {
        continue;
      }
      let u = Point::new(points[b].x - points[a].x, points[b].y - points[a].y);
      let norm = u.x * u.x + u.y * u.y;
      if norm < 1e-12 {
        continue;
      }
      let t = (((p.x - points[a].x) * u.x + (p.y - points[a].y) * u.y) / norm).clamp(0., 1.);
      let q = points[a].translated(t * u.x, t * u.y);
      let distance = p.distance(q).max(1e-8);
      if distance < 0.45 {
        let strength = 0.15 * (0.45 - distance) / distance;
        let (dx, dy) = ((p.x - q.x) * strength, (p.y - q.y) * strength);
        shifts[i] = shifts[i].translated(dx, dy);
        shifts[a] = shifts[a].translated(-(1. - t) * dx, -(1. - t) * dy);
        shifts[b] = shifts[b].translated(-t * dx, -t * dy);
      }
    }
  }
  for (i, (point, shift)) in points.iter_mut().zip(shifts).enumerate() {
    if !fixed[i] {
      *point = point.translated(shift.x, shift.y);
    }
  }
}

/// Applies bounded distance sweeps while retaining complete rigid fragments.
///
/// False rejects exhausted or numerically invalid work. True means sweeps
/// completed, not necessarily convergence; publication requires a residual scan.
fn project(
  points: &mut [Point],
  constraints: &[([usize; 2], f64)],
  fixed: &[bool],
  passes: usize,
  budget: &mut Budget,
) -> bool {
  for _ in 0..passes {
    if !budget.charge(constraints.len() * 2) {
      return false;
    }
    for &([a, b], target) in constraints {
      if fixed[a] && fixed[b] {
        continue;
      }
      let distance = points[a].distance(points[b]);
      if !distance.is_finite() || distance < 1e-12 {
        return false;
      }
      let share = if fixed[a] || fixed[b] { 1. } else { 0.5 };
      let scale = share * (distance - target) / distance;
      let (dx, dy) = ((points[b].x - points[a].x) * scale, (points[b].y - points[a].y) * scale);
      if !fixed[a] {
        points[a] = points[a].translated(dx, dy);
      }
      if !fixed[b] {
        points[b] = points[b].translated(-dx, -dy);
      }
    }
    if constraints
      .iter()
      .all(|&([a, b], target)| (points[a].distance(points[b]) - target).abs() < 1e-9)
    {
      return true;
    }
  }
  true
}

/// Ranks readable composition only after skeleton contacts have been measured.
///
/// Unit covalent rays prefer familiar bends. Declared attachments are excluded
/// from angle ideals: the drawing must not invent coordination geometry. A
/// normalized second-moment term distinguishes equally clean but sprawling poses.
fn composition(points: &[Point], edges: &[[usize; 2]], center: usize) -> f64 {
  let mut neighbors = vec![Vec::new(); points.len()];
  for &[a, b] in edges {
    if a != center && b != center {
      neighbors[a].push(b);
      neighbors[b].push(a);
    }
  }
  let mut cost = 0.;
  for (i, row) in neighbors.iter().enumerate() {
    if !(2..=3).contains(&row.len()) {
      continue;
    }
    for (j, &a) in row.iter().enumerate() {
      for &b in &row[j + 1..] {
        let cosine = ((points[a].x - points[i].x) * (points[b].x - points[i].x)
          + (points[a].y - points[i].y) * (points[b].y - points[i].y))
          / (points[a].distance(points[i]) * points[b].distance(points[i])).max(1e-12);
        cost += ANGLE_WEIGHT * (cosine.clamp(-1., 1.).acos() - std::f64::consts::TAU / 3.).powi(2);
      }
    }
  }
  let n = points.len() as f64;
  let mean = Point::new(
    points.iter().map(|p| p.x).sum::<f64>() / n,
    points.iter().map(|p| p.y).sum::<f64>() / n,
  );
  cost + EXTENT_WEIGHT * points.iter().map(|p| p.distance(mean).powi(2)).sum::<f64>() / n
}
