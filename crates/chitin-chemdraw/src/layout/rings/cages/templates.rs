//! Authored flat and perspective cage projections; no chemical facts or SMILES keys.

use std::f64::consts::PI;

use super::Point;

/// The complete core topologies supported by this initial template family.
#[derive(Debug, Clone, Copy)]
pub(super) enum Template {
  /// A tetrahedral graph with each of its six edges subdivided once.
  Adamantane,
  /// Two four-cycles with corresponding vertices joined.
  Cubane,
}

/// Outer hexagon, three inward subdivided paths and their common bridgehead.
const ADAMANTANE: &[[usize; 2]] = &[
  [0, 1],
  [1, 2],
  [2, 3],
  [3, 4],
  [4, 5],
  [5, 0],
  [0, 7],
  [7, 6],
  [2, 8],
  [8, 6],
  [4, 9],
  [9, 6],
];

/// Two corresponding four-cycles and their four joining cube edges.
const CUBANE: &[[usize; 2]] = &[
  [0, 1],
  [1, 2],
  [2, 3],
  [3, 0],
  [4, 5],
  [5, 6],
  [6, 7],
  [7, 4],
  [0, 4],
  [1, 5],
  [2, 6],
  [3, 7],
];

impl Template {
  /// Selects only a possible size/rank family; full adjacency matching follows.
  pub(super) fn candidate(atoms: usize, bonds: usize, rank: usize) -> Option<Self> {
    match (atoms, bonds, rank) {
      (10, 12, 3) => Some(Self::Adamantane),
      (8, 12, 5) => Some(Self::Cubane),
      _ => None,
    }
  }

  /// Returns the template vertex count, not a chemical atom-name assignment.
  pub(super) fn len(self) -> usize {
    match self {
      Self::Adamantane => 10,
      Self::Cubane => 8,
    }
  }

  /// Returns the number of independent cycles in this complete skeleton.
  pub(super) fn rank(self) -> usize {
    match self {
      Self::Adamantane => 3,
      Self::Cubane => 5,
    }
  }

  /// Returns every template edge exactly once, with local endpoint rows.
  pub(super) fn edges(self) -> &'static [[usize; 2]] {
    match self {
      Self::Adamantane => ADAMANTANE,
      Self::Cubane => CUBANE,
    }
  }

  /// Builds immutable local adjacency for exact induced matching.
  pub(super) fn rows(self) -> Vec<Vec<usize>> {
    let mut rows = vec![Vec::new(); self.len()];
    for &[a, b] in self.edges() {
      rows[a].push(b);
      rows[b].push(a);
    }
    rows
  }

  /// Returns a crossing-free planar pose normalized to mean skeleton length one.
  ///
  /// Interior subdividing vertices have visible bends, not collinear hidden
  /// carbons. Unequal projected lengths are intentional drawing geometry, not
  /// physical equilibrium distances or a tetrahedral stereochemical assignment.
  pub(super) fn points(self) -> Vec<Point> {
    let points = match self {
      Self::Adamantane => {
        let mut points: Vec<_> = (0..6)
          .map(|i| {
            let angle = i as f64 * PI / 3.;
            Point::new(1.2 * angle.cos(), 1.2 * angle.sin())
          })
          .collect();
        points.push(Point::new(0., 0.));
        for i in 0..3 {
          let angle = i as f64 * 2. * PI / 3.;
          let (sin, cos) = angle.sin_cos();
          points.push(Point::new(0.6 * cos - 0.18 * sin, 0.6 * sin + 0.18 * cos));
        }
        points
      }
      Self::Cubane => [0.8, 0.35]
        .into_iter()
        .flat_map(|r| {
          [
            Point::new(-r, -r),
            Point::new(r, -r),
            Point::new(r, r),
            Point::new(-r, r),
          ]
        })
        .collect(),
    };
    self.normalize(points)
  }

  /// Returns a perspective pose with separately authored crossing order.
  ///
  /// Coordinates are y-up: nearer cage vertices are lower. The continuous
  /// front edges cross rear edges away from all source atom centers.
  /// This is presentation geometry, not a replacement for supplied stereo.
  pub(super) fn perspective(self) -> Option<Vec<Point>> {
    match self {
      Self::Adamantane => Some(self.normalize(vec![
        Point::new(0.5, 1.5),
        Point::new(-0.5, 1.),
        Point::new(-0.5, 0.),
        Point::new(0.5, -0.3),
        Point::new(1.5, 0.),
        Point::new(1.5, 1.),
        Point::new(0., -0.7),
        Point::new(0., 0.6),
        Point::new(-1., -1.),
        Point::new(1., -1.),
      ])),
      Self::Cubane => Some(self.normalize(vec![
        Point::new(0., 0.),
        Point::new(1., 0.),
        Point::new(1., 1.),
        Point::new(0., 1.),
        Point::new(0.5, 0.4),
        Point::new(1.5, 0.4),
        Point::new(1.5, 1.4),
        Point::new(0.5, 1.4),
      ])),
    }
  }

  /// Returns every ordered pair of template-local front and rear edges.
  pub(super) fn crossings(self) -> &'static [[[usize; 2]; 2]] {
    match self {
      Self::Adamantane => &[[[7, 6], [2, 3]]],
      Self::Cubane => &[[[1, 2], [4, 5]], [[2, 3], [4, 7]]],
    }
  }

  /// Identifies vertices inside the perspective silhouette for label clearance.
  pub(super) fn perspective_interior(self, vertex: usize) -> bool {
    match self {
      Self::Adamantane => matches!(vertex, 2 | 3 | 6 | 7),
      Self::Cubane => matches!(vertex, 2 | 4),
    }
  }

  /// Retains projected proportions while using one nominal mean bond unit.
  fn normalize(self, mut points: Vec<Point>) -> Vec<Point> {
    let mean = self
      .edges()
      .iter()
      .map(|&[a, b]| points[a].distance(points[b]))
      .sum::<f64>()
      / self.edges().len() as f64;
    for point in &mut points {
      point.x /= mean;
      point.y /= mean;
    }
    points
  }

  /// Penalizes attachments/labels inside the outer face without altering topology.
  pub(super) fn interior(self, vertex: usize) -> bool {
    vertex
      >= match self {
        Self::Adamantane => 6,
        Self::Cubane => 4,
      }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::layout::quality::{crosses, segment_distance};

  #[test]
  fn perspective_projections_should_have_exactly_their_declared_crossings_and_no_hidden_atoms()
  -> Result<(), &'static str> {
    for template in [Template::Adamantane, Template::Cubane] {
      let points = template.perspective().ok_or("missing projection")?;
      let mut count = 0;
      for (i, &[a, b]) in template.edges().iter().enumerate() {
        for (node, &point) in points.iter().enumerate() {
          if node != a && node != b {
            assert!(segment_distance(point, points[a], points[b]) > 0.08);
          }
        }
        for &[c, d] in &template.edges()[i + 1..] {
          if ![a, b].contains(&c) && ![a, b].contains(&d) && crosses(points[a], points[b], points[c], points[d]) {
            count += 1;
            let same = |x: [usize; 2], y: [usize; 2]| x == y || x == [y[1], y[0]];
            assert!(
              template
                .crossings()
                .iter()
                .any(|&[front, back]| (same([a, b], front) && same([c, d], back))
                  || (same([a, b], back) && same([c, d], front)))
            );
          }
        }
      }
      assert_eq!(count, template.crossings().len());
      for (i, &point) in points.iter().enumerate() {
        assert!(points[i + 1..].iter().all(|&other| point.distance(other) >= 0.55));
      }
    }
    Ok(())
  }

  #[test]
  fn authored_poses_should_be_planar_distinct_and_mean_unit_without_hidden_atoms() {
    for template in [Template::Adamantane, Template::Cubane] {
      let points = template.points();
      let rows = template.rows();
      assert_eq!(template.edges().len() + 1 - points.len(), template.rank());
      for (i, &point) in points.iter().enumerate() {
        assert!(point.is_finite());
        assert!(points[i + 1..].iter().all(|&other| point.distance(other) >= 0.55));
        if let &[a, b] = rows[i].as_slice() {
          let u = Point::new(points[a].x - point.x, points[a].y - point.y);
          let v = Point::new(points[b].x - point.x, points[b].y - point.y);
          assert!((u.x * v.y - u.y * v.x).abs() > 0.05);
        }
      }
      let mut mean = 0.;
      for (i, &[a, b]) in template.edges().iter().enumerate() {
        mean += points[a].distance(points[b]) / template.edges().len() as f64;
        for (node, &point) in points.iter().enumerate() {
          if node != a && node != b {
            assert!(segment_distance(point, points[a], points[b]) > 0.08);
          }
        }
        for &[c, d] in &template.edges()[i + 1..] {
          if ![a, b].contains(&c) && ![a, b].contains(&d) {
            assert!(!crosses(points[a], points[b], points[c], points[d]));
          }
        }
      }
      assert!((mean - 1.).abs() < 1e-12);
    }
  }
}
