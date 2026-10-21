//! Optional automatic-layout feedback using actual scene text and stroke geometry.
//!
//! Explicit `depict` calls never move caller-owned coordinates. This opt-in
//! orchestration measures final labels, supplies normalized envelopes to layout,
//! then redraws and checks the actual primitives instead of trusting center gaps.

use crate::{
  depict::{self, DepictOptions},
  geometry::{Point, segment_enters_box},
  layout::{self, Layout2d, LayoutOptions},
  scene::{Drawing, Primitive, Source},
};
use chitin_chem::{AtomId, BondId, MolecularGraph};
use std::collections::{BTreeMap, BTreeSet};

/// An unresolved contact in the actual rendered scene, not invalid chemistry.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum DrawingContact {
  /// Two different atom labels occupy overlapping measured envelopes.
  Labels(AtomId, AtomId),
  /// A nonincident source-bond stroke enters an atom-label envelope.
  LabelBond(AtomId, BondId),
}

/// Automatic geometry and its final renderer-independent scene and limitations.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderedMolecule {
  /// Complete geometry, retaining all original handles and chemical constraints.
  pub layout: Layout2d,
  /// Validated scene used by SVG and other exporters without further adjustment.
  pub drawing: Drawing,
  /// Actual label/stroke contacts remaining after bounded optional repair.
  pub contacts: Vec<DrawingContact>,
}

/// A required layout or depiction stage failed; no partial scene is returned.
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
  /// Automatic coordinate generation failed.
  #[error(transparent)]
  Layout(#[from] layout::LayoutError),
  /// Required chemical representation, label measurement or scene construction failed.
  #[error(transparent)]
  Depict(#[from] depict::DepictError),
}

/// Generates and refines an automatic drawing using final label measurements.
///
/// # Parameters
/// - `graph`: immutable chemical source with graph-local handles.
/// - `layout_options`: topology search and geometry constraints.
/// - `depict_options`: the final font, labels, multiple-bond strokes and sizing.
///
/// # Returns
/// A complete scene plus unresolved scene contacts. Label feedback borrows a
/// separate quarter of `refinement_budget`; zero refinement disables movement.
/// Layout remains font-independent and the SVG exporter performs no repairs.
///
/// # Errors
/// Propagates required layout/depiction failures. A failed optional redraw keeps
/// the original complete drawing; it never drops atoms, bonds or source stereo.
pub fn generate(
  graph: &MolecularGraph,
  layout_options: &LayoutOptions,
  depict_options: &DepictOptions,
) -> Result<RenderedMolecule, RenderError> {
  finish(
    graph,
    layout::generate(graph, layout_options)?,
    layout_options,
    depict_options,
  )
}

/// Finishes caller-authorized automatic geometry with bounded label feedback.
///
/// This explicitly permits moving the supplied layout; use `depict::depict`
/// when authored coordinates must remain exact. Coordination generators may use
/// this stage because rigid repair preserves their distinct attachment lengths.
///
/// # Parameters
/// - `graph`, `geometry`: immutable chemistry and complete automatic coordinates.
/// - `layout_options`: geometric repair constraints and the label-work allowance.
/// - `depict_options`: final sizes and font used for measurement and scene output.
///
/// # Returns
/// The best complete redraw and any remaining measured label/stroke contacts.
///
/// # Errors
/// Propagates initial required depiction and envelope errors. Optional candidates
/// must redraw successfully and improve actual contacts without adding skeleton
/// crossings or crowded pairs before replacing the input snapshot.
pub fn finish(
  graph: &MolecularGraph,
  geometry: Layout2d,
  layout_options: &LayoutOptions,
  depict_options: &DepictOptions,
) -> Result<RenderedMolecule, RenderError> {
  let drawing = depict::depict(graph, &geometry, depict_options)?;
  let before = contacts(graph, &drawing)?;
  let mut result = RenderedMolecule {
    layout: geometry,
    drawing,
    contacts: before,
  };
  if result.contacts.is_empty() || layout_options.refinement_passes == 0 {
    return Ok(result);
  }
  let mut regions: BTreeMap<AtomId, [Point; 2]> = BTreeMap::new();
  for p in result.drawing.primitives() {
    let Primitive::Text(text) = p else {
      continue;
    };
    let Some(Source::Atom(id)) = text.source else {
      continue;
    };
    let bounds = text.envelope().map_err(depict::DepictError::from)?;
    let center = result.layout.positions()[&id];
    let bounds = [
      Point::new(
        bounds[0].x / depict_options.bond_length - center.x,
        -bounds[1].y / depict_options.bond_length - center.y,
      ),
      Point::new(
        bounds[1].x / depict_options.bond_length - center.x,
        -bounds[0].y / depict_options.bond_length - center.y,
      ),
    ];
    regions
      .entry(id)
      .and_modify(|old| {
        old[0].x = old[0].x.min(bounds[0].x);
        old[0].y = old[0].y.min(bounds[0].y);
        old[1].x = old[1].x.max(bounds[1].x);
        old[1].y = old[1].y.max(bounds[1].y);
      })
      .or_insert(bounds);
  }
  let mut candidate = result.layout.clone();
  layout::clear_labels(
    graph,
    &mut candidate,
    &regions,
    layout_options,
    layout_options.refinement_budget / 4,
  )?;
  if let Ok(scene) = depict::depict(graph, &candidate, depict_options) {
    let actual = contacts(graph, &scene)?;
    if actual.len() < result.contacts.len() && no_new_geometry_contacts(&result.layout, &candidate) {
      result = RenderedMolecule {
        layout: candidate,
        drawing: scene,
        contacts: actual,
      };
    }
  }
  Ok(result)
}

/// Rejects new geometry contact identities, independently of budget-limit notices.
fn no_new_geometry_contacts(initial: &Layout2d, candidate: &Layout2d) -> bool {
  candidate
    .diagnostics()
    .iter()
    .filter(|d| {
      matches!(
        d,
        layout::LayoutDiagnostic::CrossingBonds { .. } | layout::LayoutDiagnostic::CrowdedAtoms { .. }
      )
    })
    .all(|d| initial.diagnostics().contains(d))
}

/// Inspects actual text envelopes against every nonincident rendered bond stroke.
///
/// # Parameters
/// - `graph`: source namespace owning the scene's atom and bond references.
/// - `drawing`: validated scene with final text metrics and stroke widths.
///
/// # Returns
/// Deduplicated source-level contacts, including auxiliary multiple-bond strokes.
///
/// # Errors
/// Propagates font-envelope failures and rejects stale/foreign source handles.
/// Contacts are deduplicated by source IDs, so
/// split colors, double strokes and multi-run labels do not multiply warnings.
pub fn contacts(graph: &MolecularGraph, drawing: &Drawing) -> Result<Vec<DrawingContact>, depict::DepictError> {
  let mut result = BTreeSet::new();
  let mut labels = Vec::new();
  let mut strokes = Vec::new();
  // Measure each glyph run once, not once per compared label or bond stroke.
  for p in drawing.primitives() {
    match p {
      Primitive::Text(text) => {
        if let Some(Source::Atom(id)) = text.source {
          graph.atom(id).ok_or(chitin_chem::GraphError::MissingAtom(id))?;
          labels.push((id, text.envelope()?));
        }
      }
      Primitive::Line(line) => {
        if let Some(Source::Bond(id)) = line.source {
          let bond = graph.bond(id).ok_or(chitin_chem::GraphError::MissingBond(id))?;
          strokes.push((line, id, bond.atoms()));
        }
      }
      _ => {}
    }
  }
  for (i, &(id, bounds)) in labels.iter().enumerate() {
    for &(other, region) in &labels[i + 1..] {
      if id == other {
        continue;
      }
      if bounds[0].x < region[1].x
        && bounds[1].x > region[0].x
        && bounds[0].y < region[1].y
        && bounds[1].y > region[0].y
      {
        result.insert(DrawingContact::Labels(id.min(other), id.max(other)));
      }
    }
    for &(line, edge, ends) in &strokes {
      if ends.contains(&id) {
        continue;
      }
      if segment_enters_box(line.start, line.end, bounds, line.width / 2.) {
        result.insert(DrawingContact::LabelBond(id, edge));
      }
    }
  }
  Ok(result.into_iter().collect())
}
