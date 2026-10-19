//! Background-bond discontinuities are scene geometry, not background-colored masks.

use std::collections::BTreeMap;

use chitin_chem::{BondId, BondOrder, MolecularGraph};

use super::{DepictError as Error, DepictOptions, Point};
use crate::{
  layout::Layout2d,
  scene::{Line, LineCap, Primitive, Source},
};

/// A rear-axis interval to remove, measured in drawing units from its first atom.
struct Cut {
  /// Original rear atom center, before glyph clipping and color splitting.
  origin: Point,
  /// Unit direction along the rear bond.
  direction: Point,
  /// Deleted interval around the proper intersection, including cap clearance.
  gap: [f64; 2],
}

/// Cuts only explicitly ordered rear bonds, retaining source IDs, color and caps.
///
/// # Returns
/// The front stroke is untouched. The rear stroke has a visible discontinuity
/// wider than twice a stroke width, including round-cap encroachment. Transparent
/// output needs no white paint and arbitrary unannotated intersections are not
/// assigned a front/back order. Split-color seams keep their original positions.
///
/// # Errors
/// Invalid/deleted bond references, nonproper intersections, wedge/multiple-bond
/// crossings and cuts erasing a complete bond fail instead of hiding chemistry.
pub(super) fn apply(
  graph: &MolecularGraph,
  layout: &Layout2d,
  points: &BTreeMap<chitin_chem::AtomId, Point>,
  options: &DepictOptions,
  primitives: &mut Vec<Primitive>,
) -> Result<(), Error> {
  if layout.bond_crossings().is_empty() {
    return Ok(());
  }
  let mut cuts = BTreeMap::<BondId, Vec<Cut>>::new();
  for crossing in layout.bond_crossings() {
    let endpoints = |id| -> Result<_, Error> {
      let bond = graph.bond(id).ok_or(chitin_chem::GraphError::MissingBond(id))?;
      if bond.order() != BondOrder::Single {
        return Err(Error::Perspective(id));
      }
      Ok(bond.atoms())
    };
    let front = endpoints(crossing.front)?;
    let back = endpoints(crossing.back)?;
    if front.iter().chain(&back).any(|&atom| graph.tetrahedral(atom).is_some()) {
      return Err(Error::Perspective(crossing.back));
    }
    if front.iter().any(|atom| back.contains(atom)) {
      return Err(Error::Perspective(crossing.back));
    }
    let [a, b] = back.map(|id| points[&id]);
    let [c, d] = front.map(|id| points[&id]);
    let rear = Point::new(b.x - a.x, b.y - a.y);
    let near = Point::new(d.x - c.x, d.y - c.y);
    let determinant = cross(rear, near);
    if determinant.abs() < 1e-10 {
      return Err(Error::Perspective(crossing.back));
    }
    let offset = Point::new(c.x - a.x, c.y - a.y);
    let t = cross(offset, near) / determinant;
    let u = cross(offset, rear) / determinant;
    if !(0. < t && t < 1. && 0. < u && u < 1.) {
      return Err(Error::Perspective(crossing.back));
    }
    let intersection = a.translated(rear.x * t, rear.y * t);
    for id in [crossing.front, crossing.back] {
      if !primitives.iter().any(|p| match p {
        Primitive::Line(line) if line.source == Some(Source::Bond(id)) => contains(line, intersection),
        _ => false,
      }) {
        return Err(Error::Perspective(id));
      }
    }
    let length = a.distance(b);
    let sine = determinant.abs() / (length * c.distance(d));
    // Include half a cap at either side. Even at 90 degrees the visible gap
    // remains three stroke widths, exceeding the two-width minimum.
    let half_gap = options.stroke_width * (1.5 / sine + 0.5);
    cuts.entry(crossing.back).or_default().push(Cut {
      origin: a,
      direction: Point::new(rear.x / length, rear.y / length),
      gap: [length * t - half_gap, length * t + half_gap],
    });
  }
  let mut output = Vec::with_capacity(primitives.len() + cuts.len());
  for primitive in std::mem::take(primitives) {
    let Primitive::Line(line) = primitive else {
      output.push(primitive);
      continue;
    };
    let Some(Source::Bond(id)) = line.source else {
      output.push(Primitive::Line(line));
      continue;
    };
    let Some(intervals) = cuts.get(&id) else {
      output.push(Primitive::Line(line));
      continue;
    };
    let mut pieces = vec![line];
    for cut in intervals {
      pieces = pieces.into_iter().flat_map(|line| cut.split(line)).collect();
    }
    output.extend(pieces.into_iter().map(Primitive::Line));
  }
  for &id in cuts.keys() {
    if !output
      .iter()
      .any(|p| matches!(p, Primitive::Line(line) if line.source == Some(Source::Bond(id))))
    {
      return Err(Error::OccludedBond(id));
    }
  }
  *primitives = output;
  Ok(())
}

/// Returns a two-dimensional signed area.
fn cross(a: Point, b: Point) -> f64 {
  a.x * b.y - a.y * b.x
}

/// Includes color-seam endpoints so a crossing may coincide with the split plane.
fn contains(line: &Line, point: Point) -> bool {
  let distance = line.start.distance(line.end);
  let direction = Point::new(line.end.x - line.start.x, line.end.y - line.start.y);
  let offset = Point::new(point.x - line.start.x, point.y - line.start.y);
  let projection = (offset.x * direction.x + offset.y * direction.y) / (distance * distance);
  cross(direction, offset).abs() <= distance * 1e-7 && (-1e-9..=1. + 1e-9).contains(&projection)
}

impl Cut {
  /// Returns the retained line pieces without changing exterior endpoint styling.
  fn split(&self, line: Line) -> Vec<Line> {
    let along = |p: Point| (p.x - self.origin.x) * self.direction.x + (p.y - self.origin.y) * self.direction.y;
    let a = along(line.start);
    let b = along(line.end);
    if (b - a).abs() < 1e-10 {
      return vec![line];
    }
    let t = self.gap.map(|value| (value - a) / (b - a));
    let low = t[0].min(t[1]);
    let high = t[0].max(t[1]);
    if high <= 0. || low >= 1. {
      return vec![line];
    }
    let delta = Point::new(line.end.x - line.start.x, line.end.y - line.start.y);
    let mut pieces = Vec::with_capacity(2);
    if low > 1e-10 {
      let mut first = line.clone();
      first.end = line.start.translated(delta.x * low, delta.y * low);
      first.caps[1] = LineCap::Round;
      pieces.push(first);
    }
    if high < 1. - 1e-10 {
      let mut last = line;
      last.start = last.start.translated(delta.x * high, delta.y * high);
      last.caps[0] = LineCap::Round;
      pieces.push(last);
    }
    pieces
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::scene::Color;

  /// Paint/source/caps deliberately differ from an ordinary opaque whole bond.
  fn line(start: f64, end: f64) -> Line {
    Line {
      start: Point::new(start, 0.),
      end: Point::new(end, 0.),
      width: 1.,
      color: Color {
        red: 20,
        green: 40,
        blue: 60,
        alpha: 128,
      },
      caps: [LineCap::Round, LineCap::Butt],
      source: None,
    }
  }

  #[test]
  fn reverse_axis_cuts_should_preserve_color_and_exterior_caps() {
    let cut = Cut {
      origin: Point::new(0., 0.),
      direction: Point::new(1., 0.),
      gap: [4., 6.],
    };
    let original = line(10., 0.);
    let pieces = cut.split(original.clone());
    assert_eq!(pieces.len(), 2);
    assert_eq!((pieces[0].start.x, pieces[0].end.x), (10., 6.));
    assert_eq!((pieces[1].start.x, pieces[1].end.x), (4., 0.));
    assert_eq!(pieces[0].caps, [LineCap::Round, LineCap::Round]);
    assert_eq!(pieces[1].caps, [LineCap::Round, LineCap::Butt]);
    assert!(
      pieces
        .iter()
        .all(|piece| piece.color == original.color && piece.source == original.source)
    );
  }

  #[test]
  fn cuts_spanning_a_color_seam_should_remove_both_halves_without_a_mask() {
    let cut = Cut {
      origin: Point::new(0., 0.),
      direction: Point::new(1., 0.),
      gap: [4., 6.],
    };
    let first = cut.split(line(0., 5.));
    let second = cut.split(line(5., 10.));
    assert_eq!(first.len(), 1);
    assert_eq!(second.len(), 1);
    assert_eq!(first[0].end.x, 4.);
    assert_eq!(second[0].start.x, 6.);
  }
}
