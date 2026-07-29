//! Shared WGPU support code for the Chitin desktop examples.

// This module is compiled separately into each example target. The minimal
// example uses the renderer and spin matrix, while the interactive example also
// uses the camera controls, so dead-code warnings would otherwise alternate.
#![allow(dead_code)]

use std::sync::Arc;

use gpui::{Pixels, Point};
use wgpu::util::DeviceExt;

/// Initial backing texture width used before GPUI lays out the panel.
pub const INITIAL_SURFACE_WIDTH: u32 = 960;
/// Initial backing texture height used before GPUI lays out the panel.
pub const INITIAL_SURFACE_HEIGHT: u32 = 540;
/// Logical window width for the standalone examples.
pub const WINDOW_WIDTH: f32 = 1180.0;
/// Logical window height for the standalone examples.
pub const WINDOW_HEIGHT: f32 = 760.0;
/// WGSL shader compiled by the example render pipeline.
const SHADER: &str = include_str!("../shaders/chitin_wgpu_cube.wgsl");
/// Minimum camera distance from the molecular scene center.
const MIN_CAMERA_DISTANCE: f32 = 0.9;
/// Maximum camera distance from the molecular scene center.
const MAX_CAMERA_DISTANCE: f32 = 12.0;
/// Rotation scale in radians per logical pixel.
const ROTATE_RADIANS_PER_PIXEL: f32 = 0.008;
/// Pan scale relative to the current camera distance.
const PAN_UNITS_PER_PIXEL_AT_UNIT_DISTANCE: f32 = 0.0018;
/// Wheel zoom scale in exponent units per logical pixel.
const ZOOM_EXPONENT_PER_PIXEL: f32 = 0.0025;

/// A colored cube mesh with duplicated vertices per face.
///
/// Each vertex is `[x, y, z, r, g, b]`. Duplicating vertices keeps the example
/// simple: each face can carry its own color without introducing normals,
/// materials, or a molecule-specific mesh format yet.
#[rustfmt::skip]
const VERTICES: &[[f32; 6]] = &[
  [-0.5, -0.5,  0.5, 0.95, 0.22, 0.22],
  [ 0.5, -0.5,  0.5, 0.95, 0.22, 0.22],
  [ 0.5,  0.5,  0.5, 1.00, 0.56, 0.56],
  [-0.5,  0.5,  0.5, 1.00, 0.56, 0.56],
  [ 0.5, -0.5, -0.5, 0.18, 0.78, 0.36],
  [-0.5, -0.5, -0.5, 0.18, 0.78, 0.36],
  [-0.5,  0.5, -0.5, 0.50, 1.00, 0.64],
  [ 0.5,  0.5, -0.5, 0.50, 1.00, 0.64],
  [-0.5, -0.5, -0.5, 0.24, 0.38, 0.95],
  [-0.5, -0.5,  0.5, 0.24, 0.38, 0.95],
  [-0.5,  0.5,  0.5, 0.55, 0.68, 1.00],
  [-0.5,  0.5, -0.5, 0.55, 0.68, 1.00],
  [ 0.5, -0.5,  0.5, 0.95, 0.82, 0.20],
  [ 0.5, -0.5, -0.5, 0.95, 0.82, 0.20],
  [ 0.5,  0.5, -0.5, 1.00, 0.94, 0.55],
  [ 0.5,  0.5,  0.5, 1.00, 0.94, 0.55],
  [-0.5,  0.5,  0.5, 0.20, 0.86, 0.88],
  [ 0.5,  0.5,  0.5, 0.20, 0.86, 0.88],
  [ 0.5,  0.5, -0.5, 0.55, 1.00, 1.00],
  [-0.5,  0.5, -0.5, 0.55, 1.00, 1.00],
  [-0.5, -0.5, -0.5, 0.88, 0.28, 0.88],
  [ 0.5, -0.5, -0.5, 0.88, 0.28, 0.88],
  [ 0.5, -0.5,  0.5, 1.00, 0.55, 1.00],
  [-0.5, -0.5,  0.5, 1.00, 0.55, 1.00],
];

/// Cube index buffer expressed as two triangles per face.
#[rustfmt::skip]
const INDICES: &[u16] = &[
   0,  1,  2,   0,  2,  3,
   4,  5,  6,   4,  6,  7,
   8,  9, 10,   8, 10, 11,
  12, 13, 14,  12, 14, 15,
  16, 17, 18,  16, 18, 19,
  20, 21, 22,  20, 22, 23,
];

/// Minimal renderer state for drawing one cube into a GPUI-owned WGPU surface.
pub struct CubeRenderer {
  /// Render pipeline compiled from `shaders/chitin_wgpu_cube.wgsl`.
  pipeline: wgpu::RenderPipeline,
  /// Vertex buffer for the static cube mesh.
  vertex_buffer: wgpu::Buffer,
  /// Index buffer for drawing the cube faces.
  index_buffer: wgpu::Buffer,
  /// Uniform buffer containing the current model-view-projection matrix.
  uniform_buffer: wgpu::Buffer,
  /// Bind group exposing `uniform_buffer` to the vertex shader.
  bind_group: wgpu::BindGroup,
  /// Depth target matching the current surface size.
  depth_view: wgpu::TextureView,
  /// Shared WGPU device cloned from the GPUI surface handle.
  device: Arc<wgpu::Device>,
  /// Shared WGPU queue cloned from the GPUI surface handle.
  queue: Arc<wgpu::Queue>,
  /// Current render target width in physical pixels.
  width: u32,
  /// Current render target height in physical pixels.
  height: u32,
}

impl CubeRenderer {
  /// Creates GPU buffers, pipeline state, and depth resources.
  ///
  /// # Parameters
  ///
  /// `device` and `queue` come from `WgpuSurfaceHandle`; using the same device
  /// avoids any cross-device texture sharing.
  ///
  /// `width` and `height` are the initial physical pixel size of the surface.
  ///
  /// `color_format` is the texture format selected by the GPUI surface.
  ///
  /// # Returns
  ///
  /// A renderer ready to draw into a matching `WgpuSurfaceHandle` back buffer.
  pub fn new(
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    width: u32,
    height: u32,
    color_format: wgpu::TextureFormat,
  ) -> Self {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
      label: Some("chitin_wgpu_cube_shader"),
      source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
      label: Some("chitin_wgpu_cube_uniforms"),
      size: 64,
      usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
      mapped_at_creation: false,
    });
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
      label: Some("chitin_wgpu_cube_bind_group_layout"),
      entries: &[wgpu::BindGroupLayoutEntry {
        binding: 0,
        visibility: wgpu::ShaderStages::VERTEX,
        ty: wgpu::BindingType::Buffer {
          ty: wgpu::BufferBindingType::Uniform,
          has_dynamic_offset: false,
          min_binding_size: None,
        },
        count: None,
      }],
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
      label: Some("chitin_wgpu_cube_bind_group"),
      layout: &bind_group_layout,
      entries: &[wgpu::BindGroupEntry {
        binding: 0,
        resource: uniform_buffer.as_entire_binding(),
      }],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
      label: Some("chitin_wgpu_cube_pipeline_layout"),
      bind_group_layouts: &[Some(&bind_group_layout)],
      immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
      label: Some("chitin_wgpu_cube_pipeline"),
      layout: Some(&pipeline_layout),
      vertex: wgpu::VertexState {
        module: &shader,
        entry_point: Some("vs_main"),
        buffers: &[Some(wgpu::VertexBufferLayout {
          array_stride: 24,
          step_mode: wgpu::VertexStepMode::Vertex,
          attributes: &[
            wgpu::VertexAttribute {
              offset: 0,
              shader_location: 0,
              format: wgpu::VertexFormat::Float32x3,
            },
            wgpu::VertexAttribute {
              offset: 12,
              shader_location: 1,
              format: wgpu::VertexFormat::Float32x3,
            },
          ],
        })],
        compilation_options: Default::default(),
      },
      fragment: Some(wgpu::FragmentState {
        module: &shader,
        entry_point: Some("fs_main"),
        targets: &[Some(wgpu::ColorTargetState {
          format: color_format,
          blend: None,
          write_mask: wgpu::ColorWrites::ALL,
        })],
        compilation_options: Default::default(),
      }),
      primitive: wgpu::PrimitiveState {
        topology: wgpu::PrimitiveTopology::TriangleList,
        front_face: wgpu::FrontFace::Ccw,
        cull_mode: Some(wgpu::Face::Back),
        ..Default::default()
      },
      depth_stencil: Some(wgpu::DepthStencilState {
        format: wgpu::TextureFormat::Depth32Float,
        depth_write_enabled: Some(true),
        depth_compare: Some(wgpu::CompareFunction::Less),
        stencil: Default::default(),
        bias: Default::default(),
      }),
      multisample: wgpu::MultisampleState::default(),
      multiview_mask: None,
      cache: None,
    });
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
      label: Some("chitin_wgpu_cube_vertices"),
      contents: bytemuck::cast_slice(VERTICES),
      usage: wgpu::BufferUsages::VERTEX,
    });
    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
      label: Some("chitin_wgpu_cube_indices"),
      contents: bytemuck::cast_slice(INDICES),
      usage: wgpu::BufferUsages::INDEX,
    });
    let depth_view = Self::depth_view(&device, width, height);

    Self {
      pipeline,
      vertex_buffer,
      index_buffer,
      uniform_buffer,
      bind_group,
      depth_view,
      device,
      queue,
      width,
      height,
    }
  }

  /// Recreates size-dependent GPU resources when the surface size changed.
  ///
  /// # Parameters
  ///
  /// `width` and `height` are the physical pixel dimensions reported by
  /// `WgpuSurfaceHandle::back_view_with_size`.
  ///
  /// # Returns
  ///
  /// This function returns `()` after refreshing the depth texture if needed.
  pub fn resize_if_needed(&mut self, width: u32, height: u32) {
    if self.width == width && self.height == height {
      return;
    }

    self.width = width;
    self.height = height;
    self.depth_view = Self::depth_view(&self.device, width, height);
  }

  /// Returns the current render target aspect ratio.
  ///
  /// # Parameters
  ///
  /// This function takes no parameters.
  ///
  /// # Returns
  ///
  /// The width divided by height, with a non-zero height guard.
  pub fn aspect(&self) -> f32 {
    self.width as f32 / self.height.max(1) as f32
  }

  /// Draws one cube frame using a caller-provided model-view-projection matrix.
  ///
  /// # Parameters
  ///
  /// `view` is the back-buffer view obtained from `WgpuSurfaceHandle`.
  ///
  /// `mvp` transforms cube vertices into WGPU clip space.
  ///
  /// # Returns
  ///
  /// The queue submission index passed back to the surface for synchronized
  /// presentation.
  pub fn render_mvp(
    &mut self,
    view: &wgpu::TextureView,
    mvp: [[f32; 4]; 4],
  ) -> wgpu::SubmissionIndex {
    self
      .queue
      .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&mvp));

    let mut encoder = self
      .device
      .create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("chitin_wgpu_cube_encoder"),
      });
    {
      let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("chitin_wgpu_cube_pass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
          view,
          resolve_target: None,
          depth_slice: None,
          ops: wgpu::Operations {
            load: wgpu::LoadOp::Clear(wgpu::Color {
              r: 0.025,
              g: 0.030,
              b: 0.045,
              a: 1.0,
            }),
            store: wgpu::StoreOp::Store,
          },
        })],
        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
          view: &self.depth_view,
          depth_ops: Some(wgpu::Operations {
            load: wgpu::LoadOp::Clear(1.0),
            store: wgpu::StoreOp::Discard,
          }),
          stencil_ops: None,
        }),
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
      });
      pass.set_pipeline(&self.pipeline);
      pass.set_bind_group(0, &self.bind_group, &[]);
      pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
      pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
      pass.draw_indexed(0..INDICES.len() as u32, 0, 0..1);
    }

    self.queue.submit(std::iter::once(encoder.finish()))
  }

  /// Creates a depth texture view for the current surface size.
  ///
  /// # Parameters
  ///
  /// `device` creates the texture. `width` and `height` are physical pixels.
  ///
  /// # Returns
  ///
  /// A `Depth32Float` texture view suitable for one render pass.
  fn depth_view(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
      label: Some("chitin_wgpu_cube_depth"),
      size: wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
      },
      mip_level_count: 1,
      sample_count: 1,
      dimension: wgpu::TextureDimension::D2,
      format: wgpu::TextureFormat::Depth32Float,
      usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
      view_formats: &[],
    });

    texture.create_view(&wgpu::TextureViewDescriptor::default())
  }
}

/// Camera controls modeled after common molecular viewers.
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
  /// `aspect` is the render target width divided by height.
  ///
  /// # Returns
  ///
  /// A matrix that transforms scene-space positions into WGPU clip space.
  pub fn view_projection(&self, aspect: f32) -> glam::Mat4 {
    let projection = glam::camera::rh::proj::directx::perspective(0.70, aspect, 0.1, 100.0);
    projection * self.view_matrix()
  }

  /// Applies an orbit rotation from a drag delta.
  ///
  /// # Parameters
  ///
  /// `delta_x` and `delta_y` are logical pixel deltas from GPUI mouse events.
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
  /// `delta_x` and `delta_y` are logical pixel deltas from GPUI mouse events.
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

  /// Applies wheel or right-drag zoom using an exponential scale.
  ///
  /// # Parameters
  ///
  /// `delta_y` is the vertical input delta in logical pixels.
  ///
  /// # Returns
  ///
  /// This function returns `()` after clamping the camera distance.
  pub fn zoom_pixels(&mut self, delta_y: f32) {
    self.distance = (self.distance * (delta_y * ZOOM_EXPONENT_PER_PIXEL).exp())
      .clamp(MIN_CAMERA_DISTANCE, MAX_CAMERA_DISTANCE);
  }

  /// Restores the default molecular viewer orientation.
  ///
  /// # Parameters
  ///
  /// This function takes no parameters.
  ///
  /// # Returns
  ///
  /// This function returns `()` after replacing the current camera state.
  pub fn reset(&mut self) {
    *self = Self::default();
  }

  /// Returns the current camera view matrix.
  ///
  /// # Parameters
  ///
  /// This function takes no parameters.
  ///
  /// # Returns
  ///
  /// A right-handed view matrix looking at the current target.
  fn view_matrix(&self) -> glam::Mat4 {
    glam::camera::rh::view::look_at_mat4(self.eye(), self.target, glam::Vec3::Y)
  }

  /// Returns the current eye position in scene space.
  ///
  /// # Parameters
  ///
  /// This function takes no parameters.
  ///
  /// # Returns
  ///
  /// A scene-space camera position derived from orbit angles and distance.
  fn eye(&self) -> glam::Vec3 {
    let (sin_yaw, cos_yaw) = self.yaw.sin_cos();
    let (sin_pitch, cos_pitch) = self.pitch.sin_cos();
    self.target
      + glam::Vec3::new(sin_yaw * cos_pitch, sin_pitch, cos_yaw * cos_pitch) * self.distance
  }

  /// Returns the camera-right direction used for panning.
  ///
  /// # Parameters
  ///
  /// This function takes no parameters.
  ///
  /// # Returns
  ///
  /// A normalized scene-space right vector.
  fn view_right(&self) -> glam::Vec3 {
    glam::Vec3::Y
      .cross((self.target - self.eye()).normalize())
      .normalize()
  }

  /// Returns the camera-up direction used for panning.
  ///
  /// # Parameters
  ///
  /// This function takes no parameters.
  ///
  /// # Returns
  ///
  /// A normalized scene-space up vector.
  fn view_up(&self) -> glam::Vec3 {
    (self.target - self.eye())
      .normalize()
      .cross(self.view_right())
      .normalize()
  }
}

/// Interaction mode active for the current drag.
#[derive(Clone, Copy)]
pub enum DragMode {
  /// Orbit the scene around the camera target.
  Rotate,
  /// Translate the camera target parallel to the screen.
  Pan,
  /// Move the camera toward or away from the scene target.
  Zoom,
}

/// Tracks the currently active molecular-viewer drag.
#[derive(Clone, Copy)]
pub struct ViewportDrag {
  /// Operation chosen when the mouse button was pressed.
  pub mode: DragMode,
  /// Last mouse position consumed by the camera.
  pub last_position: Point<Pixels>,
}

/// Builds the default auto-spinning cube MVP used by the minimal example.
///
/// # Parameters
///
/// `elapsed_seconds` is the time since the example started.
///
/// `aspect` is the current render target width divided by height.
///
/// # Returns
///
/// A model-view-projection matrix for the animated cube.
pub fn spinning_cube_mvp(elapsed_seconds: f32, aspect: f32) -> [[f32; 4]; 4] {
  let projection = glam::camera::rh::proj::directx::perspective(0.70, aspect, 0.1, 100.0);
  let camera = glam::camera::rh::view::look_at_mat4(
    glam::Vec3::new(0.0, 0.8, 2.8),
    glam::Vec3::ZERO,
    glam::Vec3::Y,
  );
  let model = glam::Mat4::from_rotation_y(elapsed_seconds * 1.15)
    * glam::Mat4::from_rotation_x(elapsed_seconds * 0.55);

  (projection * camera * model).to_cols_array_2d()
}

/// Converts GPUI logical pixels to plain `f32` values.
///
/// # Parameters
///
/// `pixels` is a GPUI pixel value from an input event.
///
/// # Returns
///
/// The underlying scalar value.
pub fn pixels_to_f32(pixels: Pixels) -> f32 {
  f32::from(pixels)
}
