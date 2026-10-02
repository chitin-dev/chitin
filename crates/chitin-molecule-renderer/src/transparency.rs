//! Weighted blended order-independent transparency; opaque depth remains authoritative.

use chitin_wgpu::RenderTargetSize;

pub(crate) fn targets() -> [Option<wgpu::ColorTargetState>; 2] {
  let additive = wgpu::BlendComponent {
    src_factor: wgpu::BlendFactor::One,
    dst_factor: wgpu::BlendFactor::One,
    operation: wgpu::BlendOperation::Add,
  };
  let reveal = wgpu::BlendComponent {
    src_factor: wgpu::BlendFactor::Zero,
    dst_factor: wgpu::BlendFactor::OneMinusSrc,
    operation: wgpu::BlendOperation::Add,
  };
  [
    Some(wgpu::ColorTargetState {
      format: wgpu::TextureFormat::Rgba16Float,
      blend: Some(wgpu::BlendState {
        color: additive,
        alpha: additive,
      }),
      write_mask: wgpu::ColorWrites::ALL,
    }),
    Some(wgpu::ColorTargetState {
      format: wgpu::TextureFormat::R16Float,
      blend: Some(wgpu::BlendState {
        color: reveal,
        alpha: reveal,
      }),
      write_mask: wgpu::ColorWrites::RED,
    }),
  ]
}

pub(crate) struct Transparency {
  pub accumulation: wgpu::TextureView,
  pub revealage: wgpu::TextureView,
  size: RenderTargetSize,
  layout: wgpu::BindGroupLayout,
  bind_group: wgpu::BindGroup,
  pipeline: wgpu::RenderPipeline,
}

impl Transparency {
  pub fn new(device: &wgpu::Device, size: RenderTargetSize, format: wgpu::TextureFormat) -> Self {
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
      label: Some("chitin_oit_composite_layout"),
      entries: &[0, 1].map(|binding| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
          sample_type: wgpu::TextureSampleType::Float { filterable: false },
          view_dimension: wgpu::TextureViewDimension::D2,
          multisampled: false,
        },
        count: None,
      }),
    });
    let (accumulation, revealage, bind_group) = Self::textures(device, size, &layout);
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
      label: Some("chitin_oit_composite"),
      source: wgpu::ShaderSource::Wgsl(include_str!("transparency.wgsl").into()),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
      label: Some("chitin_oit_composite_pipeline_layout"),
      bind_group_layouts: &[Some(&layout)],
      immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
      label: Some("chitin_oit_composite_pipeline"),
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
          blend: Some(wgpu::BlendState::ALPHA_BLENDING),
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
      accumulation,
      revealage,
      size,
      layout,
      bind_group,
      pipeline,
    }
  }
  fn textures(
    device: &wgpu::Device,
    size: RenderTargetSize,
    layout: &wgpu::BindGroupLayout,
  ) -> (wgpu::TextureView, wgpu::TextureView, wgpu::BindGroup) {
    let make = |format| {
      device
        .create_texture(&wgpu::TextureDescriptor {
          label: Some("chitin_oit_target"),
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
    let accumulation = make(wgpu::TextureFormat::Rgba16Float);
    let revealage = make(wgpu::TextureFormat::R16Float);
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
      label: Some("chitin_oit_composite_bind_group"),
      layout,
      entries: &[
        wgpu::BindGroupEntry {
          binding: 0,
          resource: wgpu::BindingResource::TextureView(&accumulation),
        },
        wgpu::BindGroupEntry {
          binding: 1,
          resource: wgpu::BindingResource::TextureView(&revealage),
        },
      ],
    });
    (accumulation, revealage, bind_group)
  }
  pub fn resize(&mut self, device: &wgpu::Device, size: RenderTargetSize) {
    if self.size == size {
      return;
    }
    (self.accumulation, self.revealage, self.bind_group) = Self::textures(device, size, &self.layout);
    self.size = size;
  }
  pub fn composite(&self, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
      label: Some("chitin_oit_composite_pass"),
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
