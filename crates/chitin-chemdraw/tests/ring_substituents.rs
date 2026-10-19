//! Fused-ring substituents must not occupy an already embedded ring direction.

use chitin_chem::{
  AtomId, Element, MolecularGraph, StereoLigand, TetrahedralWinding,
  io::smiles::{read_smiles, write_smiles},
  perception::analyze_rings,
};
use chitin_chemdraw::{
  depict::{self, ChiralHydrogenMode, DepictOptions},
  layout::{self, LayoutOptions},
  scene::{Primitive, Source},
  svg,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Locally authored cases include two supplied steroid inputs and a minimal fusion.
const CASES: &[&str] = &[
  "CC12CCCCC1CCCC2",
  "C[C@]12CC[C@H]3[C@@H](CCC4=CC(=O)CC[C@@]34C)[C@@H]1CC[C@@H]2O",
  "C[C@H](CCCC(C)C)[C@H]1CC[C@@H]2[C@@]1(CC[C@H]3[C@H]2CC=C4[C@@]3(CC[C@@H](C4)O)C)C",
  "C1CCCCC1C2(C)CC3CC4CCCCC4CC3C2",
];

/// Finds methyl-like terminal carbons at four-coordinate fused-ring junctions.
///
/// Topology, not a fixed source atom number, identifies the affected branches.
fn methyls(graph: &MolecularGraph) -> Result<Vec<(AtomId, AtomId)>, Box<dyn std::error::Error>> {
  let rings = analyze_rings(graph)?;
  let mut result = Vec::new();
  for (id, atom) in graph.atoms() {
    if atom.element() != Element::Carbon || graph.neighbors(id)?.len() != 1 {
      continue;
    }
    let center = graph
      .neighbors(id)?
      .next()
      .map(|(id, _)| id)
      .ok_or("missing methyl host")?;
    if graph.neighbors(center)?.len() == 4
      && rings
        .blocks()
        .iter()
        .any(|block| block.cycle_rank() > 1 && block.atoms().contains(&center))
    {
      result.push((center, id));
    }
  }
  Ok(result)
}

#[test]
fn fused_junction_methyls_should_be_separate_before_and_after_refinement() -> TestResult {
  for source in CASES {
    let graph = read_smiles(source)?.into_graph();
    let before = graph.clone();
    let methyls = methyls(&graph)?;
    assert!(!methyls.is_empty());
    for passes in [0, 4] {
      let options = LayoutOptions {
        refinement_passes: passes,
        ..Default::default()
      };
      let coordinates = layout::generate(&graph, &options)?;
      assert!(
        coordinates.diagnostics().is_empty(),
        "{source}, passes={passes}: {:?}",
        coordinates.diagnostics()
      );
      for &(center, methyl) in &methyls {
        let p = coordinates.position(center).ok_or("missing junction")?;
        let q = coordinates.position(methyl).ok_or("missing methyl")?;
        assert!((p.distance(q) - 1.).abs() < 1e-8);
        for (neighbor, _) in graph.neighbors(center)?.filter(|&(id, _)| id != methyl) {
          let r = coordinates.position(neighbor).ok_or("missing ring neighbor")?;
          let cosine = ((q.x - p.x) * (r.x - p.x) + (q.y - p.y) * (r.y - p.y)) / (p.distance(q) * p.distance(r));
          assert!(
            cosine < 0.9,
            "methyl {methyl} overlaps direction toward {neighbor}: {source}"
          );
        }
      }
      for (_, bond) in graph.bonds() {
        let [a, b] = bond.atoms();
        assert!(
          (coordinates
            .position(a)
            .ok_or("missing atom")?
            .distance(coordinates.position(b).ok_or("missing atom")?)
            - 1.)
            .abs()
            < 1e-8
        );
      }
      assert_eq!(coordinates, layout::generate(&graph, &options)?);
    }
    assert_eq!(graph, before);
  }
  Ok(())
}

#[test]
fn steroid_methyl_stereo_marks_should_render_on_distinct_substituent_bonds() -> TestResult {
  for source in &CASES[1..3] {
    let graph = read_smiles(source)?.into_graph();
    let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
    for mode in [ChiralHydrogenMode::Implicit, ChiralHydrogenMode::Explicit] {
      let drawing = depict::depict(
        &graph,
        &coordinates,
        &DepictOptions {
          chiral_hydrogens: mode,
          ..Default::default()
        },
      )?;
      for (center, methyl) in methyls(&graph)? {
        assert!(graph.tetrahedral(center).is_some());
        let edge = graph.bond_between(center, methyl).ok_or("missing methyl bond")?;
        let shapes: Vec<_> = drawing
          .primitives()
          .iter()
          .filter(|p| match p {
            Primitive::Line(p) => p.source == Some(Source::Bond(edge)),
            Primitive::Polygon(p) => p.source == Some(Source::Bond(edge)),
            _ => false,
          })
          .collect();
        assert!(
          shapes.len() == 7 || (shapes.len() == 1 && matches!(shapes[0], Primitive::Polygon(_))),
          "missing methyl stereo mark: {source}"
        );
        // Independently view the other three ligand directions from the drawn
        // methyl's side of the plane, rather than calling depiction's volume helper.
        let descriptor = graph.tetrahedral(center).ok_or("missing configuration")?;
        let mut order = vec![StereoLigand::Atom(methyl)];
        order.extend(
          descriptor
            .ligands()
            .iter()
            .copied()
            .filter(|&ligand| ligand != StereoLigand::Atom(methyl)),
        );
        let order: [_; 4] = order.try_into().map_err(|_| "invalid fixture ligands")?;
        let expected = descriptor.reordered(order)?.winding();
        let origin = coordinates.position(center).ok_or("missing center")?;
        let others: Vec<_> = order[1..]
          .iter()
          .map(|ligand| match ligand {
            StereoLigand::Atom(id) => {
              let p = coordinates.position(*id).ok_or("missing ligand")?;
              let length = origin.distance(p);
              Ok(((p.x - origin.x) / length, (p.y - origin.y) / length))
            }
            StereoLigand::Hydrogen => Err("expected four explicit ligands"),
          })
          .collect::<Result<_, _>>()?;
        let area = (others[1].0 - others[0].0) * (others[2].1 - others[0].1)
          - (others[1].1 - others[0].1) * (others[2].0 - others[0].0);
        let toward = matches!(shapes[0], Primitive::Polygon(_));
        assert!(area.abs() > 1e-8);
        assert_eq!(
          (area > 0.) == toward,
          expected == TetrahedralWinding::Anticlockwise,
          "drawn methyl polarity changed configuration: {source}"
        );
      }
      let output = svg::export(&drawing, &svg::SvgOptions::default())?;
      assert!(!output.contains("NaN"));
    }
  }
  Ok(())
}

#[test]
fn changed_smiles_traversal_should_not_reintroduce_crowded_methyls() -> TestResult {
  for source in CASES {
    let original = read_smiles(source)?.into_graph();
    let serialized = write_smiles(&original)?;
    let graph = read_smiles(&serialized)?.into_graph();
    let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
    assert!(
      coordinates.diagnostics().is_empty(),
      "{serialized}: {:?}",
      coordinates.diagnostics()
    );
    for (center, methyl) in methyls(&graph)? {
      let p = coordinates.position(methyl).ok_or("missing methyl")?;
      for (id, _) in graph.atoms().filter(|&(id, _)| id != methyl && id != center) {
        assert!(p.distance(coordinates.position(id).ok_or("missing atom")?) >= 0.55);
      }
    }
  }
  Ok(())
}
