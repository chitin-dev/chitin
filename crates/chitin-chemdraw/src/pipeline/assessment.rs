//! Final candidate acceptance uses separate contact channels, never a weighted sum.

use super::{DrawingContact, RenderError, RenderedMolecule, contacts, no_new_geometry_contacts};
use crate::{
  depict::{self, DepictOptions},
  layout::{self, Layout2d},
};
use chitin_chem::MolecularGraph;

/// Owns the last completely drawn, accepted result while search explores proxies.
pub(super) struct Champion {
  /// The mandatory baseline is never replaced by a partial or undrawable result.
  result: RenderedMolecule,
  /// All comparisons use exact rendered label contacts and the same skeleton screen.
  score: Assessment,
  /// Immutable feasibility bounds limit local exceptions without cumulative drift.
  geometry: super::geometry::GeometryGuard,
}

impl Champion {
  /// Establishes a valid original result before considering optional proposals.
  pub(super) fn new(graph: &MolecularGraph, result: RenderedMolecule, spacing: f64) -> Result<Self, RenderError> {
    Ok(Self {
      score: Assessment::measure(graph, &result, spacing)?,
      geometry: super::geometry::GeometryGuard::new(graph, &result.layout)?,
      result,
    })
  }

  /// Considers a complete proposal without allowing proxy scores to commit it.
  ///
  /// Undrawable optional candidates are discarded. A successful redraw must
  /// improve at least one channel without increasing any other; equal candidates
  /// keep the earlier champion. The old source-level crowd/crossing identity
  /// protection is retained in addition to the new atom-edge count check.
  pub(super) fn consider(
    &mut self,
    graph: &MolecularGraph,
    layout: Layout2d,
    options: &DepictOptions,
    spacing: f64,
  ) -> Result<(), RenderError> {
    if !self.geometry.permits(&layout) {
      return Ok(());
    }
    let Ok(drawing) = depict::depict(graph, &layout, options) else {
      return Ok(());
    };
    let proposal = RenderedMolecule {
      contacts: contacts(graph, &drawing)?,
      layout,
      drawing,
    };
    let score = Assessment::measure(graph, &proposal, spacing)?;
    if score.improves(self.score) && no_new_geometry_contacts(&self.result.layout, &proposal.layout) {
      self.result = proposal;
      self.score = score;
    }
    Ok(())
  }

  /// Returns the owned complete champion, not the last search proposal.
  pub(super) fn finish(self) -> RenderedMolecule {
    self.result
  }

  /// Returns the currently accepted scene for contact-driven neighborhood search.
  pub(super) fn result(&self) -> &RenderedMolecule {
    &self.result
  }
}

/// Five independently protected counts; font-independent search may use proxies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Assessment([usize; 5]);

impl Assessment {
  /// Triggers optional assembly search only for a currently unresolved contact.
  pub(super) fn has_contacts(self) -> bool {
    self.0.iter().any(|&count| count > 0)
  }
  /// Measures the complete candidate only after it has been successfully depicted.
  pub(super) fn measure(graph: &MolecularGraph, result: &RenderedMolecule, spacing: f64) -> Result<Self, RenderError> {
    let [atoms, atom_edges, crossings] = layout::contact_counts(graph, &result.layout, spacing)?;
    let [labels, label_bonds] = label_counts(&result.contacts);
    Ok(Self([atoms, atom_edges, crossings, labels, label_bonds]))
  }

  /// Requires Pareto improvement: no channel can purchase an increase in another.
  pub(super) fn improves(self, previous: Self) -> bool {
    self != previous && self.0.iter().zip(previous.0).all(|(&after, before)| after <= before)
  }
}

/// Admits a fully rendered independent seed without freezing old contact IDs.
///
/// # Returns
/// The baseline unless every measured contact channel is nonincreasing and at
/// least one improves. Geometry limits are captured from the immutable baseline;
/// scored ideal angles and supplied alkene sides stay protected. Automatic bond
/// lengths can recover toward unit length without cumulative bound drift.
///
/// # Errors
/// Inconsistent source positions or scene measurement errors propagate.
pub(super) fn prefer_embedding(
  graph: &MolecularGraph,
  reference: &Layout2d,
  baseline: RenderedMolecule,
  candidate: RenderedMolecule,
  spacing: f64,
) -> Result<RenderedMolecule, RenderError> {
  let guard = super::geometry::GeometryGuard::reembedding(graph, reference)?;
  if guard.permits(&candidate.layout)
    && Assessment::measure(graph, &candidate, spacing)?.improves(Assessment::measure(graph, &baseline, spacing)?)
    && super::edges::count(graph, &candidate.layout)? <= super::edges::count(graph, &baseline.layout)?
  {
    Ok(candidate)
  } else {
    Ok(baseline)
  }
}

/// Counts logical label pairs separately from label/stroke pairs, including H.
fn label_counts(contacts: &[DrawingContact]) -> [usize; 2] {
  let mut result = [0; 2];
  for contact in contacts {
    let category = match contact {
      DrawingContact::Labels(..) | DrawingContact::AuxiliaryLabels(..) => 0,
      DrawingContact::LabelBond(..) | DrawingContact::AuxiliaryLabelStroke(..) => 1,
    };
    result[category] += 1;
  }
  result
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::geometry::Point;
  use chitin_chem::io::smiles::read_smiles;

  type TestResult = Result<(), Box<dyn std::error::Error>>;

  #[test]
  fn a_later_worse_or_incomplete_proposal_should_not_replace_the_champion() -> TestResult {
    let graph = read_smiles("O.O")?.into_graph();
    let ids: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
    let options = DepictOptions::default();
    let crowded = Layout2d::from_positions([(ids[0], Point::new(0., 0.)), (ids[1], Point::new(0.3, 0.))].into())?;
    let drawing = depict::depict(&graph, &crowded, &options)?;
    let original = RenderedMolecule {
      contacts: contacts(&graph, &drawing)?,
      drawing,
      layout: crowded.clone(),
    };
    let clear = Layout2d::from_positions([(ids[0], Point::new(0., 0.)), (ids[1], Point::new(3., 0.))].into())?;
    let mut champion = Champion::new(&graph, original, 0.55)?;
    champion.consider(&graph, clear.clone(), &options, 0.55)?;
    champion.consider(&graph, crowded, &options, 0.55)?;
    champion.consider(
      &graph,
      Layout2d::from_positions([(ids[0], Point::new(0., 0.))].into())?,
      &options,
      0.55,
    )?;
    let result = champion.finish();
    assert_eq!(result.layout, clear);
    assert!(result.contacts.is_empty());
    Ok(())
  }

  #[test]
  fn fewer_total_contacts_should_not_purchase_new_label_overlap() {
    assert!(!Assessment([0, 0, 0, 1, 0]).improves(Assessment([0, 0, 0, 0, 3])));
  }

  #[test]
  fn fewer_labels_should_not_purchase_an_atom_edge_contact() {
    assert!(!Assessment([0, 1, 0, 0, 0]).improves(Assessment([0, 0, 0, 2, 0])));
  }

  #[test]
  fn equal_contact_counts_should_keep_the_existing_champion() {
    assert!(!Assessment([1, 2, 3, 4, 5]).improves(Assessment([1, 2, 3, 4, 5])));
  }

  #[test]
  fn removing_one_contact_without_a_trade_should_improve() {
    assert!(Assessment([1, 2, 3, 4, 4]).improves(Assessment([1, 2, 3, 4, 5])));
  }
}
