//! Composes highlights and an authored substitution mechanism into one SVG.

use std::collections::BTreeMap;

use chitin_chem::io::smiles::read_smiles;
use chitin_chemdraw::{
  Color,
  depict::DepictOptions,
  diagram::{
    Anchor, Annotation, ArrowStyle, Diagram, ElectronArrow, ElectronHead, Highlight, LonePair, Placement,
    ReactionArrow, ReactionArrowKind,
  },
  geometry::Point,
  layout::{self, Layout2d, LayoutOptions},
  svg,
};

/// Writes the requested diagram; atom indices below belong only to this example.
///
/// # Errors
/// Propagates parsing, molecular depiction, annotation validation and file errors.
fn main() -> Result<(), Box<dyn std::error::Error>> {
  let path = std::env::args().nth(1).ok_or("usage: reaction-diagram <output.svg>")?;
  let options = DepictOptions::default();
  let mut diagram = Diagram::new();

  let fragment = read_smiles("c1ccccc1OCCN")?.into_graph();
  let layout = layout::generate(&fragment, &LayoutOptions::default())?;
  let instance = diagram.add_molecule(
    &fragment,
    &layout,
    &options,
    Placement {
      origin: Point::new(50., -210.),
      scale: 1.,
    },
  )?;
  let bonds: Vec<_> = fragment.bonds().map(|(id, _)| id).collect();
  for (selected, color) in [
    (
      bonds[..6].to_vec(),
      Color {
        red: 100,
        green: 180,
        blue: 145,
        alpha: 100,
      },
    ),
    (
      bonds[6..].to_vec(),
      Color {
        red: 240,
        green: 165,
        blue: 60,
        alpha: 110,
      },
    ),
  ] {
    diagram.annotate(Annotation::Highlight(Highlight {
      molecule: instance,
      atoms: vec![],
      bonds: selected,
      radius: 7.,
      color,
    }));
  }

  let hydroxide = read_smiles("[OH-]")?.into_graph();
  let oxygen = hydroxide.atoms().next().ok_or("missing oxygen")?.0;
  let layout = Layout2d::from_positions(BTreeMap::from([(oxygen, Point::new(0., 0.))]))?;
  let hydroxide_id = diagram.add_molecule(
    &hydroxide,
    &layout,
    &options,
    Placement {
      origin: Point::new(40., 10.),
      scale: 1.,
    },
  )?;
  let pair_anchor = Anchor::atom(hydroxide_id, oxygen).with_offset(Point::new(16., 14.));
  diagram.annotate(Annotation::LonePair(LonePair {
    anchor: pair_anchor,
    angle: 0.,
    separation: 5.,
    radius: 1.5,
    color: Color {
      red: 240,
      green: 30,
      blue: 30,
      alpha: 255,
    },
  }));

  let substrate = read_smiles("CC(C)I")?.into_graph();
  let atoms: Vec<_> = substrate.atoms().map(|(id, _)| id).collect();
  let positions = [
    Point::new(-0.866, 0.5),
    Point::new(0., 0.),
    Point::new(0.866, 0.5),
    Point::new(0., -1.),
  ];
  let layout = Layout2d::from_positions(atoms.iter().copied().zip(positions).collect())?;
  let substrate_id = diagram.add_molecule(
    &substrate,
    &layout,
    &options,
    Placement {
      origin: Point::new(200., 60.),
      scale: 1.,
    },
  )?;
  diagram.annotate(Annotation::ElectronArrow(ElectronArrow {
    from: pair_anchor,
    to: Anchor::atom(substrate_id, atoms[1]),
    bend: 45.,
    head: ElectronHead::Pair,
    style: ArrowStyle::default(),
  }));
  let leaving_bond = substrate
    .bond_between(atoms[1], atoms[3])
    .ok_or("missing leaving-group bond")?;
  diagram.annotate(Annotation::ElectronArrow(ElectronArrow {
    from: Anchor::bond(substrate_id, leaving_bond),
    to: Anchor::atom(substrate_id, atoms[3]),
    bend: -42.,
    head: ElectronHead::Pair,
    style: ArrowStyle::default(),
  }));
  diagram.annotate(Annotation::ReactionArrow(ReactionArrow {
    start: Point::new(290., 65.),
    end: Point::new(370., 65.),
    kind: ReactionArrowKind::Forward,
    style: ArrowStyle::default(),
  }));

  let product = read_smiles("CC(C)O.[I-]")?.into_graph();
  let layout = layout::generate(&product, &LayoutOptions::default())?;
  diagram.add_molecule(
    &product,
    &layout,
    &options,
    Placement {
      origin: Point::new(450., 65.),
      scale: 1.,
    },
  )?;
  let drawing = diagram.render()?;
  std::fs::write(path, svg::export(&drawing, &svg::SvgOptions::default())?)?;
  Ok(())
}
