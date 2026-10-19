//! Dense macrocycles need a readable cavity without editing chemical identity.

use chitin_chem::{
  MolecularGraph,
  io::smiles::{read_smiles, write_smiles},
  perception::analyze_rings,
};
use chitin_chemdraw::{
  depict,
  geometry::{Bounds, Point},
  layout::{self, LayoutDiagnostic},
  scene::{Primitive, Source},
  svg,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Checks labels against other atoms' labels and nonincident bond strokes.
fn readable_labels(graph: &MolecularGraph, drawing: &chitin_chemdraw::Drawing) -> TestResult {
  let labels: Vec<_> = drawing
    .primitives()
    .iter()
    .filter_map(|p| {
      if let Primitive::Text(text) = p {
        Some(text)
      } else {
        None
      }
    })
    .collect();
  for (i, text) in labels.iter().enumerate() {
    let [a, b] = text.envelope()?;
    for other in labels.iter().skip(i + 1) {
      if text.source == other.source {
        continue;
      }
      let [c, d] = other.envelope()?;
      assert!(
        b.x <= c.x || d.x <= a.x || b.y <= c.y || d.y <= a.y,
        "overlapping labels {} and {}",
        text.content,
        other.content
      );
    }
    for primitive in drawing.primitives() {
      let Primitive::Line(line) = primitive else { continue };
      let (Some(Source::Atom(atom)), Some(Source::Bond(edge))) = (text.source, line.source) else {
        continue;
      };
      if graph.bond(edge).is_some_and(|bond| bond.atoms().contains(&atom)) {
        continue;
      }
      assert!(
        !intersects(line.start, line.end, a, b),
        "bond {edge} intersects label {}",
        text.content
      );
    }
  }
  Ok(())
}

/// Clips a segment against a label's axis-aligned measured rectangle.
fn intersects(start: Point, end: Point, min: Point, max: Point) -> bool {
  let (mut enter, mut leave) = (0_f64, 1_f64);
  for (origin, delta, low, high) in [
    (start.x, end.x - start.x, min.x, max.x),
    (start.y, end.y - start.y, min.y, max.y),
  ] {
    if delta.abs() < 1e-12 {
      if origin < low || origin > high {
        return false;
      }
    } else {
      let a = (low - origin) / delta;
      let b = (high - origin) / delta;
      enter = enter.max(a.min(b));
      leave = leave.min(a.max(b));
      if enter > leave {
        return false;
      }
    }
  }
  true
}

#[test]
fn cyclosporine_should_have_a_clear_macrocycle_and_readable_labels() -> TestResult {
  let graph = read_smiles(include_str!("fixtures/cyclosporine.smi").trim())?.into_graph();
  let before = graph.clone();
  let written = write_smiles(&graph)?;
  let coordinates = layout::generate(&graph, &Default::default())?;
  assert!(
    !coordinates.diagnostics().iter().any(|d| matches!(
      d,
      LayoutDiagnostic::CrowdedAtoms { .. } | LayoutDiagnostic::CrossingBonds { .. }
    )),
    "{:?}",
    coordinates.diagnostics()
  );
  let rings = analyze_rings(&graph)?;
  let block = rings
    .blocks()
    .iter()
    .find(|block| block.cycle_rank() == 1)
    .ok_or("missing macrocycle")?;
  assert_eq!(block.atoms().len(), 33);
  let points = block
    .atoms()
    .iter()
    .map(|&id| coordinates.position(id).ok_or("missing ring atom"))
    .collect::<Result<Vec<_>, _>>()?;
  let bounds = Bounds::from_points(points.iter().copied())?.ok_or("missing ring bounds")?;
  assert!(bounds.width().min(bounds.height()) > 5., "ring bounds {bounds:?}");
  for (_, bond) in graph.bonds() {
    let [a, b] = bond.atoms();
    let distance = coordinates
      .position(a)
      .ok_or("missing atom")?
      .distance(coordinates.position(b).ok_or("missing atom")?);
    assert!((distance - 1.).abs() < 1e-8, "bond length {distance}");
  }
  let drawing = depict::depict(&graph, &coordinates, &Default::default())?;
  readable_labels(&graph, &drawing)?;
  assert!(drawing.primitives().iter().any(|p| matches!(p, Primitive::Polygon(_))));
  assert_eq!(graph, before);
  assert_eq!(write_smiles(&graph)?, written);
  assert_eq!(coordinates, layout::generate(&graph, &Default::default())?);
  assert!(svg::export(&drawing, &Default::default())?.contains("<svg"));
  Ok(())
}

#[test]
fn alternate_traversals_should_preserve_readable_macrocycle_depiction() -> TestResult {
  let original = read_smiles(include_str!("fixtures/cyclosporine.smi").trim())?.into_graph();
  let rewritten = write_smiles(&original)?;
  let alternative = concat!(
    "CC[C@H]1C(=O)N(CC(=O)N([C@H](C(=O)N[C@H](C(=O)N([C@H](C(=O)N[C@H](C(=",
    "O)N[C@@H](C(=O)N([C@H](C(=O)N([C@H](C(=O)N([C@H](C(=O)N([C@H](C(=O)N1)",
    "[C@@H]([C@H](C)C/C=C/C)O)C)C(C)C)C)CC(C)C)C)CC(C)C)C)C)C)CC(C)C)C)C(C)C)CC(C)C)C)C"
  );
  for source in [rewritten.as_str(), alternative] {
    let graph = read_smiles(source)?.into_graph();
    assert_eq!(graph.atom_count(), original.atom_count());
    let positions = layout::generate(&graph, &Default::default())?;
    assert!(
      !positions.diagnostics().iter().any(|d| matches!(
        d,
        LayoutDiagnostic::CrowdedAtoms { .. } | LayoutDiagnostic::CrossingBonds { .. }
      )),
      "{:?}",
      positions.diagnostics()
    );
    readable_labels(&graph, &depict::depict(&graph, &positions, &Default::default())?)?;
  }
  Ok(())
}

#[test]
fn densely_substituted_large_rings_should_not_require_a_named_molecule_template() -> TestResult {
  for size in [25, 26, 27, 33, 34, 39, 40] {
    let source = format!("C1(O){}C1O", "C(O)".repeat(size - 2));
    let graph = read_smiles(&source)?.into_graph();
    let positions = layout::generate(&graph, &Default::default())?;
    assert!(
      !positions.diagnostics().iter().any(|d| matches!(
        d,
        LayoutDiagnostic::CrowdedAtoms { .. } | LayoutDiagnostic::CrossingBonds { .. }
      )),
      "size {size}: {:?}",
      positions.diagnostics()
    );
    readable_labels(&graph, &depict::depict(&graph, &positions, &Default::default())?)?;
  }
  Ok(())
}
