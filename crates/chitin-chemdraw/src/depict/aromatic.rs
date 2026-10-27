//! Conservative inner-circle annotation from supplied aromatic facts and geometry.
//!
//! Chemical validation and H resolution still use the detached integer model.
//! A circle is a display substitution only after a complete independent ring
//! passes geometric and label clearance checks; failed rings retain their doubles.

use std::{
  collections::{BTreeMap, BTreeSet},
  f64::consts::TAU,
};

use chitin_chem::{
  AtomId, BondId, BondOrder, MolecularGraph,
  perception::{RingBlock, analyze_rings},
};

use super::{DepictError as Error, DepictOptions, LabelBounds, hydrogens::ExpandedHydrogen};
use crate::{
  geometry::Point,
  scene::{Circle, Primitive, Source},
};

/// Representation of explicitly aromatic cycles, separate from chemical bond order.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum AromaticStyle {
  /// Retain the current detached alternating single/double representation.
  #[default]
  Kekule,
  /// Use an inner circle for eligible independent simple aromatic rings.
  /// Fused/shared-vertex cycles and unsuitable geometry retain Kekulé strokes.
  Circle,
}

/// Accepted annotations and the exact edges whose inner double strokes they replace.
#[derive(Default)]
pub(super) struct Plan {
  /// Suppress double strokes only after all checks for that ring succeed.
  pub edges: BTreeSet<BondId>,
  /// Generic circles, sharing foreground/width with ordinary skeleton strokes.
  pub circles: Vec<Circle>,
}

/// Selects simple cyclic blocks without perceiving or inventing aromaticity.
///
/// # Parameters
/// - `graph`: original atom flags and aromatic bond designations, before resolving.
/// - `points`, `labels`: complete drawing-space coordinates and expanded envelopes.
/// - `attached`: separately drawn chiral H labels/marks, also treated as obstacles.
/// - `options`: validated display style, scale, stroke and label clearance.
///
/// # Returns
/// Independent rank-one cycles with all-aromatic atoms/edges can replace their
/// inner double strokes. Every other ring remains unchanged. Geometry must be
/// near-regular and leave a safe inner disk free of labels and non-ring bonds.
/// A source ring bond locates the annotation without assigning a chemical ring ID.
///
/// # Errors
/// Propagates inconsistent graph references; ineligible geometry is a fallback,
/// not permission to omit a ring or publish a partial substitution.
pub(super) fn plan(
  graph: &MolecularGraph,
  points: &BTreeMap<AtomId, Point>,
  labels: &BTreeMap<AtomId, LabelBounds>,
  attached: &[ExpandedHydrogen],
  options: &DepictOptions,
) -> Result<Plan, Error> {
  let mut result = Plan::default();
  if options.aromatic_style != AromaticStyle::Circle {
    return Ok(result);
  }
  let topology = analyze_rings(graph)?;
  let mut incidence = BTreeMap::<AtomId, usize>::new();
  for block in topology.blocks().iter().filter(|block| block.cycle_rank() > 0) {
    for &atom in block.atoms() {
      *incidence.entry(atom).or_default() += 1;
    }
  }
  for block in topology.blocks().iter().filter(|block| block.cycle_rank() == 1) {
    if block
      .atoms()
      .iter()
      .any(|id| incidence[id] != 1 || graph.atom(*id).is_none_or(|a| !a.is_aromatic()))
      || block
        .bonds()
        .iter()
        .any(|id| graph.bond(*id).is_none_or(|b| b.order() != BondOrder::Aromatic))
    {
      continue;
    }
    let cycle = ordered(block, graph)?;
    let vertices: Vec<_> = cycle.iter().map(|id| points[id]).collect();
    let Some((center, radius)) = fit(&vertices, options) else {
      continue;
    };
    let outer = radius + options.stroke_width / 2.;
    if labels.values().any(|bounds| {
      center.distance(Point::new(
        center.x.clamp(bounds.min.x, bounds.max.x),
        center.y.clamp(bounds.min.y, bounds.max.y),
      )) <= outer
    }) {
      continue;
    }
    let mut covered = false;
    for hydrogen in attached {
      let [min, max] = hydrogen.label.envelope()?;
      let nearest = Point::new(center.x.clamp(min.x, max.x), center.y.clamp(min.y, max.y));
      if center.distance(nearest) <= outer + options.label_padding
        || hydrogen.wedge.iter().any(|mark| match mark {
          Primitive::Line(line) => segment_distance(center, line.start, line.end) <= outer + line.width / 2.,
          Primitive::Polygon(polygon) => polygon
            .points
            .iter()
            .zip(polygon.points.iter().cycle().skip(1))
            .any(|(&a, &b)| segment_distance(center, a, b) <= outer),
          _ => false,
        })
      {
        covered = true;
        break;
      }
    }
    if covered {
      continue;
    }
    let edges: BTreeSet<_> = block.bonds().iter().copied().collect();
    let mut obstructed = false;
    for (_, bond) in graph.bonds().filter(|(id, _)| !edges.contains(id)) {
      let [a, b] = bond.atoms();
      let distance = segment_distance(center, points[&a], points[&b]);
      if !distance.is_finite() || distance <= outer + options.label_padding + options.stroke_width / 2. {
        // Adjacent substituents normally point outward; inward/manual skeleton
        // geometry must not be covered by an added annotation.
        obstructed = true;
        break;
      }
    }
    if obstructed {
      continue;
    }
    result.edges.extend(edges);
    result.circles.push(Circle {
      center,
      radius,
      width: options.stroke_width,
      color: options.foreground,
      source: Some(Source::AromaticRing(block.bonds()[0])),
    });
  }
  Ok(result)
}

/// Traverses a rank-one block in source-stable order, verifying degree and closure.
fn ordered(block: &RingBlock, graph: &MolecularGraph) -> Result<Vec<AtomId>, Error> {
  let mut rows = BTreeMap::<AtomId, Vec<AtomId>>::new();
  for &id in block.bonds() {
    let [a, b] = graph.bond(id).ok_or(chitin_chem::GraphError::MissingBond(id))?.atoms();
    rows.entry(a).or_default().push(b);
    rows.entry(b).or_default().push(a);
  }
  for row in rows.values_mut() {
    row.sort_unstable();
  }
  let start = block.atoms()[0];
  if rows.values().any(|row| row.len() != 2) {
    return Err(Error::Bond(block.bonds()[0]));
  }
  let mut cycle = vec![start];
  let (mut previous, mut node) = (start, rows[&start][0]);
  while node != start && cycle.len() < rows.len() {
    cycle.push(node);
    let next = rows[&node]
      .iter()
      .copied()
      .find(|&id| id != previous)
      .ok_or(Error::Bond(block.bonds()[0]))?;
    previous = node;
    node = next;
  }
  if node != start || cycle.len() != rows.len() {
    return Err(Error::Bond(block.bonds()[0]));
  }
  Ok(cycle)
}

/// Fits only near-regular, singly wound polygons; no coordinate warping is done.
///
/// Radius is 70% of the smallest edge apothem. A 5% radial tolerance and a
/// 0.05-radian turning tolerance accept floating-point layout noise but reject
/// distorted/manual star or concave polygons. Thick strokes fall back instead
/// of producing a filled-looking or touching inner mark.
fn fit(vertices: &[Point], options: &DepictOptions) -> Option<(Point, f64)> {
  let count = vertices.len();
  if count < 3 {
    return None;
  }
  let center = Point::new(
    vertices.iter().map(|p| p.x / count as f64).sum(),
    vertices.iter().map(|p| p.y / count as f64).sum(),
  );
  let radial = center.distance(vertices[0]);
  if !center.is_finite() || !radial.is_finite() || radial <= 0. {
    return None;
  }
  let expected = TAU / count as f64;
  let mut orientation = 0.;
  let mut apothem = f64::INFINITY;
  for (&a, &b) in vertices.iter().zip(vertices.iter().cycle().skip(1)) {
    if (center.distance(a) / radial - 1.).abs() > 0.05 {
      return None;
    }
    let (a, b) = (
      Point::new(a.x - center.x, a.y - center.y),
      Point::new(b.x - center.x, b.y - center.y),
    );
    let turn = (a.x * b.y - a.y * b.x).atan2(a.x * b.x + a.y * b.y);
    if (turn.abs() - expected).abs() > 0.05 || (orientation != 0. && turn * orientation <= 0.) {
      return None;
    }
    orientation = turn;
    apothem = apothem.min(segment_distance(Point::new(0., 0.), a, b));
  }
  let radius = 0.70 * apothem;
  (radius.is_finite()
    && radius > options.stroke_width * 2.
    && apothem - radius > options.stroke_width + options.label_padding)
    .then_some((center, radius))
}

/// Distance to a segment, treating a degenerate edge as unsuitable geometry.
fn segment_distance(p: Point, a: Point, b: Point) -> f64 {
  let (dx, dy) = (b.x - a.x, b.y - a.y);
  let length = dx * dx + dy * dy;
  if !length.is_finite() || length <= 0. {
    return f64::NAN;
  }
  let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / length).clamp(0., 1.);
  p.distance(a.translated(t * dx, t * dy))
}
