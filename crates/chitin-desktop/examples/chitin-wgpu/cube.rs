//! Example cube renderer used only by the desktop WGPU integration example.

use std::sync::Arc;

use chitin_desktop::wgpu_panel::{WgpuPanelFrame, WgpuPanelScene};
use chitin_wgpu::{DepthTarget, RenderTargetSize};
use wgpu::util::DeviceExt;

/// WGSL shader compiled by the example cube render pipeline.
const SHADER: &str = include_str!("cube.wgsl");

/// A colored cube mesh with duplicated vertices per face.
///
/// Each vertex is `[x, y, z, r, g, b]`. This is intentionally example-local
/// scene data; production structure rendering should live in a renderer crate.
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

/// Scene adapter that lets the shared GPUI WGPU panel host the example cube.
pub struct ExampleCubeScene {
  /// Lazy renderer created once GPUI supplies a concrete WGPU surface.
  renderer: Option<ExampleCubeRenderer>,
}

impl ExampleCubeScene {
  /// Creates a lazy example cube scene.
  ///
  /// # Parameters
  ///
  /// This function takes no parameters.
  ///
  /// # Returns
  ///
  /// A scene that allocates GPU resources on its first rendered frame.
  pub fn new() -> Self {
    Self { renderer: None }
  }
}

impl WgpuPanelScene for ExampleCubeScene {
  /// Renders one animated cube frame into the shared WGPU panel target.
  ///
  /// # Parameters
  ///
  /// * `frame` contains the current surface target and camera state.
  ///
  /// # Returns
  ///
  /// The queue submission index used by the panel to present the frame.
  fn render_frame(&mut self, frame: WgpuPanelFrame<'_>) -> wgpu::SubmissionIndex {
    let renderer = self.renderer.get_or_insert_with(|| {
      ExampleCubeRenderer::new(
        Arc::new(frame.device.clone()),
        Arc::new(frame.queue.clone()),
        frame.size,
        frame.format,
      )
    });

    renderer.resize_if_needed(frame.size);
    let mvp = (frame.camera.view_projection(renderer.aspect()) * spinning_cube_model(frame.elapsed.as_secs_f32()))
      .to_cols_array_2d();
    renderer.render_mvp(frame.view, mvp)
  }

  /// Returns the cube-specific interaction hint.
  ///
  /// # Parameters
  ///
  /// This function takes no parameters.
  ///
  /// # Returns
  ///
  /// Static overlay text for the example scene.
  fn interaction_hint(&self) -> &'static str {
    "Example cube | L-drag rotate | Shift-L/M-drag pan | R-drag/wheel zoom"
  }
}

/// Renderer for the example cube scene.
pub struct ExampleCubeRenderer {
  /// Render pipeline compiled from the example WGSL shader.
  pipeline: wgpu::RenderPipeline,
  /// Vertex buffer for the static cube mesh.
  vertex_buffer: wgpu::Buffer,
  /// Index buffer for drawing the cube faces.
  index_buffer: wgpu::Buffer,
  /// Uniform buffer containing the current model-view-projection matrix.
  uniform_buffer: wgpu::Buffer,
  /// Bind group exposing `uniform_buffer` to the vertex shader.
  bind_group: wgpu::BindGroup,
  /// Size-dependent depth target.
  depth: DepthTarget,
  /// Shared WGPU device cloned from the GPUI surface handle.
  device: Arc<wgpu::Device>,
  /// Shared WGPU queue cloned from the GPUI surface handle.
  queue: Arc<wgpu::Queue>,
  /// Current render target size in physical pixels.
  size: RenderTargetSize,
}

impl ExampleCubeRenderer {
  /// Creates GPU resources for the example cube.
  ///
  /// # Parameters
  ///
  /// * `device` creates WGPU resources and command encoders.
  /// * `queue` submits command buffers.
  /// * `size` is the initial render target size in physical pixels.
  /// * `color_format` is the render target texture format.
  ///
  /// # Returns
  ///
  /// A renderer ready to draw into compatible WGPU texture views.
  pub fn new(
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    size: RenderTargetSize,
    color_format: wgpu::TextureFormat,
  ) -> Self {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
      label: Some("chitin_wgpu_desktop_example_cube_shader"),
      source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
      label: Some("chitin_wgpu_desktop_example_cube_uniforms"),
      size: 64,
      usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
      mapped_at_creation: false,
    });
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
      label: Some("chitin_wgpu_desktop_example_cube_bind_group_layout"),
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
      label: Some("chitin_wgpu_desktop_example_cube_bind_group"),
      layout: &bind_group_layout,
      entries: &[wgpu::BindGroupEntry {
        binding: 0,
        resource: uniform_buffer.as_entire_binding(),
      }],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
      label: Some("chitin_wgpu_desktop_example_cube_pipeline_layout"),
      bind_group_layouts: &[Some(&bind_group_layout)],
      immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
      label: Some("chitin_wgpu_desktop_example_cube_pipeline"),
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
      label: Some("chitin_wgpu_desktop_example_cube_vertices"),
      contents: bytemuck::cast_slice(VERTICES),
      usage: wgpu::BufferUsages::VERTEX,
    });
    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
      label: Some("chitin_wgpu_desktop_example_cube_indices"),
      contents: bytemuck::cast_slice(INDICES),
      usage: wgpu::BufferUsages::INDEX,
    });
    let depth = DepthTarget::new(&device, size);

    Self {
      pipeline,
      vertex_buffer,
      index_buffer,
      uniform_buffer,
      bind_group,
      depth,
      device,
      queue,
      size,
    }
  }

  /// Recreates size-dependent resources when the target size changes.
  ///
  /// # Parameters
  ///
  /// * `size` is the latest render target size in physical pixels.
  ///
  /// # Returns
  ///
  /// This function returns `()` after refreshing depth resources when needed.
  pub fn resize_if_needed(&mut self, size: RenderTargetSize) {
    self.depth.resize_if_needed(&self.device, size);
    self.size = size;
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
    self.size.aspect()
  }

  /// Draws one example cube frame.
  ///
  /// # Parameters
  ///
  /// * `view` is the WGPU color render target.
  /// * `mvp` transforms cube vertices into WGPU clip space.
  ///
  /// # Returns
  ///
  /// The queue submission index for synchronized presentation by the caller.
  pub fn render_mvp(&mut self, view: &wgpu::TextureView, mvp: [[f32; 4]; 4]) -> wgpu::SubmissionIndex {
    self
      .queue
      .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&mvp));

    let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
      label: Some("chitin_wgpu_desktop_example_cube_encoder"),
    });
    {
      let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("chitin_wgpu_desktop_example_cube_pass"),
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
          view: self.depth.view(),
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
}

/// Builds a model matrix for the example cube animation.
///
/// # Parameters
///
/// * `elapsed_seconds` is the animation time in seconds.
///
/// # Returns
///
/// A model matrix that rotates the cube in scene space.
pub fn spinning_cube_model(elapsed_seconds: f32) -> glam::Mat4 {
  glam::Mat4::from_rotation_y(elapsed_seconds * 0.7) * glam::Mat4::from_rotation_x(elapsed_seconds * 0.45)
}
