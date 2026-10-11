//! Source-space intersection witnesses for analytic atoms and finite stick cylinders.

use glam::{Mat4, Vec3, Vec4};

pub(crate) struct SelectionVolume {
  mvp: Mat4,
  min: [f32; 2],
  max: [f32; 2],
  planes: [Vec4; 6],
  faces: [[Vec3; 3]; 12],
}

impl SelectionVolume {
  pub fn new(mvp: Mat4, min: [f32; 2], max: [f32; 2]) -> Option<Self> {
    let inverse = mvp.inverse();
    if !inverse.is_finite() || min[0] >= max[0] || min[1] >= max[1] {
      return None;
    }
    let corners = [
      [min[0], min[1], 0.],
      [max[0], min[1], 0.],
      [max[0], max[1], 0.],
      [min[0], max[1], 0.],
      [min[0], min[1], 1.],
      [max[0], min[1], 1.],
      [max[0], max[1], 1.],
      [min[0], max[1], 1.],
    ]
    .map(|p| inverse.project_point3(Vec3::from(p)));
    if corners.iter().any(|p| !p.is_finite()) {
      return None;
    }
    let planes = [
      Vec4::new(1., 0., 0., -min[0]),
      Vec4::new(-1., 0., 0., max[0]),
      Vec4::new(0., 1., 0., -min[1]),
      Vec4::new(0., -1., 0., max[1]),
      Vec4::Z,
      Vec4::new(0., 0., -1., 1.),
    ]
    .map(|plane| mvp.transpose() * plane);
    let faces = [
      [0, 1, 2],
      [0, 2, 3],
      [4, 5, 6],
      [4, 6, 7],
      [0, 1, 5],
      [0, 5, 4],
      [1, 2, 6],
      [1, 6, 5],
      [2, 3, 7],
      [2, 7, 6],
      [3, 0, 4],
      [3, 4, 7],
    ]
    .map(|face| face.map(|i| corners[i]));
    Some(Self {
      mvp,
      min,
      max,
      planes,
      faces,
    })
  }

  fn project(&self, p: Vec3, visitor: &mut impl FnMut([f32; 2])) {
    let clip = self.mvp * p.extend(1.);
    if clip.is_finite() && clip.w > 0. {
      // Face witnesses lie inside the rectangle; clamp roundoff at its boundaries.
      visitor([
        (clip.x / clip.w).clamp(self.min[0], self.max[0]),
        (clip.y / clip.w).clamp(self.min[1], self.max[1]),
      ]);
    }
  }

  /// Closest face/edge points find sphere slivers even when its center is outside the rectangle.
  pub fn sphere(&self, center: Vec3, radius: f32, mut visitor: impl FnMut([f32; 2])) {
    if self
      .planes
      .iter()
      .any(|plane| plane.dot(center.extend(1.)) < -radius * plane.truncate().length())
    {
      return;
    }
    if self.planes.iter().all(|plane| plane.dot(center.extend(1.)) >= 0.) {
      self.project(center, &mut visitor);
    }
    for face in self.faces {
      let point = closest_triangle(center, face);
      if center.distance_squared(point) <= radius * radius {
        self.project(point, &mut visitor);
      }
    }
  }

  /// Clips faces to the cylinder's axial slab, so spherical end-cap bounds cannot select a stick.
  pub fn cylinder(&self, a: Vec3, b: Vec3, radius: f32, mut visitor: impl FnMut([f32; 2])) {
    let axis = b - a;
    let length = axis.length_squared();
    if length <= 1.0e-12 {
      return;
    }
    if self.planes.iter().any(|plane| {
      let normal = plane.truncate();
      let radial = radius
        * (normal.length_squared() - normal.dot(axis).powi(2) / length)
          .max(0.)
          .sqrt();
      plane.dot(a.extend(1.)).max(plane.dot(b.extend(1.))) < -radial
    }) {
      return;
    }
    let mut interval: [f32; 2] = [0., 1.];
    let mut intersects = true;
    for plane in self.planes {
      let start = plane.dot(a.extend(1.));
      let end = plane.dot(b.extend(1.));
      if start < 0. && end < 0. {
        intersects = false;
        break;
      }
      if (start < 0.) != (end < 0.) {
        let t = start / (start - end);
        if start < 0. {
          interval[0] = interval[0].max(t);
        } else {
          interval[1] = interval[1].min(t);
        }
      }
    }
    if intersects && interval[0] <= interval[1] {
      for t in [interval[0], (interval[0] + interval[1]) * 0.5, interval[1]] {
        self.project(a + axis * t, &mut visitor);
      }
    }
    for face in self.faces {
      let mut polygon = [Vec3::ZERO; 6];
      polygon[..3].copy_from_slice(&face);
      let mut count = 3;
      for (normal, offset) in [(axis, -axis.dot(a)), (-axis, axis.dot(b))] {
        let mut clipped = [Vec3::ZERO; 6];
        let mut n = 0;
        if count == 0 {
          break;
        }
        let mut previous = polygon[count - 1];
        let mut before = normal.dot(previous) + offset;
        for &point in &polygon[..count] {
          let distance = normal.dot(point) + offset;
          if (before >= 0.) != (distance >= 0.) {
            clipped[n] = previous.lerp(point, before / (before - distance));
            n += 1;
          }
          if distance >= 0. {
            clipped[n] = point;
            n += 1;
          }
          previous = point;
          before = distance;
        }
        polygon = clipped;
        count = n;
      }
      for i in 1..count.saturating_sub(1) {
        let point = closest_segment_triangle(a, b, [polygon[0], polygon[i], polygon[i + 1]]);
        let on_axis = a + axis * ((point - a).dot(axis) / length).clamp(0., 1.);
        if on_axis.distance_squared(point) <= radius * radius {
          self.project(point, &mut visitor);
        }
      }
    }
  }
}

fn closest_triangle(p: Vec3, [a, b, c]: [Vec3; 3]) -> Vec3 {
  let ab = b - a;
  let ac = c - a;
  // Slab clipping can collapse a face to a line or repeat an endpoint.
  if ab.cross(ac).length_squared() <= f32::MIN_POSITIVE {
    let mut closest = a;
    for (start, end) in [(a, b), (b, c), (c, a)] {
      let edge = end - start;
      let length = edge.length_squared();
      let candidate = if length > f32::MIN_POSITIVE {
        start + edge * ((p - start).dot(edge) / length).clamp(0., 1.)
      } else {
        start
      };
      if candidate.distance_squared(p) < closest.distance_squared(p) {
        closest = candidate;
      }
    }
    return closest;
  }
  let ap = p - a;
  let d1 = ab.dot(ap);
  let d2 = ac.dot(ap);
  if d1 <= 0. && d2 <= 0. {
    return a;
  }
  let bp = p - b;
  let d3 = ab.dot(bp);
  let d4 = ac.dot(bp);
  if d3 >= 0. && d4 <= d3 {
    return b;
  }
  let vc = d1 * d4 - d3 * d2;
  if vc <= 0. && d1 >= 0. && d3 <= 0. {
    return a + ab * (d1 / (d1 - d3));
  }
  let cp = p - c;
  let d5 = ab.dot(cp);
  let d6 = ac.dot(cp);
  if d6 >= 0. && d5 <= d6 {
    return c;
  }
  let vb = d5 * d2 - d1 * d6;
  if vb <= 0. && d2 >= 0. && d6 <= 0. {
    return a + ac * (d2 / (d2 - d6));
  }
  let va = d3 * d6 - d5 * d4;
  if va <= 0. && d4 - d3 >= 0. && d5 - d6 >= 0. {
    return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
  }
  let sum = va + vb + vc;
  if sum.abs() <= f32::MIN_POSITIVE {
    return a;
  }
  a + ab * (vb / sum) + ac * (vc / sum)
}

fn closest_segment_triangle(a: Vec3, b: Vec3, face: [Vec3; 3]) -> Vec3 {
  let axis = b - a;
  let normal = (face[1] - face[0]).cross(face[2] - face[0]);
  let denominator = normal.dot(axis);
  if denominator.abs() > 1.0e-12 {
    let t = normal.dot(face[0] - a) / denominator;
    if (0. ..=1.).contains(&t) {
      let p = a + axis * t;
      let point = closest_triangle(p, face);
      if point.distance_squared(p) <= 1.0e-12 {
        return point;
      }
    }
  }
  let mut point = closest_triangle(a, face);
  let mut distance = point.distance_squared(a);
  let candidate = closest_triangle(b, face);
  if candidate.distance_squared(b) < distance {
    point = candidate;
    distance = candidate.distance_squared(b);
  }
  for i in 0..3 {
    let c = face[i];
    let d = face[(i + 1) % 3];
    let edge = d - c;
    let length = axis.length_squared();
    let edge_length = edge.length_squared();
    if edge_length <= 1.0e-12 {
      continue;
    }
    let offset = a - c;
    let dot = axis.dot(edge);
    let denominator = length * edge_length - dot * dot;
    let mut s = if denominator > 1.0e-12 {
      ((dot * edge.dot(offset) - axis.dot(offset) * edge_length) / denominator).clamp(0., 1.)
    } else {
      0.
    };
    let t = ((dot * s + edge.dot(offset)) / edge_length).clamp(0., 1.);
    s = ((dot * t - axis.dot(offset)) / length).clamp(0., 1.);
    let candidate = c + edge * t;
    let squared = candidate.distance_squared(a + axis * s);
    if squared < distance {
      point = candidate;
      distance = squared;
    }
  }
  point
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn collapsed_faces_should_produce_finite_closest_points() {
    let point = Vec3::new(0.5, 1., 0.);
    for face in [[Vec3::ZERO, Vec3::ZERO, Vec3::X], [Vec3::ZERO, Vec3::X, Vec3::X * 2.]] {
      assert_eq!(closest_triangle(point, face), Vec3::X * 0.5);
    }
    assert_eq!(closest_triangle(point, [Vec3::ZERO; 3]), Vec3::ZERO);
  }
}
