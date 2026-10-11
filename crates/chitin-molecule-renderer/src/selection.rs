//! Depth-tested selection silhouette; independent of molecular colors and opacity.

use chitin_wgpu::RenderTargetSize;
use wgpu::util::DeviceExt as _;

pub(crate) struct SelectionMask {
  pub mask: wgpu::TextureView,
  pub depth: wgpu::TextureView,
  size: RenderTargetSize,
  layout: wgpu::BindGroupLayout,
  width: wgpu::Buffer,
  bind_group: wgpu::BindGroup,
  pipeline: wgpu::RenderPipeline,
}

impl SelectionMask {
  pub fn new(device: &wgpu::Device, size: RenderTargetSize, format: wgpu::TextureFormat) -> Self {
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
      label: Some("chitin_selection_layout"),
      entries: &[
        wgpu::BindGroupLayoutEntry {
          binding: 0,
          visibility: wgpu::ShaderStages::FRAGMENT,
          ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: false },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
          },
          count: None,
        },
        wgpu::BindGroupLayoutEntry {
          binding: 1,
          visibility: wgpu::ShaderStages::FRAGMENT,
          ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
          },
          count: None,
        },
      ],
    });
    let width = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
      label: Some("chitin_selection_width"),
      contents: bytemuck::cast_slice(&[2.0_f32, 0., 0., 0.]),
      usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let (mask, depth, bind_group) = Self::textures(device, size, &layout, &width);
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
      label: Some("chitin_selection_outline"),
      source: wgpu::ShaderSource::Wgsl(include_str!("selection.wgsl").into()),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
      label: Some("chitin_selection_outline_layout"),
      bind_group_layouts: &[Some(&layout)],
      immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
      label: Some("chitin_selection_outline"),
      layout: Some(&pipeline_layout),
      vertex: wgpu::VertexState {
        module: &shader,
        entry_point: Some("vertex"),
        buffers: &[],
        compilation_options: Default::default(),
      },
      fragment: Some(wgpu::FragmentState {
        module: &shader,
        entry_point: Some("fragment"),
        targets: &[Some(wgpu::ColorTargetState {
          format,
          blend: None,
          write_mask: wgpu::ColorWrites::ALL,
        })],
        compilation_options: Default::default(),
      }),
      primitive: Default::default(),
      depth_stencil: None,
      multisample: Default::default(),
      multiview_mask: None,
      cache: None,
    });
    Self {
      mask,
      depth,
      size,
      layout,
      width,
      bind_group,
      pipeline,
    }
  }

  fn textures(
    device: &wgpu::Device,
    size: RenderTargetSize,
    layout: &wgpu::BindGroupLayout,
    width: &wgpu::Buffer,
  ) -> (wgpu::TextureView, wgpu::TextureView, wgpu::BindGroup) {
    let make = |format| {
      device
        .create_texture(&wgpu::TextureDescriptor {
          label: Some("chitin_selection_target"),
          size: wgpu::Extent3d {
            width: size.width,
            height: size.height,
            depth_or_array_layers: 1,
          },
          mip_level_count: 1,
          sample_count: 1,
          dimension: wgpu::TextureDimension::D2,
          format,
          usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
          view_formats: &[],
        })
        .create_view(&Default::default())
    };
    let mask = make(wgpu::TextureFormat::Rgba8Unorm);
    let depth = make(wgpu::TextureFormat::Depth32Float);
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
      label: Some("chitin_selection_outline_group"),
      layout,
      entries: &[
        wgpu::BindGroupEntry {
          binding: 0,
          resource: wgpu::BindingResource::TextureView(&mask),
        },
        wgpu::BindGroupEntry {
          binding: 1,
          resource: width.as_entire_binding(),
        },
      ],
    });
    (mask, depth, bind_group)
  }

  pub fn resize(&mut self, device: &wgpu::Device, size: RenderTargetSize) {
    if size != self.size {
      (self.mask, self.depth, self.bind_group) = Self::textures(device, size, &self.layout, &self.width);
      self.size = size;
    }
  }

  pub fn composite(
    &self,
    encoder: &mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    queue: &wgpu::Queue,
    scale: f32,
  ) {
    queue.write_buffer(
      &self.width,
      0,
      bytemuck::cast_slice(&[2. * scale.clamp(0.5, 4.), 0., 0., 0.]),
    );
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
      label: Some("chitin_selection_outline_pass"),
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
    pass.draw(0..3, 0..1);
  }
}

#[cfg(test)]
mod tests {
  #[test]
  fn selection_outline_shader_should_parse_and_validate() -> Result<(), Box<dyn std::error::Error>> {
    let module = wgpu::naga::front::wgsl::parse_str(include_str!("selection.wgsl"))?;
    wgpu::naga::valid::Validator::new(
      wgpu::naga::valid::ValidationFlags::all(),
      wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)?;
    Ok(())
  }
}
