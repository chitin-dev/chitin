//! Chemical depiction from supplied coordinates into generic scene commands.
//!
//! Carbon suppression, attached-H labels, charge/isotope placement, and bond
//! clipping live here, never in SVG export. Source facts remain unchanged.

use crate::{
  font::{FontError, FontFace},
  geometry::Point,
  layout::Layout2d,
  scene::{Color, Drawing, DrawingError, Primitive, Source, Text, positive},
};
use chitin_chem::perception::ValenceModel;
use chitin_chem::{AtomId, BondId, BondOrder, Element, GraphError, MolecularGraph};
use std::collections::BTreeMap;

mod aromatic;
mod bonds;
mod color;
mod hydrogens;
mod labels;
mod stereo;

pub use aromatic::AromaticStyle;
pub use color::{BondColorStyle, ColorScheme};
pub use labels::measurement_requests;

/// Display policy for one declared H on an annotated tetrahedral carbon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChiralHydrogenMode {
  /// Keep attached H skeletal and put the wedge on an existing graph bond.
  Implicit,
  /// Draw a separate H label and C–H wedge without adding graph entities.
  Explicit,
}

/// Policy for obtaining label H counts; explicit H vertices are never collapsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HydrogenMode {
  /// Display only known fixed/declaration counts; unresolved inference is an error.
  KnownOnly,
  /// A named model may resolve atoms whose input policy explicitly permits inference.
  Infer(ValenceModel),
}

/// Depiction sizes and display policies, independent of normalized layout units.
#[derive(Debug, Clone, PartialEq)]
pub struct DepictOptions {
  /// Drawing units per normalized bond unit.
  pub bond_length: f64,
  /// Positive element-label font size.
  pub font_size: f64,
  /// Positive bond stroke width.
  pub stroke_width: f64,
  /// Positive full width at the broad end of a tetrahedral wedge.
  pub wedge_width: f64,
  /// Separation of adjacent double/triple strokes.
  pub bond_spacing: f64,
  /// Retained axis-length fraction for an auxiliary one-sided double stroke.
  pub short_bond_ratio: f64,
  /// Font identity shared by measurement, scene primitives and export.
  pub font: FontFace,
  /// Additional clearance around label envelopes before a bond starts.
  pub label_padding: f64,
  /// Uniform bond color and fallback for elements without a palette entry.
  pub foreground: Color,
  /// Atom palette used by labels and endpoint-colored bonds; defaults to element.
  pub color_scheme: ColorScheme,
  /// Bond paint policy; defaults to two endpoint colors meeting at the axis midpoint.
  pub bond_color_style: BondColorStyle,
  /// Element-mode overrides, including optional carbon/H entries and alpha.
  /// Ignored in monochrome mode; absent entries use the builtin palette/fallback.
  pub element_colors: BTreeMap<Element, Color>,
  /// Aromatic ring display policy; does not change supplied aromatic facts.
  pub aromatic_style: AromaticStyle,
  /// Explicit H-resolution model; no domain facts are overwritten.
  pub hydrogens: HydrogenMode,
  /// Whether annotated carbon's declared H becomes a separate drawn ligand.
  /// Explicit graph H vertices are unaffected and are never drawn twice.
  pub chiral_hydrogens: ChiralHydrogenMode,
}

impl Default for DepictOptions {
  /// Uses standard skeletal drawing proportions with a 48-unit bond length.
  fn default() -> Self {
    Self {
      bond_length: 48.,
      font_size: 48. * 11. / 30.,
      stroke_width: 1.6,
      wedge_width: 48. * 0.14,
      bond_spacing: 48. * 0.17,
      short_bond_ratio: 0.8,
      font: FontFace::NotoSans,
      label_padding: 2.,
      foreground: Color::BLACK,
      color_scheme: ColorScheme::Element,
      bond_color_style: BondColorStyle::Split,
      element_colors: BTreeMap::new(),
      aromatic_style: AromaticStyle::Kekule,
      hydrogens: HydrogenMode::Infer(ValenceModel::SmilesOrganicV1),
      chiral_hydrogens: ChiralHydrogenMode::Implicit,
    }
  }
}

/// Information that cannot be drawn faithfully by the current depiction profile.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DepictError {
  /// Supplied coordinates invert or flatten an annotated double bond.
  #[error("cannot depict double-bond configuration at bond {0}")]
  DoubleBondStereo(BondId),
  /// The supplied local stereo cannot be shown unambiguously in these coordinates.
  #[error("cannot depict tetrahedral configuration at atom {0}")]
  Stereo(AtomId),
  /// Supplied aromatic facts cannot be assigned by the restricted domain model.
  #[error(transparent)]
  Kekule(#[from] chitin_chem::perception::KekuleError),
  /// The scene font cannot represent the requested chemical label.
  #[error(transparent)]
  Font(#[from] FontError),
  /// Invalid finite/positive sizing options.
  #[error("invalid depiction options")]
  Options,
  /// A live graph atom has no position in the supplied coordinate namespace.
  #[error("missing position for atom {0}")]
  MissingPosition(AtomId),
  /// Coordinates contain an atom that is not live in this graph.
  #[error("unexpected position for atom {0}")]
  UnexpectedPosition(AtomId),
  /// Charge or declared-H knowledge is required to choose labels/suppression.
  #[error("required input facts are unknown for atom {0}")]
  UnknownFacts(AtomId),
  /// Open-shell glyph conventions are not implemented yet.
  #[error("explicit radical depiction is unsupported at atom {0}")]
  Radical(AtomId),
  /// H inference is not allowed or cannot be resolved by the selected model.
  #[error("cannot resolve label hydrogens for atom {0}")]
  Hydrogens(AtomId),
  /// An unknown order cannot be drawn without inventing bond semantics.
  #[error("unsupported bond order at bond {0}")]
  Bond(BondId),
  /// Endpoints coincide or label envelopes leave no visible stroke.
  #[error("bond {0} has no drawable length after label clipping")]
  OccludedBond(BondId),
  /// A source graph reference is inconsistent.
  #[error(transparent)]
  Topology(#[from] GraphError),
  /// Derived scene geometry cannot be represented safely.
  #[error(transparent)]
  Drawing(#[from] DrawingError),
}

/// Rectangular label envelope, expanded before clipping round-capped strokes.
#[derive(Clone, Copy)]
struct LabelBounds {
  /// Minimum x/y of all runs and their stroke-safe clearance.
  min: Point,
  /// Maximum x/y; used for endpoint clipping, not a chemical bounding volume.
  max: Point,
}

/// Resolved label geometry, separate from charge/H input facts and visual sizing.
struct LabelPlacement {
  /// Atom anchor after scaling and the y-up to y-down transform.
  center: Point,
  /// Attached H extends left when neighboring geometry occupies the right side.
  hydrogen_left: bool,
}

/// Adds one generic text run and returns its right baseline edge.
///
/// # Errors
/// Missing glyphs and invalid font measurements fail before appending a run.
fn text(
  primitives: &mut Vec<Primitive>,
  atom: AtomId,
  origin: Point,
  content: String,
  size: f64,
  options: &DepictOptions,
) -> Result<f64, DepictError> {
  let width = options.font.measure(&content, size)?.advance;
  primitives.push(Primitive::Text(Text {
    font: options.font.clone(),
    origin,
    content,
    font_size: size,
    width,
    color: options.foreground,
    source: Some(Source::Atom(atom)),
  }));
  Ok(origin.x + width)
}

/// Emits separate base/subscript/superscript runs and computes their union.
///
/// # Parameters
/// - `primitives`: append-only text-command destination in painter's order.
/// - `recipe`: resolved chemical fragments, shared with host measurement requests.
/// - `placement`: transformed atom anchor and resolved horizontal H direction.
/// - `options`: validated sizes, color, and label clearance.
///
/// # Returns
/// An envelope expanded by label padding and half the stroke width. Unknown
/// radical state is left unspecified; a known radical is rejected by `depict`.
fn label(
  primitives: &mut Vec<Primitive>,
  recipe: &labels::Recipe,
  placement: LabelPlacement,
  options: &DepictOptions,
) -> Result<LabelBounds, DepictError> {
  let LabelPlacement { center, hydrogen_left } = placement;
  let id = recipe.id;
  let start = primitives.len();
  let size = options.font_size;
  let base = options.font.measure(recipe.element.symbol(), size)?;
  let baseline = center.y - (base.min.y + base.max.y) / 2.;
  let left = center.x - (base.min.x + base.max.x) / 2.;
  let mut right = text(
    primitives,
    id,
    Point::new(left, baseline),
    recipe.element.symbol().into(),
    size,
    options,
  )?;
  let mut prefix_right = left;
  if let Some(content) = &recipe.isotope {
    let small = size * 0.7;
    prefix_right = left - options.font.measure(content, small)?.advance;
    text(
      primitives,
      id,
      Point::new(prefix_right, baseline - size * 0.5),
      content.clone(),
      small,
      options,
    )?;
  }
  if recipe.hydrogens > 0 {
    let count = recipe.hydrogen_count().unwrap_or_default();
    let h_width = options.font.measure("H", size)?.advance;
    let count_width = if count.is_empty() {
      0.
    } else {
      options.font.measure(&count, size * 0.7)?.advance
    };
    let h_start = if hydrogen_left {
      prefix_right - h_width - count_width
    } else {
      right
    };
    let mut h_right = text(primitives, id, Point::new(h_start, baseline), "H".into(), size, options)?;
    if recipe.hydrogens > 1 {
      h_right = text(
        primitives,
        id,
        Point::new(h_right, baseline + size * 0.25),
        count,
        size * 0.7,
        options,
      )?;
    }
    if !hydrogen_left {
      right = h_right;
    }
  }
  if let Some(content) = &recipe.charge {
    text(
      primitives,
      id,
      Point::new(right, baseline - size * 0.5),
      content.clone(),
      size * 0.7,
      options,
    )?;
  }
  let padding = options.label_padding + options.stroke_width / 2.;
  let mut bounds = LabelBounds {
    min: center,
    max: center,
  };
  // All runs belong to this atom, including H counts, isotope and charge.
  // Resolve once so annotations cannot acquire a different elemental identity.
  let color = options.label_color(recipe.element);
  for primitive in &mut primitives[start..] {
    if let Primitive::Text(run) = primitive {
      run.color = color;
      let [min, max] = run.envelope()?;
      bounds.min.x = bounds.min.x.min(min.x - padding);
      bounds.min.y = bounds.min.y.min(min.y - padding);
      bounds.max.x = bounds.max.x.max(max.x + padding);
      bounds.max.y = bounds.max.y.max(max.y + padding);
    }
  }
  Ok(bounds)
}

/// Clips a segment's starting point out of its own label envelope.
///
/// # Parameters
/// - `start`, `end`: oriented offset stroke, already checked for nonzero length.
/// - `bounds`: expanded endpoint-label envelope, or no label for skeletal carbon.
///
/// # Returns
/// Returns no point when the entire segment lies inside the envelope. The
/// endpoint direction is retained; clipping never extends or reverses a bond.
/// A start already outside the envelope is retained. Slab exit parameters
/// determine clipping without modifying the opposite endpoint's geometry.
fn clip(start: Point, end: Point, bounds: Option<LabelBounds>) -> Option<Point> {
  let Some(bounds) = bounds else {
    return Some(start);
  };
  if start.x < bounds.min.x || start.x > bounds.max.x || start.y < bounds.min.y || start.y > bounds.max.y {
    return Some(start);
  }
  let dx = end.x - start.x;
  let dy = end.y - start.y;
  let tx = if dx > 0. {
    (bounds.max.x - start.x) / dx
  } else if dx < 0. {
    (bounds.min.x - start.x) / dx
  } else {
    f64::INFINITY
  };
  let ty = if dy > 0. {
    (bounds.max.y - start.y) / dy
  } else if dy < 0. {
    (bounds.min.y - start.y) / dy
  } else {
    f64::INFINITY
  };
  let exit = tx.min(ty);
  if exit >= 1. {
    None
  } else {
    Some(start.translated(dx * exit, dy * exit))
  }
}

/// Converts graph facts and explicit coordinates into a validated drawing scene.
///
/// # Parameters
/// - `graph`: chemical input; handles must share the layout's namespace.
/// - `layout`: automatic or caller-supplied normalized y-up coordinates.
/// - `options`: drawing scale, colors, aromatic style and explicit H-resolution policy.
///
/// # Returns
/// Bonds followed by labels in painter's order, with source IDs on every stroke
/// and text run. Ordinary neutral skeletal carbons are suppressed except when
/// isolated; isotope/charge/declared-H or bracket-state carbon stays labelled.
/// Layout and molecular facts are not modified. Supported aromatic input uses
/// the shared detached Kekulé model before drawing ordinary integer bonds.
/// Circle style may replace eligible original aromatic cycles after geometry
/// and annotation clearance checks; it does not alter validation or H counts.
/// No stereo is guessed; explicit H vertices remain independently visible.
/// `chiral_hydrogens` can display a declared H at an annotated carbon as a
/// separate label and C–H wedge. These primitives use `Source::AttachedHydrogen`
/// and do not allocate graph handles or change hydrogen counts.
///
/// # Errors
/// Rejects coordinate-coverage mismatch, unknown required facts, known radicals,
/// unsupported bond orders, unresolved label H, coincident/fully obscured bonds,
/// invalid sizing, or nonrepresentable derived geometry.
pub fn depict(graph: &MolecularGraph, layout: &Layout2d, options: &DepictOptions) -> Result<Drawing, DepictError> {
  validate_options(options)?;
  depict_validated(graph, layout, options)
}

/// Checks shared sizes before native or externally measured label preparation.
fn validate_options(options: &DepictOptions) -> Result<(), DepictError> {
  if ![
    options.bond_length,
    options.font_size,
    options.stroke_width,
    options.wedge_width,
    options.bond_spacing,
  ]
  .into_iter()
  .all(positive)
    || !options.label_padding.is_finite()
    || options.label_padding < 0.
    || !options.short_bond_ratio.is_finite()
    || options.short_bond_ratio <= 0.
    || options.short_bond_ratio > 1.
  {
    return Err(DepictError::Options);
  }
  Ok(())
}

/// Builds geometry using the same immutable font for label placement and bounds.
fn depict_validated(
  graph: &MolecularGraph,
  layout: &Layout2d,
  options: &DepictOptions,
) -> Result<Drawing, DepictError> {
  for &id in layout.positions().keys() {
    if graph.atom(id).is_none() {
      return Err(DepictError::UnexpectedPosition(id));
    }
  }
  let source_graph = graph;
  let resolved = crate::chemical::integer_bonds(graph)?;
  let graph = resolved.as_ref();
  // Check representation before H inference so missing bond semantics do not
  // masquerade as an unrelated hydrogen-model failure.
  for (id, bond) in graph.bonds() {
    if !matches!(bond.order(), BondOrder::Single | BondOrder::Double | BondOrder::Triple) {
      return Err(DepictError::Bond(id));
    }
  }
  let mut points = BTreeMap::new();
  for (id, _) in graph.atoms() {
    let point = layout.position(id).ok_or(DepictError::MissingPosition(id))?;
    points.insert(
      id,
      Point::new(point.x * options.bond_length, -point.y * options.bond_length),
    );
  }
  for (id, stereo) in graph.double_bond_stereos() {
    let [a, b] = stereo.ends();
    let [ra, rb] = stereo.references();
    if crate::stereo_geometry::relation([a, b, ra, rb].map(|atom| points[&atom])) != Some(stereo.relation()) {
      return Err(DepictError::DoubleBondStereo(id));
    }
  }
  let recipes = labels::prepare(graph, options)?;
  let mut labels = Vec::new();
  let mut envelopes = BTreeMap::new();
  let expanded = hydrogens::centers(graph, options);
  for (&id, recipe) in &recipes {
    let mut neighbor_x = 0.;
    for (neighbor, _) in graph.neighbors(id)? {
      let distance = points[&neighbor].distance(points[&id]);
      if positive(distance) {
        neighbor_x += (points[&neighbor].x - points[&id].x) / distance;
      }
    }
    let bounds = label(
      &mut labels,
      recipe,
      LabelPlacement {
        center: points[&id],
        hydrogen_left: neighbor_x > 1e-10,
      },
      options,
    )?;
    envelopes.insert(id, bounds);
  }
  let attached = hydrogens::plan(graph, &points, &envelopes, &expanded, options)?;
  let aromatic = aromatic::plan(source_graph, &points, &envelopes, &attached, options)?;
  let mut primitives = Vec::new();
  bonds::draw(
    graph,
    &points,
    &envelopes,
    &expanded,
    &aromatic.edges,
    options,
    &mut primitives,
  )?;
  primitives.extend(aromatic.circles.into_iter().map(Primitive::Circle));
  for attached in attached {
    primitives.extend(attached.wedge);
    labels.push(Primitive::Text(attached.label));
  }
  primitives.extend(labels);
  Ok(Drawing::new(primitives)?)
}
