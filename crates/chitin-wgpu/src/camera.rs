//! Framework-neutral orbit camera controls for WGPU viewports.

/// Minimum camera distance from the scene center.
const MIN_CAMERA_DISTANCE: f32 = 0.9;
/// Maximum camera distance from the scene center.
const MAX_CAMERA_DISTANCE: f32 = 12.0;
/// Rotation scale in radians per logical pixel.
const ROTATE_RADIANS_PER_PIXEL: f32 = 0.008;
/// Pan scale relative to the current camera distance.
const PAN_UNITS_PER_PIXEL_AT_UNIT_DISTANCE: f32 = 0.0018;
/// Wheel zoom scale in exponent units per logical pixel.
const ZOOM_EXPONENT_PER_PIXEL: f32 = 0.0025;

/// Camera controls for a generic orbit/pan/zoom 3D viewport.
#[derive(Clone, Copy)]
pub struct ViewerCamera {
  /// Horizontal orbit angle in radians.
  yaw: f32,
  /// Vertical orbit angle in radians.
  pitch: f32,
  /// Distance from the camera eye to the scene target.
  distance: f32,
  /// Scene-space point at the center of the orbit.
  target: glam::Vec3,
}

impl Default for ViewerCamera {
  fn default() -> Self {
    Self {
      yaw: 0.45,
      pitch: 0.28,
      distance: 3.2,
      target: glam::Vec3::ZERO,
    }
  }
}

impl ViewerCamera {
  /// Builds the combined projection and view matrix for the current camera.
  ///
  /// # Parameters
  ///
  /// * `aspect` is the render target width divided by height.
  ///
  /// # Returns
  ///
  /// A matrix that transforms scene-space positions into WGPU clip space.
  pub fn view_projection(&self, aspect: f32) -> glam::Mat4 {
    self.projection_matrix(aspect) * self.view_matrix()
  }

  /// Builds the perspective projection matrix for the current viewport.
  ///
  /// # Parameters
  ///
  /// * `aspect` is the render target width divided by height.
  ///
  /// # Returns
  ///
  /// A right-handed projection matrix using WGPU's zero-to-one depth range.
  pub fn projection_matrix(&self, aspect: f32) -> glam::Mat4 {
    glam::camera::rh::proj::directx::perspective(0.70, aspect, 0.1, 100.0)
  }

  /// Applies an orbit rotation from a drag delta.
  ///
  /// # Parameters
  ///
  /// * `delta_x` and `delta_y` are logical pixel deltas from UI input events.
  ///
  /// # Returns
  ///
  /// This function returns `()` after updating yaw and pitch.
  pub fn rotate_pixels(&mut self, delta_x: f32, delta_y: f32) {
    self.yaw -= delta_x * ROTATE_RADIANS_PER_PIXEL;
    self.pitch = (self.pitch - delta_y * ROTATE_RADIANS_PER_PIXEL).clamp(-1.45, 1.45);
  }

  /// Applies a screen-space pan from a drag delta.
  ///
  /// # Parameters
  ///
  /// * `delta_x` and `delta_y` are logical pixel deltas from UI input events.
  ///
  /// # Returns
  ///
  /// This function returns `()` after moving the camera target.
  pub fn pan_pixels(&mut self, delta_x: f32, delta_y: f32) {
    let right = self.view_right();
    let up = self.view_up();
    let scale = self.distance * PAN_UNITS_PER_PIXEL_AT_UNIT_DISTANCE;
    self.target += (-right * delta_x + up * delta_y) * scale;
  }

  /// Applies wheel or drag zoom using an exponential scale.
  ///
  /// # Parameters
  ///
  /// * `delta_y` is the vertical input delta in logical pixels.
  ///
  /// # Returns
  ///
  /// This function returns `()` after clamping the camera distance.
  pub fn zoom_pixels(&mut self, delta_y: f32) {
    self.distance =
      (self.distance * (delta_y * ZOOM_EXPONENT_PER_PIXEL).exp()).clamp(MIN_CAMERA_DISTANCE, MAX_CAMERA_DISTANCE);
  }

  /// Restores the default viewport orientation.
  pub fn reset(&mut self) {
    *self = Self::default();
  }

  /// Returns the current camera view matrix.
  pub fn view_matrix(&self) -> glam::Mat4 {
    glam::camera::rh::view::look_at_mat4(self.eye(), self.target, glam::Vec3::Y)
  }

  /// Returns the current eye position in scene space.
  fn eye(&self) -> glam::Vec3 {
    let (sin_yaw, cos_yaw) = self.yaw.sin_cos();
    let (sin_pitch, cos_pitch) = self.pitch.sin_cos();
    self.target + glam::Vec3::new(sin_yaw * cos_pitch, sin_pitch, cos_yaw * cos_pitch) * self.distance
  }

  /// Returns the camera-right direction used for panning.
  fn view_right(&self) -> glam::Vec3 {
    glam::Vec3::Y.cross((self.target - self.eye()).normalize()).normalize()
  }

  /// Returns the camera-up direction used for panning.
  fn view_up(&self) -> glam::Vec3 {
    (self.target - self.eye())
      .normalize()
      .cross(self.view_right())
      .normalize()
  }
}

/// Interaction mode active for the current viewport drag.
#[derive(Clone, Copy)]
pub enum DragMode {
  /// Orbit the scene around the camera target.
  Rotate,
  /// Translate the camera target parallel to the screen.
  Pan,
  /// Move the camera toward or away from the scene target.
  Zoom,
}

/// Tracks the currently active viewport drag in framework-neutral units.
#[derive(Clone, Copy)]
pub struct ViewportDrag {
  /// Operation chosen when the mouse button was pressed.
  pub mode: DragMode,
  /// Last horizontal pointer position consumed by the camera.
  pub last_x: f32,
  /// Last vertical pointer position consumed by the camera.
  pub last_y: f32,
}
