//! Depiction-only expansion of declared H at annotated tetrahedral carbons.
//!
//! Placement considers existing ink and bonds but never edits graph topology or
//! layout coordinates. Every generated primitive refers to the original host.

use std::collections::{BTreeMap, BTreeSet};

use chitin_chem::{AtomId, Element, MolecularGraph, StereoLigand};

use super::{ChiralHydrogenMode, DepictError as Error, DepictOptions, LabelBounds, stereo};
use crate::{
  geometry::Point,
  scene::{Primitive, Source, Text},
};

/// A validated virtual C–H mark and measured H label, ready for painter ordering.
pub(super) struct ExpandedHydrogen {
  /// Generic wedge strokes/polygon; no graph edge is created for them.
  pub wedge: Vec<Primitive>,
  /// H glyph whose source identifies the declared-H host, not a fictitious atom.
  pub label: Text,
  /// Exact font envelope plus clearance, used to avoid subsequent H labels.
  bounds: LabelBounds,
  /// Unclipped center-to-H segment for avoiding later generated bond crossings.
  axis: [Point; 2],
}

/// Borrowed obstacle snapshot shared by all bounded H-placement candidates.
struct Obstacles<'a> {
  /// Source bonds for geometric intersection tests; no runtime ownership here.
  graph: &'a MolecularGraph,
  /// Existing drawing-space atom centers, never modified during placement.
  points: &'a BTreeMap<AtomId, Point>,
  /// Already prepared font envelopes, including isotope/charge runs.
  labels: &'a BTreeMap<AtomId, LabelBounds>,
  /// Scale used to normalize clearance costs across document sizes.
  bond_length: f64,
}

/// Finds only annotated carbon centers with a declared virtual H to display.
///
/// Explicit H vertices and unannotated CH groups are not expansion candidates.
pub(super) fn centers(graph: &MolecularGraph, options: &DepictOptions) -> BTreeSet<AtomId> {
  if options.chiral_hydrogens == ChiralHydrogenMode::Implicit {
    return BTreeSet::new();
  }
  graph
    .tetrahedral_centers()
    .filter_map(|(center, stereo)| {
      (graph.atom(center).is_some_and(|atom| atom.element() == Element::Carbon)
        && stereo.ligands().contains(&StereoLigand::Hydrogen))
      .then_some(center)
    })
    .collect()
}

/// Chooses measured H positions and correct C–H wedge polarities in stable order.
///
/// # Parameters
/// - `graph`, `points`: the same immutable graph/coordinate namespace.
/// - `labels`: already prepared nonexpanded labels, including host isotope/charge.
/// - `centers`: eligible source centers; not generated H identifiers.
/// - `options`: shared font, colors, bond scale and wedge geometry.
///
/// # Returns
/// One H glyph and one stereo mark per center. Candidates use local bond-vector
/// bisectors and opposite directions at 0.55/0.70/0.85 bond lengths. A bounded
/// local cost prefers clearance from labels, atoms, bonds and earlier H labels;
/// it is not a global layout repair or a collision-free guarantee.
///
/// # Errors
/// Rejects degenerate projections, missing glyphs, or completely obscured marks.
/// Selection never changes winding, atom IDs, graph H counts, or coordinates.
pub(super) fn plan(
  graph: &MolecularGraph,
  points: &BTreeMap<AtomId, Point>,
  labels: &BTreeMap<AtomId, LabelBounds>,
  centers: &BTreeSet<AtomId>,
  options: &DepictOptions,
) -> Result<Vec<ExpandedHydrogen>, Error> {
  let mut result: Vec<ExpandedHydrogen> = Vec::new();
  let obstacles = Obstacles {
    graph,
    points,
    labels,
    bond_length: options.bond_length,
  };
  for &center in centers {
    let descriptor = graph.tetrahedral(center).ok_or(Error::Stereo(center))?;
    let origin = points[&center];
    let mut rays = Vec::new();
    for (neighbor, _) in graph.neighbors(center)? {
      let p = points[&neighbor];
      rays.push(
        unit(Point::new(
          (p.x - origin.x) / options.bond_length,
          (p.y - origin.y) / options.bond_length,
        ))
        .ok_or(Error::Stereo(center))?,
      );
    }
    let mut directions = Vec::new();
    let sum = Point::new(rays.iter().map(|p| p.x).sum(), rays.iter().map(|p| p.y).sum());
    if let Some(direction) = unit(Point::new(-sum.x, -sum.y)) {
      directions.push(direction);
    }
    for (i, &ray) in rays.iter().enumerate() {
      directions.push(Point::new(-ray.x, -ray.y));
      for &other in &rays[i + 1..] {
        if let Some(direction) = unit(Point::new(ray.x + other.x, ray.y + other.y)) {
          directions.push(direction);
          directions.push(Point::new(-direction.x, -direction.y));
        }
      }
    }
    let mut best: Option<(f64, ExpandedHydrogen)> = None;
    for direction in directions {
      let alignment = rays
        .iter()
        .map(|ray| ray.x * direction.x + ray.y * direction.y)
        .fold(-1., f64::max);
      // Do not put a virtual ligand along an existing outward bond ray.
      if alignment > 0.92 {
        continue;
      }
      for fraction in [0.70, 0.55, 0.85] {
        let position = origin.translated(
          direction.x * options.bond_length * fraction,
          direction.y * options.bond_length * fraction,
        );
        let Some(solid) = stereo::solid_for(descriptor, origin, points, StereoLigand::Hydrogen, Some(position))? else {
          continue;
        };
        let (label, bounds) = label(center, position, options)?;
        let mut wedge = Vec::new();
        if stereo::wedge(
          [origin, position],
          [labels.get(&center).copied(), Some(bounds)],
          solid,
          Source::AttachedHydrogen(center),
          options,
          &mut wedge,
        )
        .is_err()
        {
          continue;
        }
        let score = obstacles.clearance(center, [origin, position], bounds, &result)
          + alignment * alignment.max(0.)
          + (fraction - 0.70_f64).abs() * 0.1;
        if score.is_finite() && best.as_ref().is_none_or(|(previous, _)| score < *previous - 1e-9) {
          best = Some((
            score,
            ExpandedHydrogen {
              wedge,
              label,
              bounds,
              axis: [origin, position],
            },
          ));
        }
      }
    }
    result.push(best.map(|(_, drawing)| drawing).ok_or(Error::Stereo(center))?);
  }
  Ok(result)
}

/// Normalizes a nondegenerate projected vector without a world-axis fallback.
fn unit(p: Point) -> Option<Point> {
  let length = p.x.hypot(p.y);
  (length.is_finite() && length > 1e-8).then(|| Point::new(p.x / length, p.y / length))
}

/// Prepares an upright H glyph with the same metrics/centering as normal labels.
fn label(center: AtomId, position: Point, options: &DepictOptions) -> Result<(Text, LabelBounds), Error> {
  let width = options.font.measure("H", options.font_size)?.advance;
  let run = Text {
    font: options.font,
    origin: position.translated(-width / 2., options.font_size * 0.35),
    content: "H".into(),
    font_size: options.font_size,
    width,
    color: options.label_color(Element::Carbon),
    source: Some(Source::AttachedHydrogen(center)),
  };
  let [min, max] = run.envelope()?;
  let margin = options.label_padding + options.stroke_width / 2.;
  Ok((
    run,
    LabelBounds {
      min: min.translated(-margin, -margin),
      max: max.translated(margin, margin),
    },
  ))
}

impl Obstacles<'_> {
  /// Scores exact label intersections and local bond/atom clearance in drawing units.
  ///
  /// Existing crowding remains the layout's responsibility. Soft scores allow a
  /// complete best candidate when every local direction has some congestion.
  fn clearance(
    &self,
    center: AtomId,
    [start, end]: [Point; 2],
    bounds: LabelBounds,
    placed: &[ExpandedHydrogen],
  ) -> f64 {
    let mut cost = 0.;
    for other in self.labels.values().chain(placed.iter().map(|h| &h.bounds)) {
      if overlaps(bounds, *other) {
        cost += 1000.;
      }
      if intersects([start, end], *other) {
        cost += 10.;
      }
    }
    for (&id, &point) in self.points {
      if id == center {
        continue;
      }
      if contains(bounds, point) {
        cost += 1000.;
      }
      let distance = segment_distance(point, [start, end]) / self.bond_length;
      if distance < 0.25 {
        cost += (0.25 - distance) * 20.;
      }
    }
    for (_, bond) in self.graph.bonds() {
      let [a, b] = bond.atoms();
      let segment = [self.points[&a], self.points[&b]];
      if intersects(segment, bounds) {
        cost += 100.;
      }
      if a != center && b != center && crosses([start, end], segment) {
        cost += 50.;
      }
    }
    // Later H bonds should not cross already accepted H marks, not merely labels.
    for hydrogen in placed {
      if crosses([start, end], hydrogen.axis) {
        cost += 50.;
      }
      if hydrogen.wedge.iter().any(|p| match p {
        Primitive::Line(line) => intersects([line.start, line.end], bounds),
        Primitive::Polygon(polygon) => polygon.points.iter().any(|&p| contains(bounds, p)),
        _ => false,
      }) {
        cost += 100.;
      }
    }
    cost
  }
}

/// Tests overlap of measured, clearance-expanded glyph rectangles.
fn overlaps(a: LabelBounds, b: LabelBounds) -> bool {
  a.min.x < b.max.x && b.min.x < a.max.x && a.min.y < b.max.y && b.min.y < a.max.y
}

/// Tests inclusion in the closed ink/clearance rectangle.
fn contains(bounds: LabelBounds, p: Point) -> bool {
  p.x >= bounds.min.x && p.x <= bounds.max.x && p.y >= bounds.min.y && p.y <= bounds.max.y
}

/// Uses a slab interval to test a complete segment against a glyph rectangle.
fn intersects([a, b]: [Point; 2], bounds: LabelBounds) -> bool {
  let mut interval = [0_f64, 1_f64];
  for (start, delta, min, max) in [
    (a.x, b.x - a.x, bounds.min.x, bounds.max.x),
    (a.y, b.y - a.y, bounds.min.y, bounds.max.y),
  ] {
    if delta.abs() < 1e-12 {
      if start < min || start > max {
        return false;
      }
    } else {
      let t = [(min - start) / delta, (max - start) / delta];
      interval[0] = interval[0].max(t[0].min(t[1]));
      interval[1] = interval[1].min(t[0].max(t[1]));
      if interval[0] > interval[1] {
        return false;
      }
    }
  }
  true
}

/// Computes point-to-segment distance without treating an infinite line as a bond.
fn segment_distance(p: Point, [a, b]: [Point; 2]) -> f64 {
  let length = a.distance(b);
  if length <= 0. || !length.is_finite() {
    return f64::INFINITY;
  }
  let direction = Point::new((b.x - a.x) / length, (b.y - a.y) / length);
  let along = ((p.x - a.x) * direction.x + (p.y - a.y) * direction.y).clamp(0., length);
  p.distance(a.translated(direction.x * along, direction.y * along))
}

/// Counts only proper crossings; shared bond endpoints are not intersections.
fn crosses([a, b]: [Point; 2], [c, d]: [Point; 2]) -> bool {
  let side = |a: Point, b: Point, p: Point| (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
  side(a, b, c) * side(a, b, d) < 0. && side(c, d, a) * side(c, d, b) < 0.
}
