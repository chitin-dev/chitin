//! Typed drawing interactions retain source energy and force conventions separately.
//! Source attribution and redistribution terms are in licenses/coordinate-generation.txt.

use super::math::{EPSILON, Vector, project, same_side, segment_square};

/// No owning pointers or virtual callbacks are needed for the finite interaction set.
#[derive(Clone, Copy, Debug)]
pub(super) enum Interaction {
  Stretch {
    atoms: [usize; 2],
    target: f32,
    tolerance: f32,
    k: f32,
  },
  Bend {
    atoms: [usize; 3],
    target: f32,
    k: f32,
    k2: f32,
  },
  Clash {
    atoms: [usize; 3],
    target_square: f32,
    k: f32,
    k2: f32,
  },
  Alkene {
    atoms: [usize; 4],
    same_side: bool,
  },
}

impl Interaction {
  /// Accumulates one interaction's energy and force without moving coordinates.
  ///
  /// # Returns
  /// The source drawing penalty, not a molecular potential. Bend energy and its
  /// force use the source's different angular factors; tolerance affects stretch
  /// force but not its energy. These conventions must not be silently rederived.
  pub(super) fn score(self, points: &[Vector], forces: &mut [Vector]) -> f32 {
    match self {
      Self::Stretch {
        atoms: [a, b],
        target,
        tolerance,
        k,
      } => {
        let ray = points[a] - points[b];
        let m = ray.length();
        let distance = ray.square().sqrt();
        let energy = 0.5 * k * (distance - target) * (distance - target);
        let delta = if m < target - tolerance {
          target - tolerance - m
        } else if m > target + tolerance {
          target + tolerance - m
        } else {
          return energy;
        };
        let ray = if m > EPSILON { ray / m } else { ray };
        let force = ray * (k * delta + (target * 0.4 - m).max(0.) * 10.);
        forces[a] += force;
        forces[b] -= force;
        energy
      }
      Self::Bend {
        atoms: [a, c, b],
        target,
        k,
        k2,
      } => {
        let v1 = points[a] - points[c];
        let v2 = points[b] - points[c];
        let v3 = points[b] - points[a];
        let d = (v1.square().sqrt() * v2.square().sqrt()).max(EPSILON);
        let angle = (v1.dot(v2) / d).clamp(-1., 1.).acos() * (180. / std::f64::consts::PI) as f32;
        let error = angle - target;
        let energy = 0.5 * k * k2 * error * error * 10.;
        let effective = if target > 180. { 360. - target } else { target };
        let mut n1 = Vector::new(v1.y, -v1.x);
        let mut n2 = Vector::new(v2.y, -v2.x);
        if n1.dot(v3) > 0. {
          n1 = n1 * -1.;
        }
        if n2.dot(v3) < 0. {
          n2 = n2 * -1.;
        }
        n1 = n1 / n1.square().sqrt().max(EPSILON);
        n2 = n2 / n2.square().sqrt().max(EPSILON);
        let scale = k * k2 * (effective - angle.abs());
        let f1 = n1 * scale;
        let f2 = n2 * scale;
        forces[a] += f1;
        forces[b] += f2;
        forces[c] -= f1 + f2;
        energy
      }
      Self::Clash {
        atoms: [a, p, b],
        target_square,
        k,
        k2,
      } => {
        let distance = segment_square(points[p], points[a], points[b]);
        if distance > target_square {
          return 0.;
        }
        let delta = target_square - distance;
        let force = (points[p] - project(points[p], points[a], points[b])).normalized() * (delta * k * k2);
        forces[p] += force;
        forces[a] -= force * 0.5;
        forces[b] -= force * 0.5;
        0.5 * k * k2 * delta
      }
      Self::Alkene {
        atoms: [ra, a, b, rb],
        same_side: expected,
      } => {
        if same_side(points[ra], points[rb], points[a], points[b]) == expected {
          return 0.;
        }
        let p1 = project(points[ra], points[a], points[b]);
        let p2 = project(points[rb], points[a], points[b]);
        let (side, axis, projection) = if (points[ra] - p1).square() > (points[rb] - p2).square() {
          (rb, b, p2)
        } else {
          (ra, a, p1)
        };
        let force = (projection - points[side]).normalized() * 10.;
        forces[side] += force;
        forces[axis] -= force;
        5000.
      }
    }
  }
}
