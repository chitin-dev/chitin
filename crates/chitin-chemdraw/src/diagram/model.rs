//! Typed instance references and authored reaction-diagram annotations.

use crate::{
  depict,
  geometry::Point,
  scene::{Color, DrawingError, positive},
};
use chitin_chem::{AtomId, BondId};

/// An opaque, process-unique molecule occurrence, not a chemical graph identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoleculeId(pub(super) u64);

/// Positive uniform scale and translation of one molecular drawing.
///
/// Reflections are deliberately excluded: they require new stereobond depiction,
/// not merely transforming an existing scene.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
  /// Diagram coordinate of the molecular drawing's local origin.
  pub origin: Point,
  /// Positive multiplier for geometry, font sizes, and stroke widths.
  pub scale: f64,
}

impl Default for Placement {
  fn default() -> Self {
    Self {
      origin: Point::new(0., 0.),
      scale: 1.,
    }
  }
}

impl Placement {
  pub(super) fn validate(self) -> Result<(), DiagramError> {
    if !self.origin.is_finite() || !positive(self.scale) {
      return Err(DiagramError::Geometry);
    }
    Ok(())
  }

  pub(super) fn point(self, point: Point) -> Point {
    Point::new(
      self.origin.x + point.x * self.scale,
      self.origin.y + point.y * self.scale,
    )
  }
}

/// Chemical object used as an annotation attachment point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
  /// An atom center; electron arrows clear its measured visible labels.
  Atom(AtomId),
  /// The midpoint of the logical bond axis, not one colored half-stroke.
  Bond(BondId),
  /// Explicit radical-dot position attached to an atom, with a local offset.
  /// This is authored notation, not an inference of atomic spin or valence.
  Radical(AtomId),
  /// Midpoint of an intended bond, which need not exist in either source graph.
  BondFormation {
    /// Atom in the anchor's own occurrence.
    atom: AtomId,
    /// Occurrence containing the other reacting atom.
    other_molecule: MoleculeId,
    /// Other reacting atom in its original graph namespace.
    other_atom: AtomId,
  },
}

/// An instance-qualified attachment point with an optional local displacement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Anchor {
  /// Specific occurrence of a molecule in this diagram.
  pub molecule: MoleculeId,
  /// Chemical atom or bond in that occurrence's original graph namespace.
  pub target: Target,
  /// Offset in the molecule's local drawing units; scales with its placement.
  pub offset: Point,
}

impl Anchor {
  /// Attaches an explicitly authored radical dot to an atom-local displacement.
  pub const fn radical(molecule: MoleculeId, atom: AtomId, offset: Point) -> Self {
    Self {
      molecule,
      target: Target::Radical(atom),
      offset,
    }
  }

  /// References a future bond's midpoint; both occurrences are resolved on render.
  pub const fn bond_formation(
    molecule: MoleculeId,
    atom: AtomId,
    other_molecule: MoleculeId,
    other_atom: AtomId,
  ) -> Self {
    Self {
      molecule,
      target: Target::BondFormation {
        atom,
        other_molecule,
        other_atom,
      },
      offset: Point::new(0., 0.),
    }
  }
  /// Attaches to a graph-local atom in one molecule occurrence.
  pub const fn atom(molecule: MoleculeId, atom: AtomId) -> Self {
    Self {
      molecule,
      target: Target::Atom(atom),
      offset: Point::new(0., 0.),
    }
  }

  /// Attaches to the midpoint of a graph-local bond in one occurrence.
  pub const fn bond(molecule: MoleculeId, bond: BondId) -> Self {
    Self {
      molecule,
      target: Target::Bond(bond),
      offset: Point::new(0., 0.),
    }
  }

  /// Sets a displacement, for example the center of an explicitly placed lone pair.
  pub const fn with_offset(mut self, offset: Point) -> Self {
    self.offset = offset;
    self
  }
}

/// Atom disks and rounded bond bands painted beneath every molecule.
///
/// Every selected bond includes its endpoints and their measured atom labels.
/// Overlapping pieces within
/// this request receive opacity once; separate requests intentionally stack.
#[derive(Debug, Clone, PartialEq)]
pub struct Highlight {
  /// Molecule occurrence owning the selected handles.
  pub molecule: MoleculeId,
  /// Selected atom centers; duplicates do not darken the result.
  pub atoms: Vec<AtomId>,
  /// Selected logical bonds, including all their chemical stroke variants.
  pub bonds: Vec<BondId>,
  /// Band half-width in local drawing units, scaled with the molecule.
  pub radius: f64,
  /// Shared highlight color and opacity.
  pub color: Color,
}

/// Positive arrow dimensions in diagram units, independent of molecule scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArrowStyle {
  /// Shaft and arrowhead color.
  pub color: Color,
  /// Shaft width.
  pub width: f64,
  /// Arrowhead length along the final tangent.
  pub head_length: f64,
  /// Full width of a two-electron arrowhead.
  pub head_width: f64,
  /// Extra separation from atom labels and logical bond axes.
  pub clearance: f64,
}

impl Default for ArrowStyle {
  fn default() -> Self {
    Self {
      color: Color::BLACK,
      width: 1.6,
      head_length: 8.,
      head_width: 6.,
      clearance: 3.,
    }
  }
}

/// Explicit electron-flow head; the renderer does not infer electron counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElectronHead {
  /// A full triangular head indicating movement of an electron pair.
  Pair,
  /// One side of the head, selected relative to the final tangent in y-down axes.
  FishhookLeft,
  /// The opposite side of a single-electron head.
  FishhookRight,
}

/// Curve policy for electron-flow notation, independent of electron count.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub enum ElectronCurve {
  /// Existing bend or endpoint-local control-point geometry.
  #[default]
  Bezier,
  /// An exact circular arc with a signed central sweep in radians.
  /// Absolute sweep must be between one and 180 degrees, inclusive.
  /// Positive is clockwise in y-down coordinates; `bend` is ignored.
  Arc {
    /// Signed central angle; determines curvature independently of chord length.
    sweep: f64,
  },
}

/// Authored electron movement between instance-qualified chemical objects.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ElectronArrow {
  /// Curve geometry; exact arcs cannot be combined with Bezier controls.
  pub curve: ElectronCurve,
  /// Optional departure/arrival control offsets relative to `from`/`to`.
  /// Each offset uses its endpoint molecule's local drawing units and scale.
  /// Both controls follow their endpoint on movement. When present, `bend` is
  /// ignored and automatic routing evaluates this exact authored curve only.
  pub controls: Option<[Point; 2]>,
  /// Source atom, bond, displaced lone pair, or explicitly authored radical.
  pub from: Anchor,
  /// Destination atom, bond or intended formation site, possibly intermolecular.
  pub to: Anchor,
  /// Signed perpendicular control-point displacement in diagram units.
  /// Positive bends clockwise from the source-to-destination axis in y-down axes.
  pub bend: f64,
  /// Explicit one- or two-electron notation.
  pub head: ElectronHead,
  /// Paint and sizing policy.
  pub style: ArrowStyle,
}

/// Directional notation for a reaction step, distinct from electron movement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReactionArrowKind {
  /// One full arrow pointing from start to end.
  Forward,
  /// Oppositely directed half-headed parallel arrows.
  Equilibrium,
}

/// A reaction arrow at authored diagram coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReactionArrow {
  /// Beginning of the reaction step.
  pub start: Point,
  /// Destination of the reaction step.
  pub end: Point,
  /// Forward or equilibrium notation.
  pub kind: ReactionArrowKind,
  /// Paint and arrowhead dimensions; clearance does not alter these free endpoints.
  pub style: ArrowStyle,
}

/// Two explicitly authored electron dots; no valence inference is performed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LonePair {
  /// Center of the pair, usually an atom anchor with an outward offset.
  pub anchor: Anchor,
  /// Direction of the line joining the dots, in radians in y-down axes.
  pub angle: f64,
  /// Positive center-to-center separation in local molecule drawing units.
  pub separation: f64,
  /// Positive dot radius in local molecule drawing units.
  pub radius: f64,
  /// Dot color and opacity.
  pub color: Color,
}

/// One explicitly authored electron dot; no radical chemistry is inferred.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ElectronDot {
  /// Atom-local radical anchor, retained when the occurrence is moved.
  pub anchor: Anchor,
  /// Positive dot radius in local drawing units.
  pub radius: f64,
  /// Dot paint and opacity.
  pub color: Color,
}

/// Authored diagram annotations, separate from molecular facts and layout.
#[derive(Debug, Clone, PartialEq)]
pub enum Annotation {
  /// Underlying fragment paint.
  Highlight(Highlight),
  /// Curved electron-flow arrow.
  ElectronArrow(ElectronArrow),
  /// Straight reaction-step notation.
  ReactionArrow(ReactionArrow),
  /// Explicit electron pair.
  LonePair(LonePair),
  /// A single electron at an explicitly supplied radical position.
  ElectronDot(ElectronDot),
}

/// Typed failures at diagram construction or annotation resolution boundaries.
#[derive(Debug, thiserror::Error)]
pub enum DiagramError {
  /// Molecular depiction could not faithfully represent the supplied input.
  #[error(transparent)]
  Depict(#[from] depict::DepictError),
  /// Primitive geometry, dimensions, or measured text failed validation.
  #[error(transparent)]
  Drawing(#[from] DrawingError),
  /// A molecule handle belongs to another diagram or has never been inserted.
  #[error("molecule occurrence does not belong to this diagram")]
  MissingMolecule,
  /// A selected atom is absent from this molecule's coordinate namespace.
  #[error("missing atom {0} in molecule occurrence")]
  MissingAtom(AtomId),
  /// A selected bond is absent from this molecule's graph namespace.
  #[error("missing bond {0} in molecule occurrence")]
  MissingBond(BondId),
  /// Nonfinite coordinates, invalid sizes, or a degenerate arrow were supplied.
  #[error("invalid diagram geometry or annotation dimensions")]
  Geometry,
  /// No more unique occurrence identifiers are available.
  #[error("molecule occurrence identifier space exhausted")]
  IdentifierExhausted,
}
