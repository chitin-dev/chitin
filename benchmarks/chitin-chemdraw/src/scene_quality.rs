//! Independent final-scene label/stroke checks with source-level deduplication.
//!
//! Measured glyph envelopes are conservative boxes, not raster ink masks.
//! Polygon wedges are checked as filled polygons; split colors and seven hash
//! strokes still represent one source bond, not multiple independent contacts.

use crate::quality::{Metrics, Observation, State};
use anyhow::{Result, ensure};
use chitin_chem::{AtomId, Element, MolecularGraph};
use chitin_chemdraw::{
  geometry::Point,
  scene::{Drawing, Primitive, Source},
};
use std::collections::BTreeSet;

/// One logical label; an expanded hydrogen is distinct from its host's label.
type Label = (AtomId, bool);

/// Adds rendered contacts and source coverage, independently of repair diagnostics.
///
/// # Errors
/// Rejects stale/foreign source handles and failures to measure actual scene text.
/// Curved ink contacts and full label semantics remain explicitly unimplemented.
pub fn measure(graph: &MolecularGraph, drawing: &Drawing, output: &mut Metrics) -> Result<()> {
  let mut labels: Vec<(Label, [Point; 2])> = Vec::new();
  let mut strokes = Vec::new();
  let mut visible = BTreeSet::new();
  let mut circles = 0;
  for primitive in drawing.primitives() {
    let source = match primitive {
      Primitive::Text(text) => text.source,
      Primitive::Line(line) => line.source,
      Primitive::Polygon(polygon) => polygon.source,
      Primitive::Circle(circle) => {
        circles += 1;
        circle.source
      }
      _ => None,
    };
    match source {
      Some(Source::Bond(id) | Source::AromaticRing(id)) => {
        ensure!(graph.bond(id).is_some(), "foreign scene bond");
      }
      Some(Source::Atom(id) | Source::Charge(id) | Source::AttachedHydrogen(id)) => {
        ensure!(graph.atom(id).is_some(), "foreign scene atom");
      }
      None => {}
    }
    match primitive {
      Primitive::Text(text) => {
        if let Some(source) = source {
          let key = match source {
            Source::Atom(id) | Source::Charge(id) => Some((id, false)),
            Source::AttachedHydrogen(id) => Some((id, true)),
            _ => None,
          };
          if let Some(key) = key {
            labels.push((key, text.envelope()?));
          }
        }
      }
      Primitive::Line(_) | Primitive::Polygon(_) => {
        if let Some(source @ (Source::Bond(_) | Source::AttachedHydrogen(_))) = source {
          if let Source::Bond(id) = source {
            visible.insert(id);
          }
          strokes.push((source, primitive));
        }
      }
      _ => {}
    }
  }
  let keys: BTreeSet<_> = labels.iter().map(|(key, _)| *key).collect();
  let mut required_labels = (0, 0);
  for (id, atom) in graph.atoms() {
    let required = !matches!(atom.element(), Element::Carbon | Element::Hydrogen)
      || atom.isotope().is_some()
      || atom.formal_charge().is_some_and(|charge| charge != 0)
      || atom.unpaired_electrons().is_some_and(|count| count > 0)
      || graph.neighbors(id)?.len() == 0;
    if required {
      required_labels.1 += 1;
      required_labels.0 += usize::from(!keys.contains(&(id, false)));
    }
  }
  output.measurements.insert(
    "required_atom_label_coverage".into(),
    Observation::rate(required_labels.0, required_labels.1),
  );
  let mut pair_domains = BTreeSet::new();
  let mut pair_contacts = BTreeSet::new();
  for (i, &(key, region)) in labels.iter().enumerate() {
    for &(other, bounds) in &labels[i + 1..] {
      if key == other {
        continue;
      }
      let pair = (key.min(other), key.max(other));
      pair_domains.insert(pair);
      if region[0].x < bounds[1].x
        && region[1].x > bounds[0].x
        && region[0].y < bounds[1].y
        && region[1].y > bounds[0].y
      {
        pair_contacts.insert(pair);
      }
    }
  }
  let mut bond_domains = BTreeSet::new();
  let mut bond_contacts = BTreeSet::new();
  // Source is not Ord; use disjoint (bond/H, source ordinal) stable local keys.
  let atom_indices: std::collections::BTreeMap<_, _> = graph.atoms().enumerate().map(|(i, (id, _))| (id, i)).collect();
  let bond_indices: std::collections::BTreeMap<_, _> = graph.bonds().enumerate().map(|(i, (id, _))| (id, i)).collect();
  for &(key, region) in &labels {
    for &(source, primitive) in &strokes {
      let locator = match source {
        Source::Bond(id) => {
          let bond = graph.bond(id).ok_or_else(|| anyhow::anyhow!("foreign bond"))?;
          if !key.1 && bond.atoms().contains(&key.0) {
            continue;
          }
          (false, bond_indices[&id])
        }
        Source::AttachedHydrogen(host) => {
          if host == key.0 {
            continue;
          }
          (true, atom_indices[&host])
        }
        _ => continue,
      };
      bond_domains.insert((key, locator));
      if touches_box(primitive, region) {
        bond_contacts.insert((key, locator));
      }
    }
  }
  output.measurements.insert(
    "label_label_overlap".into(),
    Observation::rate(pair_contacts.len(), pair_domains.len()),
  );
  output.measurements.insert(
    "label_bond_overlap".into(),
    Observation::rate(bond_contacts.len(), bond_domains.len()),
  );
  output.measurements.insert(
    "source_bond_coverage".into(),
    Observation::rate(graph.bond_count() - visible.len(), graph.bond_count()),
  );
  output.measurements.insert(
    "curved_ink_contacts".into(),
    Observation::unavailable(
      if circles > 0 {
        State::NotImplemented
      } else {
        State::NotApplicable
      },
      circles,
      "aromatic and charge-circle ink contacts are not yet evaluated",
    ),
  );
  output.measurements.insert(
    "measured_labels".into(),
    Observation::measured(keys.len() as f64, 1, None),
  );
  output.finish();
  Ok(())
}

/// Filled wedge intersection or a line stroke's conservative half-width envelope.
fn touches_box(primitive: &Primitive, region: [Point; 2]) -> bool {
  match primitive {
    Primitive::Line(line) => segment_box(
      line.start,
      line.end,
      [
        Point::new(region[0].x - line.width / 2., region[0].y - line.width / 2.),
        Point::new(region[1].x + line.width / 2., region[1].y + line.width / 2.),
      ],
    ),
    Primitive::Polygon(polygon) => {
      let corners = [
        region[0],
        Point::new(region[1].x, region[0].y),
        region[1],
        Point::new(region[0].x, region[1].y),
      ];
      polygon
        .points
        .iter()
        .zip(polygon.points.iter().cycle().skip(1))
        .any(|(&a, &b)| segment_box(a, b, region))
        || corners.iter().any(|&point| inside_polygon(point, &polygon.points))
    }
    _ => false,
  }
}

/// Closed segment/axis-aligned box intersection using parameter interval clipping.
fn segment_box(a: Point, b: Point, region: [Point; 2]) -> bool {
  let mut low: f64 = 0.;
  let mut high: f64 = 1.;
  for (origin, delta, min, max) in [
    (a.x, b.x - a.x, region[0].x, region[1].x),
    (a.y, b.y - a.y, region[0].y, region[1].y),
  ] {
    if delta == 0. {
      if origin < min || origin > max {
        return false;
      }
    } else {
      let t1 = (min - origin) / delta;
      let t2 = (max - origin) / delta;
      low = low.max(t1.min(t2));
      high = high.min(t1.max(t2));
      if low > high {
        return false;
      }
    }
  }
  true
}

/// Ray parity for the actual filled polygon, not its much larger bounding box.
fn inside_polygon(point: Point, vertices: &[Point]) -> bool {
  let mut inside = false;
  for (&a, &b) in vertices.iter().zip(vertices.iter().cycle().skip(1)) {
    if (a.y > point.y) != (b.y > point.y) && point.x < (b.x - a.x) * (point.y - a.y) / (b.y - a.y) + a.x {
      inside = !inside;
    }
  }
  inside
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn omitted_source_bonds_cannot_pass_the_readability_screen() -> Result<()> {
    let graph = chitin_chem::io::smiles::read_smiles("CC")?.into_graph();
    let coordinates = chitin_chemdraw::layout::generate(&graph, &Default::default())?;
    let mut metrics = crate::metrics::measure(&graph, &coordinates)?;
    measure(&graph, &Drawing::new(vec![])?, &mut metrics)?;
    assert_eq!(metrics.measurements["source_bond_coverage"].violations, Some(1));
    assert_eq!(metrics.readability_clear, Some(false));
    Ok(())
  }

  #[test]
  fn empty_heteroatom_diagram_cannot_pass_source_coverage() -> Result<()> {
    let graph = chitin_chem::io::smiles::read_smiles("O")?.into_graph();
    let coordinates = chitin_chemdraw::layout::generate(&graph, &Default::default())?;
    let mut metrics = crate::metrics::measure(&graph, &coordinates)?;
    measure(&graph, &Drawing::new(vec![])?, &mut metrics)?;
    assert_eq!(metrics.measurements["required_atom_label_coverage"].violations, Some(1));
    assert_eq!(metrics.readability_clear, Some(false));
    Ok(())
  }

  #[test]
  fn unavailable_pair_scale_is_not_a_clear_readability_result() -> Result<()> {
    let graph = chitin_chem::io::smiles::read_smiles("[Na+].[Cl-]")?.into_graph();
    let coordinates = chitin_chemdraw::layout::generate(&graph, &Default::default())?;
    let drawing = chitin_chemdraw::depict::depict(&graph, &coordinates, &Default::default())?;
    let mut metrics = crate::metrics::measure(&graph, &coordinates)?;
    measure(&graph, &drawing, &mut metrics)?;
    assert_eq!(metrics.measurements["nonbonded_atom_proximity"].eligible, 1);
    assert_eq!(metrics.readability_clear, None);
    Ok(())
  }

  #[test]
  fn duplicate_strokes_do_not_multiply_label_bond_contacts() -> Result<()> {
    let graph = chitin_chem::io::smiles::read_smiles("O.CC")?.into_graph();
    let coordinates = chitin_chemdraw::layout::Layout2d::from_positions(
      graph
        .atoms()
        .map(|(id, _)| id)
        .zip([Point::new(0., 0.), Point::new(-1., 0.), Point::new(1., 0.)])
        .collect(),
    )?;
    let drawing = chitin_chemdraw::depict::depict(&graph, &coordinates, &Default::default())?;
    let mut duplicated = drawing.primitives().to_vec();
    duplicated.extend_from_slice(drawing.primitives());
    let mut result = crate::metrics::measure(&graph, &coordinates)?;
    measure(&graph, &Drawing::new(duplicated)?, &mut result)?;
    assert_eq!(result.measurements["label_bond_overlap"].violations, Some(1));
    assert_eq!(result.measurements["label_bond_overlap"].eligible, 1);
    Ok(())
  }

  #[test]
  fn filled_wedge_contact_is_not_reduced_to_its_boundary() {
    let wedge = Primitive::Polygon(chitin_chemdraw::scene::Polygon {
      points: vec![Point::new(-5., -5.), Point::new(5., -5.), Point::new(0., 5.)],
      color: chitin_chemdraw::scene::Color::BLACK,
      source: None,
    });
    assert!(touches_box(&wedge, [Point::new(-0.1, -0.1), Point::new(0.1, 0.1)]));
    assert!(!touches_box(&wedge, [Point::new(3., 3.), Point::new(4., 4.)]));
  }

  #[test]
  fn clipping_rejects_an_edge_outside_the_label_box() {
    assert!(!segment_box(
      Point::new(-2., 2.),
      Point::new(2., 2.),
      [Point::new(-1., -1.), Point::new(1., 1.)]
    ));
  }

  #[test]
  fn clipping_detects_a_segment_contained_in_the_label_box() {
    assert!(segment_box(
      Point::new(0., 0.),
      Point::new(0.5, 0.5),
      [Point::new(-1., -1.), Point::new(1., 1.)]
    ));
  }
}
