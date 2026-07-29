#![forbid(unsafe_code)]
//! Experimental GPUI panel that renders directly into a WGPU surface.
//!
//! Run with `cargo run --example chitin-wgpu`.
//!
//! The example demonstrates the integration pattern Chitin needs for structure
//! visualization: GPUI owns the panel layout, while raw `wgpu` draws into a
//! `WgpuSurfaceHandle` texture that GPUI composites like any other element.

use std::{sync::Arc, time};

use gpui::{
  App, Application, Bounds, Context, IntoElement, Render, WgpuSurfaceHandle, Window, WindowBounds,
  WindowOptions, div, prelude::*, px, rgb, size, wgpu_surface,
};
use wgpu::util::DeviceExt;

/// Initial backing texture width used before GPUI lays out the panel.
const INITIAL_SURFACE_WIDTH: u32 = 960;
/// Initial backing texture height used before GPUI lays out the panel.
const INITIAL_SURFACE_HEIGHT: u32 = 540;
/// Logical window width for the standalone example.
const WINDOW_WIDTH: f32 = 1180.0;
/// Logical window height for the standalone example.
const WINDOW_HEIGHT: f32 = 760.0;
/// WGSL shader compiled by the example's render pipeline.
const SHADER: &str = include_str!("shaders/chitin_wgpu_cube.wgsl");

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
///
/// This type deliberately owns only GPU objects that are independent of GPUI
/// layout. The panel view owns the `WgpuSurfaceHandle` and asks this renderer
/// to draw whenever GPUI schedules a frame.
struct CubeRenderer {
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
  /// Start time used to animate the cube deterministically.
  started_at: time::Instant,
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
  fn new(
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    width: u32,
    height: u32,
    color_format: wgpu::TextureFormat,
  ) -> Self {
    // The shader is external so the Rust file teaches the integration flow
    // without hiding pipeline setup inside a long string literal.
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
      label: Some("chitin_wgpu_cube_shader"),
      source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    // A single MVP matrix is enough for this probe. A real structure viewer
    // will replace this with camera, lighting, and representation uniforms.
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
    // Keep the pipeline deliberately conventional: triangle list, back-face
    // culling, and depth testing. This is the baseline Chitin can extend.
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
      started_at: time::Instant::now(),
      width,
      height,
    }
  }

  /// Recreates size-dependent GPU resources.
  ///
  /// # Parameters
  ///
  /// `width` and `height` are the new physical pixel dimensions reported by
  /// `WgpuSurfaceHandle::back_view_with_size`.
  ///
  /// # Returns
  ///
  /// This function returns `()` after refreshing the depth texture.
  fn resize(&mut self, width: u32, height: u32) {
    self.width = width;
    self.height = height;
    self.depth_view = Self::depth_view(&self.device, width, height);
  }

  /// Draws the current cube frame into a WGPU texture view.
  ///
  /// # Parameters
  ///
  /// `view` is the back-buffer view obtained from `WgpuSurfaceHandle`.
  ///
  /// # Returns
  ///
  /// The queue submission index passed back to the surface for synchronized
  /// presentation.
  fn render(&mut self, view: &wgpu::TextureView) -> wgpu::SubmissionIndex {
    let elapsed = self.started_at.elapsed().as_secs_f32();
    let aspect = self.width as f32 / self.height.max(1) as f32;
    // WGPU uses a DirectX-style depth range, so use glam's matching helper.
    let projection = glam::camera::rh::proj::directx::perspective(0.70, aspect, 0.1, 100.0);
    let camera = glam::camera::rh::view::look_at_mat4(
      glam::Vec3::new(0.0, 0.8, 2.8),
      glam::Vec3::ZERO,
      glam::Vec3::Y,
    );
    let model =
      glam::Mat4::from_rotation_y(elapsed * 1.15) * glam::Mat4::from_rotation_x(elapsed * 0.55);
    let mvp: [[f32; 4]; 4] = (projection * camera * model).to_cols_array_2d();

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

/// GPUI view state for the experimental WGPU panel.
///
/// The panel owns the surface handle and lazily creates `CubeRenderer` after
/// GPUI has produced a real back buffer. This mirrors the future structure
/// viewer: panel state stays in GPUI, render resources stay in a renderer.
struct ChitinWgpuPanel {
  /// GPUI surface handle, absent only if the backend cannot create WGPU surfaces.
  surface: Option<WgpuSurfaceHandle>,
  /// Lazy renderer using the surface's device, queue, and color format.
  renderer: Option<CubeRenderer>,
  /// Number of frames since the last FPS update.
  frame_count: u32,
  /// Timestamp for coarse FPS reporting.
  last_fps_update: time::Instant,
  /// Last computed frames-per-second value displayed in the panel.
  display_fps: f64,
}

impl ChitinWgpuPanel {
  /// Renders one frame into the surface back buffer when available.
  ///
  /// # Parameters
  ///
  /// This method mutably borrows `self` to update renderer and FPS state.
  ///
  /// # Returns
  ///
  /// This function returns `()`. If the surface is unavailable or has no back
  /// buffer yet, it leaves the previous state unchanged.
  fn render_surface(&mut self) {
    let Some(surface) = self.surface.as_ref() else {
      return;
    };
    // Hold this guard while encoding, submitting, and presenting. It prevents
    // resize/reconfigure work from racing the queue on the shared GPUI device.
    let _submit_guard = surface.submit_guard();
    let Some((view, (width, height))) = surface.back_view_with_size() else {
      return;
    };
    let renderer = self.renderer.get_or_insert_with(|| {
      CubeRenderer::new(
        Arc::new(surface.device().clone()),
        Arc::new(surface.queue().clone()),
        width,
        height,
        surface.format(),
      )
    });

    if renderer.width != width || renderer.height != height {
      renderer.resize(width, height);
    }

    let submission_index = renderer.render(&view);
    drop(view);
    // GPUI drives the repaint below via request_animation_frame, so use the
    // silent present path and avoid scheduling an extra full-window refresh.
    surface.present_synced_silent(submission_index);

    self.frame_count = self.frame_count.wrapping_add(1);
    let now = time::Instant::now();
    if now.duration_since(self.last_fps_update) >= time::Duration::from_secs(1) {
      self.display_fps = f64::from(self.frame_count);
      self.frame_count = 0;
      self.last_fps_update = now;
    }
  }
}

impl Render for ChitinWgpuPanel {
  /// Renders the panel chrome and schedules the next WGPU frame.
  ///
  /// # Parameters
  ///
  /// `window` is used to request animation frames while the example is open.
  ///
  /// `cx` is notified so GPUI keeps repainting the panel.
  ///
  /// # Returns
  ///
  /// A GPUI element tree containing the WGPU surface and a small FPS overlay.
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    self.render_surface();
    window.request_animation_frame();
    cx.notify();

    let panel_body = match self.surface.clone() {
      Some(surface) => div()
        .relative()
        .flex_1()
        .min_h_0()
        .rounded_sm()
        .overflow_hidden()
        .border_1()
        .border_color(rgb(0x2a3140))
        .child(
          // WgpuSurface is just another GPUI element here. GPUI lays it out,
          // then composites the latest ready texture from the handle.
          wgpu_surface(surface)
            .absolute()
            .inset_0()
            .defer_resize_until_mouse_up(true),
        )
        .child(
          div()
            .absolute()
            .top(px(10.0))
            .left(px(12.0))
            .px_2()
            .py_1()
            .rounded_sm()
            .bg(gpui::rgba(0x000000a8))
            .text_xs()
            .text_color(rgb(0xd7e0f2))
            .child(format!("{:.0} fps", self.display_fps)),
        ),
      None => div()
        .flex()
        .flex_1()
        .items_center()
        .justify_center()
        .rounded_sm()
        .border_1()
        .border_color(rgb(0x3a2330))
        .bg(rgb(0x180f14))
        .text_color(rgb(0xffb4c4))
        .child("WGPU surface is not supported by this GPUI backend"),
    };

    div()
      .size_full()
      .bg(rgb(0x0b0e14))
      .text_color(rgb(0xe6edf7))
      .p_4()
      .child(
        div()
          .flex()
          .flex_col()
          .size_full()
          .gap_3()
          .child(
            div()
              .flex()
              .items_center()
              .justify_between()
              .child(
                div()
                  .text_lg()
                  .font_weight(gpui::FontWeight::SEMIBOLD)
                  .child("Chitin WGPU Panel"),
              )
              .child(
                div()
                  .text_sm()
                  .text_color(rgb(0x93a4ba))
                  .child("GPUI + WGPU surface"),
              ),
          )
          .child(panel_body),
      )
  }
}

/// Starts the standalone Chitin WGPU example.
///
/// # Parameters
///
/// This function takes no Rust parameters.
///
/// # Returns
///
/// This function returns `()` after the GPUI application exits.
fn main() {
  env_logger::init();

  Application::new().run(|cx: &mut App| {
    let bounds = Bounds::centered(None, size(px(WINDOW_WIDTH), px(WINDOW_HEIGHT)), cx);
    let result = cx.open_window(
      WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        app_id: Some("dev.chitin.ChitinWgpuExample".to_string()),
        ..WindowOptions::default()
      },
      |window: &mut Window, cx: &mut App| {
        let surface = window.create_wgpu_surface(
          INITIAL_SURFACE_WIDTH,
          INITIAL_SURFACE_HEIGHT,
          wgpu::TextureFormat::Rgba8UnormSrgb,
        );

        cx.new(|_| ChitinWgpuPanel {
          surface,
          renderer: None,
          frame_count: 0,
          last_fps_update: time::Instant::now(),
          display_fps: 0.0,
        })
      },
    );

    if let Err(error) = result {
      eprintln!("failed to open Chitin WGPU example window: {error}");
      cx.quit();
    }
  });
}
