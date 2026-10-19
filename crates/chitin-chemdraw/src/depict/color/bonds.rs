//! Split final geometry at the unclipped atom-axis bisector, before export.

use chitin_chem::Element;

use super::{BondColorStyle, Color, DepictOptions};
use crate::{
  geometry::Point,
  scene::{Line, LineCap, Polygon, Primitive},
};

/// Paint frame shared by all strokes of one bond, including stereo marks.
pub(in crate::depict) struct BondPaint {
  /// Original atom centers, not label-clipped or shortened stroke endpoints.
  axis: [Point; 2],
  /// Endpoint palette colors, or the uniform foreground twice.
  colors: [Color; 2],
}

impl BondPaint {
  /// Resolves atom colors once while retaining the original geometric split plane.
  pub(in crate::depict) fn new(axis: [Point; 2], elements: [Element; 2], options: &DepictOptions) -> Self {
    Self {
      axis,
      colors: match options.bond_color_style {
        BondColorStyle::Uniform => [options.foreground; 2],
        BondColorStyle::Split => elements.map(|element| options.label_color(element)),
      },
    }
  }

  /// Returns signed distance up to an axis-length factor; zero is the bisector.
  fn side(&self, point: Point) -> f64 {
    let [a, b] = self.axis;
    (point.x - a.x - (b.x - a.x) / 2.) * (b.x - a.x) + (point.y - a.y - (b.y - a.y) / 2.) * (b.y - a.y)
  }

  /// Emits at most two colored strokes, preserving source and outer caps.
  ///
  /// All parallel strokes share the atom-axis bisector. Butt caps at the join
  /// avoid overpainting translucent colors or adding a rounded color boundary.
  pub(in crate::depict) fn line(&self, mut line: Line, output: &mut Vec<Primitive>) {
    let a = self.side(line.start);
    let b = self.side(line.end);
    if self.colors[0] == self.colors[1] || a * b >= 0. {
      line.color = self.colors[usize::from(a + b > 0.)];
      output.push(Primitive::Line(line));
      return;
    }
    let t = a / (a - b);
    let middle = line
      .start
      .translated((line.end.x - line.start.x) * t, (line.end.y - line.start.y) * t);
    let mut first = line.clone();
    first.end = middle;
    first.caps[1] = LineCap::Butt;
    first.color = self.colors[usize::from(a > 0.)];
    line.start = middle;
    line.caps[0] = LineCap::Butt;
    line.color = self.colors[usize::from(b > 0.)];
    output.extend([Primitive::Line(first), Primitive::Line(line)]);
  }

  /// Clips a convex wedge into two nonoverlapping filled regions.
  ///
  /// Sutherland–Hodgman clipping retains the original outline and chemical
  /// source; no stroke is added along the internal color boundary.
  pub(in crate::depict) fn polygon(&self, mut polygon: Polygon, output: &mut Vec<Primitive>) {
    if self.colors[0] == self.colors[1] {
      polygon.color = self.colors[0];
      output.push(Primitive::Polygon(polygon));
      return;
    }
    for (index, sign) in [-1., 1.].into_iter().enumerate() {
      let mut points = Vec::new();
      for i in 0..polygon.points.len() {
        let a = polygon.points[i];
        let b = polygon.points[(i + 1) % polygon.points.len()];
        let da = self.side(a) * sign;
        let db = self.side(b) * sign;
        if da >= 0. {
          points.push(a);
        }
        if (da < 0. && db > 0.) || (da > 0. && db < 0.) {
          let t = da / (da - db);
          points.push(a.translated((b.x - a.x) * t, (b.y - a.y) * t));
        }
      }
      if points.len() >= 3 {
        output.push(Primitive::Polygon(Polygon {
          points,
          color: self.colors[index],
          source: polygon.source,
        }));
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  /// A deliberately asymmetric clipped wedge must keep the original bisector.
  fn paint() -> BondPaint {
    BondPaint::new(
      [Point::new(0., 0.), Point::new(10., 0.)],
      [Element::Carbon, Element::Oxygen],
      &DepictOptions::default(),
    )
  }

  #[test]
  fn polygon_halves_should_preserve_area_and_stay_on_their_endpoint_side() {
    let original = Polygon {
      points: vec![Point::new(1., 0.), Point::new(8., 2.), Point::new(8., -2.)],
      color: Color::BLACK,
      source: None,
    };
    let area = |polygon: &Polygon| {
      polygon
        .points
        .iter()
        .zip(polygon.points.iter().cycle().skip(1))
        .take(polygon.points.len())
        .map(|(a, b)| a.x * b.y - a.y * b.x)
        .sum::<f64>()
        .abs()
        / 2.
    };
    let mut output = Vec::new();
    paint().polygon(original.clone(), &mut output);
    assert_eq!(output.len(), 2);
    let mut total = 0.;
    for (index, primitive) in output.iter().enumerate() {
      let Primitive::Polygon(polygon) = primitive else {
        panic!("expected wedge polygon")
      };
      total += area(polygon);
      assert_eq!(polygon.color, paint().colors[index]);
      assert!(
        polygon
          .points
          .iter()
          .all(|p| if index == 0 { p.x <= 5. } else { p.x >= 5. })
      );
    }
    assert!((total - area(&original)).abs() < 1e-12);
  }

  #[test]
  fn shortened_strokes_and_transverse_hashes_should_not_invent_a_visible_half() {
    let mut output = Vec::new();
    for (start, end) in [
      (Point::new(6., 0.), Point::new(8., 0.)),
      (Point::new(7., -1.), Point::new(7., 1.)),
    ] {
      paint().line(
        Line {
          start,
          end,
          width: 1.,
          color: Color::BLACK,
          caps: [LineCap::Round; 2],
          source: None,
        },
        &mut output,
      );
    }
    assert_eq!(output.len(), 2);
    for primitive in output {
      let Primitive::Line(line) = primitive else {
        panic!("expected line")
      };
      assert_eq!(line.color, paint().colors[1]);
      assert_eq!(line.caps, [LineCap::Round; 2]);
    }
  }
}
