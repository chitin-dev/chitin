//! Authored reaction diagrams layered above immutable molecular depiction.
//!
//! Instances retain graph-local atom/bond handles while their placements are
//! independent. Annotations reference those instances, so moving a molecule does
//! not detach its highlights or electron arrows. Distances use y-down drawing
//! units, not normalized molecular coordinates. This module draws supplied
//! mechanisms; it does not infer electron counts or reaction chemistry.
//!
//! # Examples
//! ```
//! use chitin_chem::io::smiles::read_smiles;
//! use chitin_chemdraw::{Color, depict::DepictOptions, layout, svg};
//! use chitin_chemdraw::diagram::{Diagram, Placement, Annotation, Highlight};
//! let graph = read_smiles("COc1ccccc1")?.into_graph();
//! let coordinates = layout::generate(&graph, &layout::LayoutOptions::default())?;
//! let mut diagram = Diagram::new();
//! let molecule = diagram.add_molecule(
//!   &graph, &coordinates, &DepictOptions::default(), Placement::default(),
//! )?;
//! diagram.annotate(Annotation::Highlight(Highlight {
//!   molecule, atoms: vec![], bonds: graph.bonds().map(|(id, _)| id).collect(),
//!   radius: 7., color: Color { red: 130, green: 200, blue: 170, alpha: 100 },
//! }));
//! let drawing = diagram.render()?;
//! let svg = svg::export(&drawing, &svg::SvgOptions::default())?;
//! assert!(svg.contains("<svg"));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

mod arrows;
mod lone_pair;
mod model;
mod routing;
mod transform;

pub use lone_pair::{LonePairOptions, LonePairSuggestion};
pub use model::{
  Anchor, Annotation, ArrowStyle, DiagramError, ElectronArrow, ElectronDot, ElectronHead, Highlight, LonePair,
  MoleculeId, Placement, ReactionArrow, ReactionArrowKind, Target,
};
pub use routing::{RoutingOptions, RoutingReport};

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};

use chitin_chem::{AtomId, BondId, MolecularGraph};

use crate::{
  depict::{self, DepictOptions},
  geometry::Point,
  layout::Layout2d,
  scene::{Color, Disk, Drawing, DrawingError, Group, Line, LineCap, Primitive, Source, positive},
};

struct Molecule {
  id: MoleculeId,
  placement: Placement,
  drawing: Drawing,
  atoms: BTreeMap<AtomId, Point>,
  bonds: BTreeMap<BondId, [AtomId; 2]>,
}

/// Frontend-independent composition of molecular snapshots and annotations.
///
/// No graph is owned or mutated. Molecules are depicted once at insertion;
/// changing placement only transforms the snapshot. Identifier allocation is
/// process-unique, so handles cannot alias across independently created diagrams.
#[derive(Default)]
pub struct Diagram {
  molecules: Vec<Molecule>,
  annotations: Vec<Annotation>,
}

impl Diagram {
  /// Creates an empty diagram.
  pub fn new() -> Self {
    Self::default()
  }

  /// Depicts and snapshots a molecule, returning its unique occurrence handle.
  ///
  /// # Parameters
  /// - `graph`, `layout`, `options`: the ordinary molecular depiction inputs.
  /// - `placement`: positive scale and translation in diagram drawing units.
  ///
  /// # Returns
  /// A new handle even when the same graph is inserted more than once.
  ///
  /// # Errors
  /// Rejects invalid placements, depiction failures, overflowing transformed
  /// geometry, or exhausted identifiers. Failure does not insert a partial item.
  pub fn add_molecule(
    &mut self,
    graph: &MolecularGraph,
    layout: &Layout2d,
    options: &DepictOptions,
    placement: Placement,
  ) -> Result<MoleculeId, DiagramError> {
    static NEXT_ID: AtomicU64 = AtomicU64::new(0);
    placement.validate()?;
    let drawing = depict::depict(graph, layout, options)?;
    transform::drawing(&drawing, placement)?;
    let atoms = layout
      .positions()
      .iter()
      .map(|(&id, p)| (id, Point::new(p.x * options.bond_length, -p.y * options.bond_length)))
      .collect();
    let bonds = graph.bonds().map(|(id, b)| (id, b.atoms())).collect();
    let id = MoleculeId(
      NEXT_ID
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
        .map_err(|_| DiagramError::IdentifierExhausted)?,
    );
    self.molecules.push(Molecule {
      id,
      placement,
      drawing,
      atoms,
      bonds,
    });
    Ok(id)
  }

  /// Changes one occurrence's placement without changing its anchors or chemistry.
  ///
  /// # Errors
  /// Rejects foreign handles or invalid/overflowing transformed geometry; the old
  /// placement remains intact when validation fails.
  pub fn set_placement(&mut self, id: MoleculeId, placement: Placement) -> Result<(), DiagramError> {
    placement.validate()?;
    let item = self
      .molecules
      .iter_mut()
      .find(|m| m.id == id)
      .ok_or(DiagramError::MissingMolecule)?;
    transform::drawing(&item.drawing, placement)?;
    item.placement = placement;
    Ok(())
  }

  /// Appends an authored annotation; references are validated when rendering.
  pub fn annotate(&mut self, annotation: Annotation) -> &mut Self {
    self.annotations.push(annotation);
    self
  }

  /// Redraws an edited molecule while retaining its occurrence and annotations.
  ///
  /// Supply the original graph (or a provenance-preserving clone) with new
  /// coordinates/facts. Surviving atom/bond handles retain their attachments.
  /// Deleted handles cause an explicit error at render, never index rebinding.
  /// The current occurrence placement is unchanged.
  ///
  /// # Errors
  /// Rejects foreign occurrences, invalid chemistry, coordinates or transformed
  /// geometry. Failure leaves the previous molecular snapshot intact.
  pub fn redraw_molecule(
    &mut self,
    id: MoleculeId,
    graph: &MolecularGraph,
    layout: &Layout2d,
    options: &DepictOptions,
  ) -> Result<(), DiagramError> {
    let item = self
      .molecules
      .iter_mut()
      .find(|m| m.id == id)
      .ok_or(DiagramError::MissingMolecule)?;
    let drawing = depict::depict(graph, layout, options)?;
    transform::drawing(&drawing, item.placement)?;
    let atoms = layout
      .positions()
      .iter()
      .map(|(&atom, p)| (atom, Point::new(p.x * options.bond_length, -p.y * options.bond_length)))
      .collect();
    let bonds = graph.bonds().map(|(bond, b)| (bond, b.atoms())).collect();
    item.drawing = drawing;
    item.atoms = atoms;
    item.bonds = bonds;
    Ok(())
  }

  /// Resolves an attachment point in the current diagram placement.
  ///
  /// # Errors
  /// Rejects foreign occurrence/chemical handles and nonfinite offsets or results.
  pub fn resolve(&self, anchor: Anchor) -> Result<Point, DiagramError> {
    let molecule = self.molecule(anchor.molecule)?;
    let point = match anchor.target {
      Target::Atom(id) | Target::Radical(id) => molecule.atom(id)?,
      Target::BondFormation {
        atom,
        other_molecule,
        other_atom,
      } => {
        let a = molecule.placement.point(molecule.atom(atom)?);
        let other = self.molecule(other_molecule)?;
        let b = other.placement.point(other.atom(other_atom)?);
        let point = Point::new(a.x / 2. + b.x / 2., a.y / 2. + b.y / 2.).translated(
          anchor.offset.x * molecule.placement.scale,
          anchor.offset.y * molecule.placement.scale,
        );
        return if point.is_finite() {
          Ok(point)
        } else {
          Err(DiagramError::Geometry)
        };
      }
      Target::Bond(id) => {
        let [a, b] = molecule.bond(id)?;
        Point::new(a.x / 2. + b.x / 2., a.y / 2. + b.y / 2.)
      }
    };
    let point = molecule
      .placement
      .point(point.translated(anchor.offset.x, anchor.offset.y));
    if !point.is_finite() {
      return Err(DiagramError::Geometry);
    }
    Ok(point)
  }

  /// Builds a validated scene: highlights, molecule instances, then annotations.
  ///
  /// # Returns
  /// Exporter-independent commands. Each molecule is a separate child group in
  /// insertion order, retaining the original graph-local primitive sources.
  ///
  /// # Errors
  /// Rejects missing handles, invalid annotation dimensions, degenerate arrows,
  /// or overflowing geometry without returning a partially rendered diagram.
  pub fn render(&self) -> Result<Drawing, DiagramError> {
    let mut primitives = Vec::new();
    for annotation in &self.annotations {
      if let Annotation::Highlight(highlight) = annotation {
        primitives.push(self.highlight(highlight)?);
      }
    }
    for molecule in &self.molecules {
      primitives.push(Primitive::Group(Group {
        drawing: transform::drawing(&molecule.drawing, molecule.placement)?,
        opacity: 255,
      }));
    }
    for annotation in &self.annotations {
      match annotation {
        Annotation::Highlight(_) => {}
        Annotation::ElectronArrow(arrow) => arrows::electron(self, arrow, &mut primitives)?,
        Annotation::ElectronDot(dot) => self.electron_dot(dot, &mut primitives)?,
        Annotation::ReactionArrow(arrow) => arrows::reaction(arrow, &mut primitives)?,
        Annotation::LonePair(pair) => self.lone_pair(pair, &mut primitives)?,
      }
    }
    Ok(Drawing::new(primitives)?)
  }

  fn molecule(&self, id: MoleculeId) -> Result<&Molecule, DiagramError> {
    self
      .molecules
      .iter()
      .find(|m| m.id == id)
      .ok_or(DiagramError::MissingMolecule)
  }

  fn highlight(&self, highlight: &Highlight) -> Result<Primitive, DiagramError> {
    let m = self.molecule(highlight.molecule)?;
    if !positive(highlight.radius) || (highlight.atoms.is_empty() && highlight.bonds.is_empty()) {
      return Err(DiagramError::Geometry);
    }
    let radius = highlight.radius * m.placement.scale;
    let color = Color {
      alpha: 255,
      ..highlight.color
    };
    let mut pieces = Vec::new();
    let mut atoms: BTreeSet<_> = highlight.atoms.iter().copied().collect();
    for &bond in &highlight.bonds {
      atoms.extend(*m.bonds.get(&bond).ok_or(DiagramError::MissingBond(bond))?);
      let [a, b] = m.bond(bond)?.map(|p| m.placement.point(p));
      pieces.push(Primitive::Line(Line {
        start: a,
        end: b,
        width: 2. * radius,
        color,
        caps: [LineCap::Round; 2],
        source: Some(Source::Bond(bond)),
      }));
    }
    for atom in atoms {
      pieces.push(Primitive::Disk(Disk {
        center: m.placement.point(m.atom(atom)?),
        radius,
        color,
        source: Some(Source::Atom(atom)),
      }));
      for primitive in m.drawing.leaf_primitives() {
        let bounds = match primitive {
          Primitive::Text(text) if text.source == Some(Source::Atom(atom)) => {
            text.envelope().map_err(DrawingError::from)?
          }
          Primitive::Circle(circle) if circle.source == Some(Source::Atom(atom)) => {
            let r = circle.radius + circle.width / 2.;
            [circle.center.translated(-r, -r), circle.center.translated(r, r)]
          }
          _ => continue,
        };
        let [min, max] = bounds.map(|p| m.placement.point(p));
        let half_height = ((max.y - min.y) / 2. + 2. * m.placement.scale).max(radius);
        let y = min.y / 2. + max.y / 2.;
        pieces.push(Primitive::Line(Line {
          start: Point::new(min.x, y),
          end: Point::new(max.x, y),
          width: 2. * half_height,
          color,
          caps: [LineCap::Round; 2],
          source: Some(Source::Atom(atom)),
        }));
      }
    }
    Ok(Primitive::Group(Group {
      drawing: Drawing::new(pieces)?,
      opacity: highlight.color.alpha,
    }))
  }

  fn electron_dot(&self, dot: &ElectronDot, out: &mut Vec<Primitive>) -> Result<(), DiagramError> {
    if !positive(dot.radius) || !matches!(dot.anchor.target, Target::Radical(_)) {
      return Err(DiagramError::Geometry);
    }
    out.push(Primitive::Disk(Disk {
      center: self.resolve(dot.anchor)?,
      radius: dot.radius * self.molecule(dot.anchor.molecule)?.placement.scale,
      color: dot.color,
      source: None,
    }));
    Ok(())
  }

  fn lone_pair(&self, pair: &LonePair, out: &mut Vec<Primitive>) -> Result<(), DiagramError> {
    if !pair.angle.is_finite() || !positive(pair.separation) || !positive(pair.radius) {
      return Err(DiagramError::Geometry);
    }
    let center = self.resolve(pair.anchor)?;
    let scale = self.molecule(pair.anchor.molecule)?.placement.scale;
    let offset = pair.separation * scale / 2.;
    for sign in [-1., 1.] {
      out.push(Primitive::Disk(Disk {
        center: center.translated(sign * offset * pair.angle.cos(), sign * offset * pair.angle.sin()),
        radius: pair.radius * scale,
        color: pair.color,
        source: None,
      }));
    }
    Ok(())
  }
}

impl Molecule {
  fn atom(&self, id: AtomId) -> Result<Point, DiagramError> {
    self.atoms.get(&id).copied().ok_or(DiagramError::MissingAtom(id))
  }

  fn bond(&self, id: BondId) -> Result<[Point; 2], DiagramError> {
    let &[a, b] = self.bonds.get(&id).ok_or(DiagramError::MissingBond(id))?;
    Ok([self.atom(a)?, self.atom(b)?])
  }
}
