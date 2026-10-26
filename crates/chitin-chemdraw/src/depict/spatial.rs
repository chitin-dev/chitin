//! Depth cues for dense spatial cores, independent of exporter and background.

use crate::{
  layout::Layout2d,
  scene::{Primitive, Source},
};
use chitin_chem::MolecularGraph;

/// Keeps rear-surface bonds visible while distinguishing them from the front.
///
/// Every source stroke, including both double-bond strokes, is retained. Existing
/// element colors and alpha are multiplied, never replaced by background paint.
/// Ordinary 2D structures and separately authored cage templates are unchanged.
pub(super) fn apply(graph: &MolecularGraph, layout: &Layout2d, primitives: &mut [Primitive]) {
  if layout.drawing_depths().is_empty() {
    return;
  }
  let depth = |source: Option<Source>| match source {
    Some(Source::Bond(id) | Source::AromaticRing(id)) => graph.bond(id).and_then(|bond| {
      let [a, b] = bond.atoms();
      Some((layout.drawing_depths().get(&a)? + layout.drawing_depths().get(&b)?) / 2.)
    }),
    Some(Source::Atom(id) | Source::Charge(id) | Source::AttachedHydrogen(id)) => {
      layout.drawing_depths().get(&id).copied()
    }
    _ => None,
  };
  let bound = layout
    .drawing_depths()
    .values()
    .fold(0_f64, |value, depth| value.max(depth.abs()))
    .max(1e-12);
  for primitive in primitives {
    let (source, color) = match primitive {
      Primitive::CircularArc(arc) => (arc.source, &mut arc.color),
      Primitive::Line(line) => (line.source, &mut line.color),
      Primitive::Text(text) => (text.source, &mut text.color),
      Primitive::Polygon(polygon) => (polygon.source, &mut polygon.color),
      Primitive::Circle(circle) => (circle.source, &mut circle.color),
      Primitive::Disk(disk) => (disk.source, &mut disk.color),
      Primitive::CubicBezier(curve) => (curve.source, &mut curve.color),
      Primitive::Group(_) => continue,
    };
    if let Some(z) = depth(source) {
      let opacity = 0.25 + 0.75 * ((z / bound + 1.) / 2.).clamp(0., 1.);
      color.alpha = (f64::from(color.alpha) * opacity).round() as u8;
    }
  }
}
