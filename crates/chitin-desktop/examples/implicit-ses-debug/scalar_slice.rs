//! GPU rendering for planar slices through the precomputed SES fields.

use std::sync::Arc;

use chitin_bio::surface::ScalarFieldGrid;
use wgpu::util::DeviceExt;

pub(super) const SCALAR_SLICE_SHADER: &str = include_str!("scalar_slice.wgsl");

/// Example-local overlay renderer for a colored planar scalar-field slice.
pub(super) struct ScalarSliceRenderer {
  /// Pipeline with per-vertex RGBA interpolation and alpha blending.
  pipeline: wgpu::RenderPipeline,
  /// Camera/model transform consumed by the vertex shader.
  uniform_buffer: wgpu::Buffer,
  /// Bind group exposing [`Self::uniform_buffer`].
  bind_group: wgpu::BindGroup,
  /// Fixed unit-square coordinates expanded into the selected slice by WGSL.
  vertex_buffer: wgpu::Buffer,
  /// Triangles covering the regular slice grid.
  index_buffer: wgpu::Buffer,
  /// Number of indices submitted for the slice.
  index_count: u32,
  /// Device shared with the GPUI WGPU surface.
  device: Arc<wgpu::Device>,
  /// Queue shared with the molecule renderer.
  queue: Arc<wgpu::Queue>,
  /// Minimum molecular-space corner of the uploaded scalar grid.
  bounds_min: [f32; 3],
  /// Maximum molecular-space corner of the uploaded scalar grid.
  bounds_max: [f32; 3],
  /// Sample counts along the uploaded grid axes.
  dimensions: [usize; 3],
  /// Keeps the sampled three-dimensional field alive for its bound texture view.
  _volume_texture: wgpu::Texture,
}

impl ScalarSliceRenderer {
  /// Creates the alpha-blended slice pipeline and uploads the scalar volume.
  ///
  /// # Parameters
  ///
  /// * `device` creates the pipeline and immutable geometry buffers.
  /// * `queue` updates the per-frame transformation matrix.
  /// * `format` is the GPUI surface color format.
  /// * `volume` supplies the precomputed field and spatial metadata.
  ///
  /// # Returns
  ///
  /// A renderer ready to overlay any XY slice on a molecule frame.
  pub(super) fn new(
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    format: wgpu::TextureFormat,
    volume: &ScalarFieldGrid,
  ) -> Self {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
      label: Some("ses_debug_scalar_slice_shader"),
      source: wgpu::ShaderSource::Wgsl(SCALAR_SLICE_SHADER.into()),
    });
    let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
      label: Some("ses_debug_scalar_slice_uniform"),
      size: 112,
      usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
      mapped_at_creation: false,
    });
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
      label: Some("ses_debug_scalar_slice_bind_group_layout"),
      entries: &[
        wgpu::BindGroupLayoutEntry {
          binding: 0,
          visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
          ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
          },
          count: None,
        },
        wgpu::BindGroupLayoutEntry {
          binding: 1,
          visibility: wgpu::ShaderStages::FRAGMENT,
          ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: false },
            view_dimension: wgpu::TextureViewDimension::D3,
            multisampled: false,
          },
          count: None,
        },
      ],
    });
    let volume_texture = device.create_texture(&wgpu::TextureDescriptor {
      label: Some("ses_debug_scalar_volume"),
      size: wgpu::Extent3d {
        width: volume.dimensions[0] as u32,
        height: volume.dimensions[1] as u32,
        depth_or_array_layers: volume.dimensions[2] as u32,
      },
      mip_level_count: 1,
      sample_count: 1,
      dimension: wgpu::TextureDimension::D3,
      format: wgpu::TextureFormat::R32Float,
      usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
      view_formats: &[],
    });
    queue.write_texture(
      wgpu::TexelCopyTextureInfo {
        texture: &volume_texture,
        mip_level: 0,
        origin: wgpu::Origin3d::ZERO,
        aspect: wgpu::TextureAspect::All,
      },
      bytemuck::cast_slice(&volume.values),
      wgpu::TexelCopyBufferLayout {
        offset: 0,
        bytes_per_row: Some(volume.dimensions[0] as u32 * 4),
        rows_per_image: Some(volume.dimensions[1] as u32),
      },
      wgpu::Extent3d {
        width: volume.dimensions[0] as u32,
        height: volume.dimensions[1] as u32,
        depth_or_array_layers: volume.dimensions[2] as u32,
      },
    );
    let volume_view = volume_texture.create_view(&wgpu::TextureViewDescriptor {
      label: Some("ses_debug_scalar_volume_view"),
      dimension: Some(wgpu::TextureViewDimension::D3),
      ..Default::default()
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
      label: Some("ses_debug_scalar_slice_bind_group"),
      layout: &bind_group_layout,
      entries: &[
        wgpu::BindGroupEntry {
          binding: 0,
          resource: uniform_buffer.as_entire_binding(),
        },
        wgpu::BindGroupEntry {
          binding: 1,
          resource: wgpu::BindingResource::TextureView(&volume_view),
        },
      ],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
      label: Some("ses_debug_scalar_slice_pipeline_layout"),
      bind_group_layouts: &[Some(&bind_group_layout)],
      immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
      label: Some("ses_debug_scalar_slice_pipeline"),
      layout: Some(&pipeline_layout),
      vertex: wgpu::VertexState {
        module: &shader,
        entry_point: Some("vertex_main"),
        buffers: &[Some(wgpu::VertexBufferLayout {
          array_stride: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
          step_mode: wgpu::VertexStepMode::Vertex,
          attributes: &[wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x2,
            offset: 0,
            shader_location: 0,
          }],
        })],
        compilation_options: Default::default(),
      },
      fragment: Some(wgpu::FragmentState {
        module: &shader,
        entry_point: Some("fragment_main"),
        targets: &[Some(wgpu::ColorTargetState {
          format,
          blend: Some(wgpu::BlendState::ALPHA_BLENDING),
          write_mask: wgpu::ColorWrites::ALL,
        })],
        compilation_options: Default::default(),
      }),
      primitive: wgpu::PrimitiveState {
        topology: wgpu::PrimitiveTopology::TriangleList,
        cull_mode: None,
        ..Default::default()
      },
      depth_stencil: None,
      multisample: wgpu::MultisampleState::default(),
      multiview_mask: None,
      cache: None,
    });
    let vertices = [[0.0_f32, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let indices = [0_u32, 1, 2, 0, 2, 3];
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
      label: Some("ses_debug_scalar_slice_vertices"),
      contents: bytemuck::cast_slice(&vertices),
      usage: wgpu::BufferUsages::VERTEX,
    });
    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
      label: Some("ses_debug_scalar_slice_indices"),
      contents: bytemuck::cast_slice(&indices),
      usage: wgpu::BufferUsages::INDEX,
    });
    let bounds_max = volume.bounds_max();
    Self {
      pipeline,
      uniform_buffer,
      bind_group,
      vertex_buffer,
      index_buffer,
      index_count: indices.len() as u32,
      device,
      queue,
      bounds_min: volume.bounds_min,
      bounds_max,
      dimensions: volume.dimensions,
      _volume_texture: volume_texture,
    }
  }

  /// Overlays the scalar slice without clearing the molecule color target.
  pub(super) fn render(&self, view: &wgpu::TextureView, mvp: glam::Mat4, slice_fraction: f32) -> wgpu::SubmissionIndex {
    let mut uniform = [0.0_f32; 28];
    uniform[0..16].copy_from_slice(&mvp.to_cols_array());
    uniform[16..20].copy_from_slice(&[self.bounds_min[0], self.bounds_min[1], self.bounds_min[2], 0.0]);
    uniform[20..24].copy_from_slice(&[self.bounds_max[0], self.bounds_max[1], self.bounds_max[2], 0.0]);
    uniform[24..28].copy_from_slice(&[
      slice_fraction.clamp(0.0, 1.0),
      self.dimensions[0] as f32,
      self.dimensions[1] as f32,
      self.dimensions[2] as f32,
    ]);
    self
      .queue
      .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&uniform));
    let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
      label: Some("ses_debug_scalar_slice_encoder"),
    });
    {
      let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("ses_debug_scalar_slice_pass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
          view,
          resolve_target: None,
          depth_slice: None,
          ops: wgpu::Operations {
            load: wgpu::LoadOp::Load,
            store: wgpu::StoreOp::Store,
          },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
      });
      pass.set_pipeline(&self.pipeline);
      pass.set_bind_group(0, &self.bind_group, &[]);
      pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
      pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
      pass.draw_indexed(0..self.index_count, 0, 0..1);
    }
    self.queue.submit(std::iter::once(encoder.finish()))
  }
}
