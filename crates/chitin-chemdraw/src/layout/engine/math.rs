//! Single-precision drawing-space arithmetic, distinct from normalized scene geometry.
//! Source attribution and redistribution terms are in licenses/coordinate-generation.txt.

/// Drawing simulation uses fifty units per nominal chemical bond.
pub(super) const BOND: f32 = 50.;
pub(super) const EPSILON: f32 = 0.0001;

/// Assignment-compatible quantization, including negative half-way values.
pub(super) fn quantize(value: f32) -> f32 {
  ((f64::from(value * 100.) + 0.5).floor() * 0.01) as f32
}

/// Temporary simulation coordinates are not physical three-dimensional positions.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Vector {
  pub x: f32,
  pub y: f32,
}

impl Vector {
  pub(super) const fn new(x: f32, y: f32) -> Self {
    Self { x, y }
  }
  pub(super) fn dot(self, rhs: Self) -> f32 {
    self.x * rhs.x + self.y * rhs.y
  }
  pub(super) fn square(self) -> f32 {
    self.dot(self)
  }
  pub(super) fn length(self) -> f32 {
    let d = self.square();
    if d > EPSILON { d.sqrt() } else { 0. }
  }
  pub(super) fn normalized(self) -> Self {
    let l = self.length();
    if l > EPSILON { self / l } else { self }
  }
  /// Matches coordinate assignment's two-decimal quantization, including negative ties.
  pub(super) fn rounded(self) -> Self {
    Self::new(quantize(self.x), quantize(self.y))
  }
  /// Positive angles follow the source's clockwise drawing-space convention.
  pub(super) fn rotate(self, sine: f32, cosine: f32) -> Self {
    Self::new(self.x * cosine + self.y * sine, -self.x * sine + self.y * cosine)
  }
  pub(super) fn finite(self) -> bool {
    self.x.is_finite() && self.y.is_finite()
  }
}

impl std::ops::Add for Vector {
  type Output = Self;
  fn add(self, b: Self) -> Self {
    Self::new(self.x + b.x, self.y + b.y)
  }
}
impl std::ops::Sub for Vector {
  type Output = Self;
  fn sub(self, b: Self) -> Self {
    Self::new(self.x - b.x, self.y - b.y)
  }
}
impl std::ops::Mul<f32> for Vector {
  type Output = Self;
  fn mul(self, b: f32) -> Self {
    Self::new(self.x * b, self.y * b)
  }
}
impl std::ops::Div<f32> for Vector {
  type Output = Self;
  fn div(self, b: f32) -> Self {
    Self::new(self.x / b, self.y / b)
  }
}
impl std::ops::AddAssign for Vector {
  fn add_assign(&mut self, b: Self) {
    *self = *self + b;
  }
}
impl std::ops::SubAssign for Vector {
  fn sub_assign(&mut self, b: Self) {
    *self = *self - b;
  }
}

/// Projects onto the infinite line; clash distance separately uses the segment.
pub(super) fn project(p: Vector, a: Vector, b: Vector) -> Vector {
  let axis = b - a;
  a + axis * ((p - a).dot(axis) / axis.square().max(EPSILON))
}

pub(super) fn segment_square(p: Vector, a: Vector, b: Vector) -> f32 {
  let axis = b - a;
  let t = (p - a).dot(axis) / axis.square().max(EPSILON);
  if t < 0. {
    (p - a).square()
  } else if t > 1. {
    (b - p).square()
  } else {
    ((a + axis * t) - p).square()
  }
}

/// Uses the numerically dominant axis, including the source's strict side test.
pub(super) fn same_side(p: Vector, q: Vector, a: Vector, b: Vector) -> bool {
  let axis = b - a;
  if axis.x.abs() > axis.y.abs() {
    let m = axis.y / axis.x;
    ((p.y - a.y) - m * (p.x - a.x)) * ((q.y - a.y) - m * (q.x - a.x)) > 0.
  } else {
    let m = axis.x / axis.y;
    ((p.x - a.x) - m * (p.y - a.y)) * ((q.x - a.x) - m * (q.y - a.y)) > 0.
  }
}
