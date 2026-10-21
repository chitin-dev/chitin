//! Optional automatic-layout feedback using actual scene text and stroke geometry.
//!
//! Explicit `depict` calls never move caller-owned coordinates. This opt-in
//! orchestration measures final labels, supplies normalized envelopes to layout,
//! then redraws and checks the actual primitives instead of trusting center gaps.

use crate::{
  depict::{self, DepictOptions},
  geometry::Point,
  layout::{self, Layout2d, LayoutOptions},
  scene::{Drawing, Primitive, Source},
};
use chitin_chem::{AtomId, BondId, MolecularGraph};
use std::collections::BTreeMap;

mod assessment;
mod contacts;
mod geometry;
pub use contacts::contacts;

/// An unresolved contact in the actual rendered scene, not invalid chemistry.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum DrawingContact {
  /// Two different atom labels occupy overlapping measured envelopes.
  Labels(AtomId, AtomId),
  /// A nonincident source-bond stroke enters an atom-label envelope.
  LabelBond(AtomId, BondId),
  /// Label envelopes involving a separately drawn H; each source is Atom or AttachedHydrogen.
  AuxiliaryLabels(Source, Source),
  /// A label/stroke contact involving depiction-only H geometry, not a source graph edge.
  /// The label is Atom/AttachedHydrogen; the stroke is Bond/AttachedHydrogen.
  AuxiliaryLabelStroke(Source, Source),
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
/// separate quarter of `refinement_budget`; exact terminal-label trials use at
/// most eight proposals and another sixteenth. Zero refinement disables movement.
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
/// The complete champion and any remaining measured label/stroke contacts.
/// Original, local and combined-search proposals are compared after depiction;
/// label pairs and label/stroke pairs are protected separately, alongside
/// skeleton atom, atom-edge and proper-crossing counts.
/// Every admitted proposal preserves source bond lengths and multiple-bond
/// geometry, cannot worsen scored chain/small-ring angles, and must retain
/// explicit alkene relations. Bounds refer to stage entry, not a later proposal.
///
/// # Errors
/// Propagates initial required depiction and envelope errors. Optional candidates
/// must redraw successfully and improve actual contacts without adding skeleton
/// crossings or crowded pairs before replacing the input snapshot.
/// A final bounded terminal-label neighborhood is ranked after actual depiction,
/// so a proxy winner cannot hide another feasible, clearer local candidate.
pub fn finish(
  graph: &MolecularGraph,
  geometry: Layout2d,
  layout_options: &LayoutOptions,
  depict_options: &DepictOptions,
) -> Result<RenderedMolecule, RenderError> {
  let drawing = depict::depict(graph, &geometry, depict_options)?;
  let before = contacts(graph, &drawing)?;
  let result = RenderedMolecule {
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
  let local = layout::clear_labels(
    graph,
    &mut candidate,
    &regions,
    layout_options,
    layout_options.refinement_budget / 4,
  )?;
  let mut champion = assessment::Champion::new(graph, result, layout_options.minimum_spacing)?;
  for proposal in local.into_iter().chain(std::iter::once(candidate)) {
    champion.consider(graph, proposal, depict_options, layout_options.minimum_spacing)?;
  }
  let labels = champion.result().contacts.iter().flat_map(contact_atoms).collect();
  for proposal in layout::terminal_label_trials(
    graph,
    &champion.result().layout,
    &labels,
    layout_options,
    layout_options.refinement_budget / 16,
  )? {
    champion.consider(graph, proposal, depict_options, layout_options.minimum_spacing)?;
  }
  Ok(champion.finish())
}

/// Extracts source label hosts without mistaking bond identifiers for atoms.
fn contact_atoms(contact: &DrawingContact) -> impl Iterator<Item = AtomId> {
  let sources = match *contact {
    DrawingContact::Labels(a, b) => [Some(Source::Atom(a)), Some(Source::Atom(b))],
    DrawingContact::LabelBond(a, _) => [Some(Source::Atom(a)), None],
    DrawingContact::AuxiliaryLabels(a, b) => [Some(a), Some(b)],
    DrawingContact::AuxiliaryLabelStroke(a, _) => [Some(a), None],
  };
  sources.into_iter().flatten().filter_map(|source| match source {
    Source::Atom(atom) | Source::AttachedHydrogen(atom) => Some(atom),
    Source::Bond(_) => None,
  })
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
