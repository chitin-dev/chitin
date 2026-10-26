//! Final-scene contacts, retaining logical labels and filled/hashed bond sources.
//!
//! Glyph runs, color halves and hash strokes are deduplicated by source identity.
//! A depiction-only H is separate from its host label without allocating graph IDs.

use super::{DrawingContact, depict};
use crate::scene::{Drawing, Primitive, Source};
use chitin_chem::MolecularGraph;
use std::collections::BTreeSet;

mod region;
pub(super) use region::label_region;

/// Inspects text and charge-circle footprints against labels and nonincident strokes.
///
/// # Parameters
/// - `graph`: immutable namespace owning all referenced atoms and bonds.
/// - `drawing`: validated complete scene, including separately drawn chiral H.
///
/// # Returns
/// Stable, deduplicated contacts. Ordinary labels exclude their incident source
/// bonds; an expanded H label does not inherit that exemption. Its own C-H mark
/// is excluded. Atom-sourced circles reserve their enclosed disk, including the
/// outline width, and share their text's logical source. Bond-sourced aromatic
/// circles are not labels. Curved bond strokes and independent chemical decoding
/// remain outside this inspector's contract. Groups are traversed, but this single-graph
/// inspector is not an instance-aware reaction-diagram quality evaluator.
///
/// # Errors
/// Rejects foreign scene handles and unsupported measured text. It never moves
/// coordinates or substitutes glyph metrics from a different font.
pub fn contacts(graph: &MolecularGraph, drawing: &Drawing) -> Result<Vec<DrawingContact>, depict::DepictError> {
  let mut labels = Vec::new();
  let mut strokes = Vec::new();
  for primitive in drawing.leaf_primitives() {
    let source = match primitive {
      Primitive::CircularArc(arc) => arc.source,
      Primitive::Text(text) => text.source,
      Primitive::Line(line) => line.source,
      Primitive::Polygon(polygon) => polygon.source,
      Primitive::Circle(circle) => circle.source,
      Primitive::Disk(disk) => disk.source,
      Primitive::CubicBezier(curve) => curve.source,
      Primitive::Group(_) => None,
    };
    match source {
      Some(Source::Atom(id) | Source::AttachedHydrogen(id)) => {
        graph.atom(id).ok_or(chitin_chem::GraphError::MissingAtom(id))?;
      }
      Some(Source::Bond(id)) => {
        graph.bond(id).ok_or(chitin_chem::GraphError::MissingBond(id))?;
      }
      None => {}
    }
    if let Some(label) = label_region(primitive)? {
      labels.push(label);
    }
    if let (
      Primitive::Line(_) | Primitive::Polygon(_),
      Some(source @ (Source::Bond(_) | Source::AttachedHydrogen(_))),
    ) = (primitive, source)
    {
      strokes.push((source, primitive));
    }
  }
  let mut result = BTreeSet::new();
  for (i, &(label, footprint)) in labels.iter().enumerate() {
    for &(other, region) in &labels[i + 1..] {
      if label != other && footprint.overlaps(region) {
        let contact = match (label, other) {
          (Source::Atom(a), Source::Atom(b)) => DrawingContact::Labels(a.min(b), a.max(b)),
          _ => DrawingContact::AuxiliaryLabels(label.min(other), label.max(other)),
        };
        result.insert(contact);
      }
    }
    for &(stroke, primitive) in &strokes {
      let incident = match (label, stroke) {
        (Source::Atom(atom), Source::Bond(bond)) => graph.bond(bond).is_some_and(|bond| bond.atoms().contains(&atom)),
        (Source::Atom(atom) | Source::AttachedHydrogen(atom), Source::AttachedHydrogen(host)) => atom == host,
        _ => false,
      };
      if !incident && footprint.touches(primitive) {
        result.insert(match (label, stroke) {
          (Source::Atom(atom), Source::Bond(bond)) => DrawingContact::LabelBond(atom, bond),
          _ => DrawingContact::AuxiliaryLabelStroke(label, stroke),
        });
      }
    }
  }
  Ok(result.into_iter().collect())
}
