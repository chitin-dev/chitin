//! Versioned authored-diagram input; graph handles never cross the file boundary.
//!
//! File coordinates and source atom indices are converted here, while chemical
//! interpretation and primitive generation remain in the drawing crate. Layout
//! uses normalized y-up bond units; instance placement and arrow geometry use
//! y-down SVG units. Each occurrence owns its own index-to-handle lookup.

use super::{Error, cancelled};
use crate::CommandExecutionContext;
use chitin_chem::{AtomId, MolecularGraph};
use chitin_chemdraw::{
  Drawing,
  depict::DepictOptions,
  diagram as d,
  geometry::Point,
  layout::{self, Layout2d},
  pipeline,
};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

/// Strict document envelope; omitted annotations deliberately produce molecules only.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
  /// Required schema discriminator; only version 1 is accepted.
  version: u32,
  /// Named occurrences in painter order, limited to 128 per document.
  molecules: Vec<Molecule>,
  /// Authored highlights, electron dots and arrows, limited to 4096 entries.
  #[serde(default)]
  annotations: Vec<Annotation>,
}

/// Source molecule and an independent positive-similarity placement.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Molecule {
  /// Unique occurrence name, not a chemical graph identity.
  id: String,
  /// Original notation; source atom indices follow its parse order.
  smiles: String,
  /// Local drawing origin in shared y-down SVG units, not an image corner.
  at: [f64; 2],
  /// Positive multiplier for molecular geometry, labels and local offsets.
  #[serde(default = "one")]
  scale: f64,
  /// Complete normalized y-up atom positions, or absent for automatic layout.
  #[serde(default)]
  positions: Option<Vec<[f64; 2]>>,
}
/// Default identity scale; reflections are not a valid placement shorthand.
fn one() -> f64 {
  1.
}
/// Default highlight band half-width in local SVG units.
fn radius() -> f64 {
  6.
}
/// Default filled electron-dot radius in local SVG units.
fn dot_radius() -> f64 {
  1.
}
/// Default center-to-center electron-dot separation in local SVG units.
fn separation() -> f64 {
  4.
}
/// Default translucent fragment paint, encoded as straight-alpha RGBA bytes.
fn color() -> [u8; 4] {
  [120, 190, 150, 100]
}
/// Default opaque annotation foreground, independent of element coloring.
fn black() -> [u8; 4] {
  [0, 0, 0, 255]
}

/// File-local attachment resolved against one explicitly named occurrence.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reference {
  /// Occurrence owning the selected atom or bond.
  molecule: String,
  /// Original source atom index or logical bond endpoint pair.
  target: Target,
  /// Local SVG displacement, scaled together with the selected molecule.
  #[serde(default)]
  offset: [f64; 2],
}

/// Chemical attachment kind; a bond denotes its logical axis, not a stroke half.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
enum Target {
  /// An authored radical dot at an atom-local offset, without spin inference.
  Radical {
    /// Source atom index.
    index: usize,
  },
  /// Intended bond midpoint; no source bond is required.
  BondFormation {
    /// Atom index in this occurrence.
    index: usize,
    /// Occurrence containing the second reacting atom.
    other_molecule: String,
    /// Atom index in the other occurrence.
    other_index: usize,
  },
  /// Source atom center, with label clearance applied by the arrow renderer.
  Atom {
    /// Zero-based atom index in the selected occurrence's SMILES parse order.
    index: usize,
  },
  /// Midpoint of an existing bond connecting two source atoms.
  Bond {
    /// Endpoint indices; either order identifies the same chemical bond.
    atoms: [usize; 2],
  },
}

/// Explicit electron-count notation; the renderer never infers a mechanism.
#[derive(Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Head {
  /// Full head for movement of an electron pair.
  #[default]
  Pair,
  /// Single-electron barb left of the arrival tangent in y-down drawing axes.
  FishhookLeft,
  /// Single-electron barb on the opposite side of the arrival tangent.
  FishhookRight,
}

/// Arrow paint and dimensions in shared SVG units, not molecule-local units.
#[derive(Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
struct Paint {
  /// Straight-alpha RGBA channels, each in 0..=255.
  color: [u8; 4],
  /// Positive full shaft width.
  width: f64,
  /// Positive arrowhead length along the arrival tangent.
  head_length: f64,
  /// Positive full width of a two-electron head.
  head_width: f64,
  /// Nonnegative extra clearance from visible atom labels and bond axes.
  clearance: f64,
}
impl Default for Paint {
  /// Inherits the drawing engine's arrow proportions with an opaque foreground.
  fn default() -> Self {
    let s = d::ArrowStyle::default();
    Self {
      color: black(),
      width: s.width,
      head_length: s.head_length,
      head_width: s.head_width,
      clearance: s.clearance,
    }
  }
}
impl Paint {
  /// Converts wire paint without clamping; final rendering validates dimensions.
  fn style(&self) -> d::ArrowStyle {
    d::ArrowStyle {
      color: rgba(self.color),
      width: self.width,
      head_length: self.head_length,
      head_width: self.head_width,
      clearance: self.clearance,
    }
  }
}

/// Authored visual annotations, never mutations of molecular connectivity.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
enum Annotation {
  /// One authored electron dot, independent of chemical spin inference.
  ElectronDot {
    /// Radical target with an atom-local offset.
    anchor: Reference,
    /// Positive local dot radius.
    #[serde(default = "dot_radius")]
    radius: f64,
    /// Dot paint.
    #[serde(default = "black")]
    color: [u8; 4],
  },
  /// One compositing group beneath the molecules; overlap receives opacity once.
  Highlight {
    /// Occurrence whose atom and bond indices are selected.
    molecule: String,
    /// Source atom indices; selected bond endpoints are also shaded.
    #[serde(default)]
    atoms: Vec<usize>,
    /// Existing bonds addressed by their two source atom indices.
    #[serde(default)]
    bonds: Vec<[usize; 2]>,
    /// Positive band half-width before the molecule's placement scale.
    #[serde(default = "radius")]
    radius: f64,
    /// Shared straight-alpha RGBA paint for this entire highlight request.
    #[serde(default = "color")]
    color: [u8; 4],
  },
  /// An explicitly bent electron-flow curve, possibly between two occurrences.
  ElectronArrow {
    /// Endpoint-relative departure and arrival control offsets; overrides bend.
    #[serde(default)]
    controls: Option<[[f64; 2]; 2]>,
    /// Source atom, bond or displaced lone-pair center.
    from: Reference,
    /// Destination atom or bond in any declared occurrence.
    to: Reference,
    /// Signed perpendicular control-point displacement in shared SVG units.
    #[serde(default)]
    bend: f64,
    /// Full or single-electron head; independent of chemical bond order.
    #[serde(default)]
    head: Head,
    /// Shaft, head and attachment-clearance settings.
    #[serde(default)]
    style: Paint,
  },
  /// A reaction-step connector, distinct from electron movement.
  ReactionArrow {
    /// Authored start point in shared y-down SVG units.
    start: [f64; 2],
    /// Authored end point in the same coordinate frame.
    end: [f64; 2],
    /// Draw opposing half-headed arrows instead of one forward arrow.
    #[serde(default)]
    equilibrium: bool,
    /// Paint and sizing; free endpoints do not use chemical-label clearance.
    #[serde(default)]
    style: Paint,
  },
  /// Two authored electron dots; lone pairs are not inferred from valence.
  LonePair {
    /// Pair center, normally an atom reference with an outward local offset.
    anchor: Reference,
    /// Direction joining the dots, in radians in y-down axes.
    #[serde(default)]
    angle: f64,
    /// Positive dot-center separation before instance scaling.
    #[serde(default = "separation")]
    separation: f64,
    /// Positive filled dot radius before instance scaling.
    #[serde(default = "dot_radius")]
    radius: f64,
    /// Straight-alpha RGBA paint for both dots.
    #[serde(default = "black")]
    color: [u8; 4],
  },
}

/// Per-occurrence ownership boundary between file indices and opaque graph handles.
struct Entry {
  /// Unique occurrence allocated by the current drawing composition.
  id: d::MoleculeId,
  /// Original graph retained for validating bond existence in this namespace.
  graph: MolecularGraph,
  /// Parse-order handles; numeric file indices are never treated as raw handles.
  atoms: Vec<AtomId>,
}
impl Entry {
  /// Resolves an in-range source atom index, rejecting absent indices.
  fn atom(&self, index: usize) -> Result<AtomId, Error> {
    self
      .atoms
      .get(index)
      .copied()
      .ok_or_else(|| Error::Input(format!("unknown atom index {index}")))
  }
  /// Resolves two local atom indices and rejects pairs without a chemical bond.
  fn bond(&self, [a, b]: [usize; 2]) -> Result<chitin_chem::BondId, Error> {
    self
      .graph
      .bond_between(self.atom(a)?, self.atom(b)?)
      .ok_or_else(|| Error::Input(format!("no bond between atom indices {a} and {b}")))
  }
}
/// Preserves all four validated byte channels without color-space conversion.
fn rgba([red, green, blue, alpha]: [u8; 4]) -> chitin_chemdraw::Color {
  chitin_chemdraw::Color {
    red,
    green,
    blue,
    alpha,
  }
}
/// Looks up one declared occurrence without falling back to a different graph.
fn entry<'a>(id: &str, entries: &'a BTreeMap<String, Entry>) -> Result<&'a Entry, Error> {
  entries
    .get(id)
    .ok_or_else(|| Error::Input(format!("unknown molecule occurrence {id:?}")))
}
/// Binds a file reference and its local offset to the selected occurrence.
///
/// # Errors
/// Rejects unknown occurrence names, absent atom indices, and nonexistent bonds.
/// Offset finiteness is checked when the drawing engine resolves the anchor.
fn reference(input: &Reference, entries: &BTreeMap<String, Entry>) -> Result<d::Anchor, Error> {
  let e = entry(&input.molecule, entries)?;
  let anchor = match &input.target {
    Target::Atom { index } => d::Anchor::atom(e.id, e.atom(*index)?),
    Target::Bond { atoms } => d::Anchor::bond(e.id, e.bond(*atoms)?),
    Target::Radical { index } => d::Anchor::radical(e.id, e.atom(*index)?, Point::new(0., 0.)),
    Target::BondFormation {
      index,
      other_molecule,
      other_index,
    } => {
      let other = entry(other_molecule, entries)?;
      d::Anchor::bond_formation(e.id, e.atom(*index)?, other.id, other.atom(*other_index)?)
    }
  };
  Ok(anchor.with_offset(Point::new(input.offset[0], input.offset[1])))
}

/// Decodes and composes one complete authored diagram without filesystem writes.
///
/// # Parameters
/// - `bytes`: size-bounded JSON/YAML supplied by the command's input reader.
/// - `options`: shared molecular paint and font policy; not page placement.
/// - `context`: cancellation observed between molecule and annotation stages.
///
/// # Returns
/// A validated drawing, occurrence count, and recoverable local layout/contact
/// diagnostics. Authored atom positions bypass automatic layout and refinement;
/// annotations use fresh graph-local lookups belonging to this document only.
///
/// # Errors
/// Rejects unknown fields/versions, invalid names or placements, excessive input
/// counts, unsupported chemistry, incomplete coordinates, invalid references,
/// cancellation, and final-scene geometry errors. No partial drawing is returned.
pub(super) fn render(
  bytes: &[u8],
  options: &DepictOptions,
  context: &CommandExecutionContext,
) -> Result<(Drawing, usize, Vec<String>), Error> {
  let input: Document = serde_yaml::from_slice(bytes)?;
  if input.version != 1 {
    return Err(Error::Input(format!(
      "unsupported reaction diagram version {}",
      input.version
    )));
  }
  if input.molecules.is_empty() || input.molecules.len() > 128 || input.annotations.len() > 4096 {
    return Err(Error::Input(
      "diagram requires 1..=128 molecules and at most 4096 annotations".into(),
    ));
  }
  let mut names = BTreeSet::new();
  // Validate document-wide names before allocating occurrence handles, so a
  // duplicate cannot silently replace an earlier graph in the lookup table.
  for item in &input.molecules {
    if item.id.trim().is_empty()
      || item.id.chars().any(char::is_control)
      || item.id.len() > 128
      || !names.insert(&item.id)
    {
      return Err(Error::Input(
        "molecule IDs must be unique nonempty names of at most 128 bytes".into(),
      ));
    }
    if item.smiles.len() > 64 * 1024 {
      return Err(Error::Input("SMILES exceeds 64 KiB".into()));
    }
    if !item.scale.is_finite() || item.scale <= 0. || !item.at.into_iter().all(f64::is_finite) {
      return Err(Error::Input("placement must be finite with positive scale".into()));
    }
  }
  let count = input.molecules.len();
  let mut diagram = d::Diagram::new();
  let mut entries = BTreeMap::new();
  let mut total = 0;
  let mut diagnostics = Vec::new();
  for item in input.molecules {
    cancelled(context)?;
    let graph = chitin_chem::io::smiles::read_smiles(&item.smiles)?.into_graph();
    total += graph.atom_count();
    if total > 8192 {
      return Err(Error::Input("diagram exceeds 8192 atoms".into()));
    }
    let atoms: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
    // Supplied coordinates are author intent, not a seed for automatic repair.
    let coordinates = if let Some(positions) = item.positions {
      if positions.len() != atoms.len() {
        return Err(Error::Input(
          "authored positions must cover every atom in parse order".into(),
        ));
      }
      Layout2d::from_positions(
        atoms
          .iter()
          .copied()
          .zip(positions.into_iter().map(|[x, y]| Point::new(x, y)))
          .collect(),
      )
      .map_err(|e| Error::Input(e.to_string()))?
    } else {
      let rendered = pipeline::generate(&graph, &layout::LayoutOptions::default(), options)?;
      diagnostics.extend(rendered.contacts.iter().map(|d| format!("{}: {d:?}", item.id)));
      rendered.layout
    };
    diagnostics.extend(coordinates.diagnostics().iter().map(|d| format!("{}: {d:?}", item.id)));
    let id = diagram.add_molecule(
      &graph,
      &coordinates,
      options,
      d::Placement {
        origin: Point::new(item.at[0], item.at[1]),
        scale: item.scale,
      },
    )?;
    entries.insert(item.id, Entry { id, graph, atoms });
  }
  for annotation in input.annotations {
    cancelled(context)?;
    diagram.annotate(match annotation {
      Annotation::ElectronDot { anchor, radius, color } => d::Annotation::ElectronDot(d::ElectronDot {
        anchor: reference(&anchor, &entries)?,
        radius,
        color: rgba(color),
      }),
      Annotation::Highlight {
        molecule,
        atoms,
        bonds,
        radius,
        color,
      } => {
        let e = entry(&molecule, &entries)?;
        d::Annotation::Highlight(d::Highlight {
          molecule: e.id,
          atoms: atoms.into_iter().map(|i| e.atom(i)).collect::<Result<_, _>>()?,
          bonds: bonds.into_iter().map(|pair| e.bond(pair)).collect::<Result<_, _>>()?,
          radius,
          color: rgba(color),
        })
      }
      Annotation::ElectronArrow {
        controls,
        from,
        to,
        bend,
        head,
        style,
      } => d::Annotation::ElectronArrow(d::ElectronArrow {
        controls: controls.map(|points| points.map(|p| Point::new(p[0], p[1]))),
        from: reference(&from, &entries)?,
        to: reference(&to, &entries)?,
        bend,
        head: match head {
          Head::Pair => d::ElectronHead::Pair,
          Head::FishhookLeft => d::ElectronHead::FishhookLeft,
          Head::FishhookRight => d::ElectronHead::FishhookRight,
        },
        style: style.style(),
      }),
      Annotation::ReactionArrow {
        start,
        end,
        equilibrium,
        style,
      } => d::Annotation::ReactionArrow(d::ReactionArrow {
        start: Point::new(start[0], start[1]),
        end: Point::new(end[0], end[1]),
        kind: if equilibrium {
          d::ReactionArrowKind::Equilibrium
        } else {
          d::ReactionArrowKind::Forward
        },
        style: style.style(),
      }),
      Annotation::LonePair {
        anchor,
        angle,
        separation,
        radius,
        color,
      } => d::Annotation::LonePair(d::LonePair {
        anchor: reference(&anchor, &entries)?,
        angle,
        separation,
        radius,
        color: rgba(color),
      }),
    });
  }
  Ok((diagram.render()?, count, diagnostics))
}
