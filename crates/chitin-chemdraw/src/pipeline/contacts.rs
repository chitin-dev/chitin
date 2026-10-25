//! Final-scene contacts, retaining logical labels and filled/hashed bond sources.
//!
//! Glyph runs, color halves and hash strokes are deduplicated by source identity.
//! A depiction-only H is separate from its host label without allocating graph IDs.

use super::{DrawingContact, depict};
use crate::{
  geometry::{Point, segment_enters_box},
  scene::{Drawing, Primitive, Source},
};
use chitin_chem::MolecularGraph;
use std::collections::BTreeSet;

/// Inspects measured text against nonincident line strokes and filled polygons.
///
/// # Parameters
/// - `graph`: immutable namespace owning all referenced atoms and bonds.
/// - `drawing`: validated complete scene, including separately drawn chiral H.
///
/// # Returns
/// Stable, deduplicated contacts. Ordinary labels exclude their incident source
/// bonds; an expanded H label does not inherit that exemption. Its own C-H mark
/// is excluded. Curved-circle ink and independent chemical decoding are outside
/// this inspector's contract. Groups are traversed, but this single-graph
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
    match (primitive, source) {
      (Primitive::Text(text), Some(source @ (Source::Atom(_) | Source::AttachedHydrogen(_)))) => {
        labels.push((source, text.envelope()?));
      }
      (Primitive::Line(_) | Primitive::Polygon(_), Some(source @ (Source::Bond(_) | Source::AttachedHydrogen(_)))) => {
        strokes.push((source, primitive));
      }
      _ => {}
    }
  }
  let mut result = BTreeSet::new();
  for (i, &(label, bounds)) in labels.iter().enumerate() {
    for &(other, region) in &labels[i + 1..] {
      if label != other
        && bounds[0].x < region[1].x
        && bounds[1].x > region[0].x
        && bounds[0].y < region[1].y
        && bounds[1].y > region[0].y
      {
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
      if !incident && touches(primitive, bounds) {
        result.insert(match (label, stroke) {
          (Source::Atom(atom), Source::Bond(bond)) => DrawingContact::LabelBond(atom, bond),
          _ => DrawingContact::AuxiliaryLabelStroke(label, stroke),
        });
      }
    }
  }
  Ok(result.into_iter().collect())
}

/// Checks actual filled area, not a wedge's usually much larger bounding box.
fn touches(primitive: &Primitive, bounds: [Point; 2]) -> bool {
  match primitive {
    Primitive::Line(line) => segment_enters_box(line.start, line.end, bounds, line.width / 2.),
    Primitive::Polygon(polygon) => {
      let corners = [
        bounds[0],
        Point::new(bounds[1].x, bounds[0].y),
        bounds[1],
        Point::new(bounds[0].x, bounds[1].y),
      ];
      polygon
        .points
        .iter()
        .zip(polygon.points.iter().cycle().skip(1))
        .any(|(&a, &b)| segment_enters_box(a, b, bounds, 0.))
        || corners.iter().any(|&point| inside(point, &polygon.points))
    }
    _ => false,
  }
}

/// Ray parity also catches a label completely enclosed by a large filled wedge.
fn inside(point: Point, polygon: &[Point]) -> bool {
  let mut inside = false;
  for (&a, &b) in polygon.iter().zip(polygon.iter().cycle().skip(1)) {
    if (a.y > point.y) != (b.y > point.y) && point.x < a.x + (b.x - a.x) * (point.y - a.y) / (b.y - a.y) {
      inside = !inside;
    }
  }
  inside
}
