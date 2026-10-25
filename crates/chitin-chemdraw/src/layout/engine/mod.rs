//! Source-ordered fragment construction and typed drawing simulation.
//! Source attribution and redistribution terms are in licenses/coordinate-generation.txt.

use super::{Layout2d, LayoutDiagnostic, LayoutError as Error, LayoutOptions, budget::Work};
use crate::geometry::Point;
use chitin_chem::MolecularGraph;
use std::collections::BTreeMap;
mod assembly;
mod backbone;
mod clashes;
mod cleanup;
mod constraints;
mod contours;
mod cycles;
mod evaluation;
mod fragments;
mod interactions;
mod macrocycles;
mod math;
mod moves;
mod opening;
mod ordering;
mod orientation;
mod priority;
mod ranking;
mod refinement;
mod rings;
mod rotation;
mod scopes;
mod search;
mod solver;
mod stereo;
mod templates;
mod topology;

/// Assembles source-ordered fragment frames and optionally minimizes their geometry.
///
/// # Returns
/// Native polygons, templates, constrained contours and temporary ring opening
/// provide all coordinates. No compatibility seed is accepted or requested.
///
/// # Errors
/// Source, finite-range, complete-coverage and bounded-work checks reject the
/// complete candidate; no compatibility seed is silently substituted here.
pub(super) fn generate(graph: &MolecularGraph, options: &LayoutOptions) -> Result<Layout2d, Error> {
  let mut work = Work::new(options.work_budget);
  let model = topology::Model::canonical(graph, &mut work)?;
  let forest = fragments::Forest::new(&model, &mut work)?;
  let mut points = vec![math::Vector::default(); model.atoms.len()];
  let seeded = rings::seed(&model, &forest, &mut points, &mut work)?;
  if let Some(&atom) = model.cycles.iter().flatten().find(|&&atom| !seeded.rebuilt[atom]) {
    return Err(Error::FragmentEmbeddingIncomplete(model.atoms[atom].id));
  }
  let rigid = seeded.rigid;
  let assembly = assembly::prepare(&model, &forest, &points, &mut work)?;
  let mut search_limit = None;
  let mut points = if options.refinement_passes > 0 {
    let refined = refinement::run(
      &model,
      &forest,
      &assembly,
      &rigid,
      seeded.requires_minimization,
      &mut work,
    )?;
    search_limit = refined.limited_search;
    refined.points
  } else {
    assembly.points
  };
  rotation::apply(&model, &forest, &mut points, &mut work)?;
  // Disconnected components are packed only after their independent simulations.
  let mut offset = 0.;
  for component in model.components.iter().filter(|_| model.components.len() > 1) {
    let min = component.iter().map(|&i| points[i].x).fold(f32::INFINITY, f32::min);
    let max = component.iter().map(|&i| points[i].x).fold(f32::NEG_INFINITY, f32::max);
    for &i in component {
      points[i].x += offset - min;
    }
    offset += max - min + options.component_gap as f32 * math::BOND;
  }
  stereo::enforce(&model, &mut points, &mut Work::new(options.stereo_budget)).map_err(|error| {
    if matches!(error, Error::RingSearchBudgetExceeded) {
      Error::StereoBudgetExceeded
    } else {
      error
    }
  })?;
  let positions: BTreeMap<_, _> = model
    .atoms
    .iter()
    .zip(&points)
    .map(|(atom, p)| {
      (
        atom.id,
        Point::new(f64::from(p.x / math::BOND), f64::from(p.y / math::BOND)),
      )
    })
    .collect();
  let mut layout = Layout2d::from_positions(positions)?;
  if let Some(evaluations) = search_limit {
    layout
      .diagnostics
      .push(LayoutDiagnostic::FragmentSearchLimited { evaluations });
  }
  let points: Vec<_> = model.atoms.iter().map(|atom| layout.positions[&atom.id]).collect();
  let edges: Vec<_> = model.edges.iter().map(|edge| edge.ends).collect();
  let quality = work.measure(&points, &edges, options.minimum_spacing)?;
  for (a, b) in quality.crowds {
    layout.diagnostics.push(LayoutDiagnostic::CrowdedAtoms {
      a: model.atoms[a].id,
      b: model.atoms[b].id,
    });
  }
  for (a, b) in quality.crossings {
    layout.diagnostics.push(LayoutDiagnostic::CrossingBonds {
      a: model.edges[a].id,
      b: model.edges[b].id,
    });
  }
  Ok(layout)
}

#[cfg(test)]
mod tests;
