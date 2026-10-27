//! Positive similarity transforms of generic scene snapshots, including text metrics.

use super::{DiagramError, Placement};
use crate::scene::{Drawing, Primitive};

pub(super) fn drawing(input: &Drawing, placement: Placement) -> Result<Drawing, DiagramError> {
  let mut output = input.primitives().to_vec();
  for primitive in &mut output {
    match primitive {
      Primitive::CircularArc(arc) => {
        arc.center = placement.point(arc.center);
        arc.radius *= placement.scale;
        arc.width *= placement.scale;
      }
      Primitive::Line(line) => {
        line.start = placement.point(line.start);
        line.end = placement.point(line.end);
        line.width *= placement.scale;
      }
      Primitive::Circle(circle) => {
        circle.center = placement.point(circle.center);
        circle.radius *= placement.scale;
        circle.width *= placement.scale;
      }
      Primitive::Disk(disk) => {
        disk.center = placement.point(disk.center);
        disk.radius *= placement.scale;
      }
      Primitive::Polygon(polygon) => {
        for point in &mut polygon.points {
          *point = placement.point(*point);
        }
      }
      Primitive::Text(text) => {
        text.origin = placement.point(text.origin);
        text.font_size *= placement.scale;
        text.width *= placement.scale;
      }
      Primitive::CubicBezier(curve) => {
        curve.start = placement.point(curve.start);
        curve.control1 = placement.point(curve.control1);
        curve.control2 = placement.point(curve.control2);
        curve.end = placement.point(curve.end);
        curve.width *= placement.scale;
      }
      Primitive::Group(group) => group.drawing = drawing(&group.drawing, placement)?,
    }
  }
  Ok(Drawing::new(output)?)
}
