//! Instanced atom and explicit-bond rendering for molecular structure scenes.

use std::{f32::consts::PI, sync::Arc};

use chitin_bio::structure::{ElementCategory, StructureScene};
use wgpu::util::DeviceExt;

use crate::{DepthTarget, RenderTargetSize};

/// WGSL shader shared by the atom and bond pipelines.
const SHADER: &str = include_str!("molecule.wgsl");
/// Number of longitudinal segments in the low-poly atom sphere.
const SPHERE_LONGITUDES: u16 = 12;
/// Number of latitudinal segments in the low-poly atom sphere.
const SPHERE_LATITUDES: u16 = 8;
/// Number of radial segments in the bond cylinder.
const BOND_SEGMENTS: u16 = 10;

/// Instanced renderer for atoms and bonds explicitly present in a structure.
///
/// One shared sphere and cylinder mesh are uploaded per renderer. Molecular
/// coordinates, radii, and colors are stored in instance buffers so GPU memory
/// grows linearly with atom and bond counts rather than tessellated mesh size.
pub struct MoleculeRenderer {
  /// Pipeline for instanced atom spheres.
  atom_pipeline: wgpu::RenderPipeline,
  /// Pipeline for instanced bond cylinders.
  bond_pipeline: wgpu::RenderPipeline,
  /// Shared low-poly sphere vertices.
  sphere_vertex_buffer: wgpu::Buffer,
  /// Shared low-poly sphere triangle indices.
  sphere_index_buffer: wgpu::Buffer,
  /// Number of indices in `sphere_index_buffer`.
  sphere_index_count: u32,
  /// Per-atom position, radius, and color data.
  atom_instance_buffer: wgpu::Buffer,
  /// Number of atom instances to draw.
  atom_count: u32,
  /// Shared open-ended cylinder vertices.
  bond_vertex_buffer: wgpu::Buffer,
  /// Shared cylinder triangle indices.
  bond_index_buffer: wgpu::Buffer,
  /// Number of indices in `bond_index_buffer`.
  bond_index_count: u32,
  /// Per-bond endpoint, radius, and color data.
  bond_instance_buffer: wgpu::Buffer,
  /// Number of bond instances to draw.
  bond_count: u32,
  /// Uniform buffer containing the current model-view-projection matrix.
  uniform_buffer: wgpu::Buffer,
  /// Bind group exposing `uniform_buffer` to both vertex shaders.
  bind_group: wgpu::BindGroup,
  /// Transform that centers and uniformly fits source ångström coordinates.
  fit_transform: glam::Mat4,
  /// Size-dependent depth target.
  depth: DepthTarget,
  /// Shared WGPU device used to resize and encode frames.
  device: Arc<wgpu::Device>,
  /// Shared WGPU queue used to update uniforms and submit frames.
  queue: Arc<wgpu::Queue>,
  /// Current render target size in physical pixels.
  size: RenderTargetSize,
}

impl MoleculeRenderer {
  /// Creates all static mesh, instance, uniform, and pipeline resources.
  ///
  /// # Parameters
  ///
  /// * `device` creates WGPU buffers, shaders, and pipelines.
  /// * `queue` updates uniforms and submits encoded frames.
  /// * `size` is the initial render target size in physical pixels.
  /// * `color_format` is the UI-owned target texture format.
  /// * `scene` supplies renderer-neutral atom, bond, and bounds data.
  ///
  /// # Returns
  ///
  /// A renderer ready to draw `scene` into compatible texture views.
  pub fn new(
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    size: RenderTargetSize,
    color_format: wgpu::TextureFormat,
    scene: &StructureScene,
  ) -> Self {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
      label: Some("chitin_molecule_shader"),
      source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
      label: Some("chitin_molecule_uniforms"),
      size: 64,
      usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
      mapped_at_creation: false,
    });
    let bind_group_layout = create_bind_group_layout(&device);
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
      label: Some("chitin_molecule_bind_group"),
      layout: &bind_group_layout,
      entries: &[wgpu::BindGroupEntry {
        binding: 0,
        resource: uniform_buffer.as_entire_binding(),
      }],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
      label: Some("chitin_molecule_pipeline_layout"),
      bind_group_layouts: &[Some(&bind_group_layout)],
      immediate_size: 0,
    });
    let atom_pipeline = create_pipeline(
      &device,
      &pipeline_layout,
      &shader,
      "chitin_molecule_atom_pipeline",
      "atom_vertex",
      color_format,
      atom_instance_layout(),
    );
    let bond_pipeline = create_pipeline(
      &device,
      &pipeline_layout,
      &shader,
      "chitin_molecule_bond_pipeline",
      "bond_vertex",
      color_format,
      bond_instance_layout(),
    );

    let (sphere_vertices, sphere_indices) = sphere_mesh(SPHERE_LATITUDES, SPHERE_LONGITUDES);
    let (bond_vertices, bond_indices) = cylinder_mesh(BOND_SEGMENTS);
    let atom_instances = atom_instances(scene);
    let bond_instances = bond_instances(scene);
    let sphere_vertex_buffer = create_buffer(
      &device,
      "chitin_molecule_sphere_vertices",
      &sphere_vertices,
      wgpu::BufferUsages::VERTEX,
    );
    let sphere_index_buffer = create_buffer(
      &device,
      "chitin_molecule_sphere_indices",
      &sphere_indices,
      wgpu::BufferUsages::INDEX,
    );
    let atom_instance_buffer = create_buffer(
      &device,
      "chitin_molecule_atom_instances",
      &atom_instances,
      wgpu::BufferUsages::VERTEX,
    );
    let bond_vertex_buffer = create_buffer(
      &device,
      "chitin_molecule_bond_vertices",
      &bond_vertices,
      wgpu::BufferUsages::VERTEX,
    );
    let bond_index_buffer = create_buffer(
      &device,
      "chitin_molecule_bond_indices",
      &bond_indices,
      wgpu::BufferUsages::INDEX,
    );

    // WGPU buffers cannot have a zero-byte binding range. Keep a single dummy
    // row when a parser supplied no explicit bonds, while drawing zero rows.
    let bond_buffer_data = if bond_instances.is_empty() {
      vec![[0.0; 12]]
    } else {
      bond_instances
    };
    let bond_instance_buffer = create_buffer(
      &device,
      "chitin_molecule_bond_instances",
      &bond_buffer_data,
      wgpu::BufferUsages::VERTEX,
    );
    let fit_transform = fit_transform(scene);
    let depth = DepthTarget::new(&device, size);

    Self {
      atom_pipeline,
      bond_pipeline,
      sphere_vertex_buffer,
      sphere_index_buffer,
      sphere_index_count: sphere_indices.len() as u32,
      atom_instance_buffer,
      atom_count: scene.atoms.len() as u32,
      bond_vertex_buffer,
      bond_index_buffer,
      bond_index_count: bond_indices.len() as u32,
      bond_instance_buffer,
      bond_count: scene.bonds.len() as u32,
      uniform_buffer,
      bind_group,
      fit_transform,
      depth,
      device,
      queue,
      size,
    }
  }

  /// Recreates size-dependent depth resources when the target changes.
  pub fn resize_if_needed(&mut self, size: RenderTargetSize) {
    self.depth.resize_if_needed(&self.device, size);
    self.size = size;
  }

  /// Returns the current render target aspect ratio.
  pub fn aspect(&self) -> f32 {
    self.size.aspect()
  }

  /// Draws the molecule using a caller-supplied camera matrix.
  ///
  /// # Parameters
  ///
  /// * `view` is the UI-owned WGPU color render target.
  /// * `view_projection` transforms fitted scene coordinates into clip space.
  ///
  /// # Returns
  ///
  /// The queue submission index used by the UI surface presentation path.
  pub fn render(&mut self, view: &wgpu::TextureView, view_projection: glam::Mat4) -> wgpu::SubmissionIndex {
    let mvp = (view_projection * self.fit_transform).to_cols_array_2d();
    self
      .queue
      .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&mvp));

    let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
      label: Some("chitin_molecule_encoder"),
    });
    {
      let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("chitin_molecule_pass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
          view,
          resolve_target: None,
          depth_slice: None,
          ops: wgpu::Operations {
            load: wgpu::LoadOp::Clear(wgpu::Color {
              r: 0.022,
              g: 0.026,
              b: 0.036,
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

      if self.bond_count > 0 {
        pass.set_pipeline(&self.bond_pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.bond_vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, self.bond_instance_buffer.slice(..));
        pass.set_index_buffer(self.bond_index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        pass.draw_indexed(0..self.bond_index_count, 0, 0..self.bond_count);
      }

      pass.set_pipeline(&self.atom_pipeline);
      pass.set_bind_group(0, &self.bind_group, &[]);
      pass.set_vertex_buffer(0, self.sphere_vertex_buffer.slice(..));
      pass.set_vertex_buffer(1, self.atom_instance_buffer.slice(..));
      pass.set_index_buffer(self.sphere_index_buffer.slice(..), wgpu::IndexFormat::Uint16);
      pass.draw_indexed(0..self.sphere_index_count, 0, 0..self.atom_count);
    }

    self.queue.submit(std::iter::once(encoder.finish()))
  }
}

/// Creates the uniform bind group layout shared by both pipelines.
fn create_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
  device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
    label: Some("chitin_molecule_bind_group_layout"),
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
  })
}

/// Creates one triangle pipeline for a mesh and instance layout.
fn create_pipeline(
  device: &wgpu::Device,
  layout: &wgpu::PipelineLayout,
  shader: &wgpu::ShaderModule,
  label: &'static str,
  vertex_entry: &'static str,
  color_format: wgpu::TextureFormat,
  instance_layout: wgpu::VertexBufferLayout<'static>,
) -> wgpu::RenderPipeline {
  device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
    label: Some(label),
    layout: Some(layout),
    vertex: wgpu::VertexState {
      module: shader,
      entry_point: Some(vertex_entry),
      buffers: &[Some(mesh_vertex_layout()), Some(instance_layout)],
      compilation_options: Default::default(),
    },
    fragment: Some(wgpu::FragmentState {
      module: shader,
      entry_point: Some("fragment"),
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
  })
}

/// Returns the shared position-and-normal mesh vertex layout.
fn mesh_vertex_layout() -> wgpu::VertexBufferLayout<'static> {
  wgpu::VertexBufferLayout {
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
  }
}

/// Returns the atom position/radius and color instance layout.
fn atom_instance_layout() -> wgpu::VertexBufferLayout<'static> {
  wgpu::VertexBufferLayout {
    array_stride: 32,
    step_mode: wgpu::VertexStepMode::Instance,
    attributes: &[
      wgpu::VertexAttribute {
        offset: 0,
        shader_location: 2,
        format: wgpu::VertexFormat::Float32x4,
      },
      wgpu::VertexAttribute {
        offset: 16,
        shader_location: 3,
        format: wgpu::VertexFormat::Float32x4,
      },
    ],
  }
}

/// Returns the bond endpoints/radius and color instance layout.
fn bond_instance_layout() -> wgpu::VertexBufferLayout<'static> {
  wgpu::VertexBufferLayout {
    array_stride: 48,
    step_mode: wgpu::VertexStepMode::Instance,
    attributes: &[
      wgpu::VertexAttribute {
        offset: 0,
        shader_location: 2,
        format: wgpu::VertexFormat::Float32x4,
      },
      wgpu::VertexAttribute {
        offset: 16,
        shader_location: 3,
        format: wgpu::VertexFormat::Float32x4,
      },
      wgpu::VertexAttribute {
        offset: 32,
        shader_location: 4,
        format: wgpu::VertexFormat::Float32x4,
      },
    ],
  }
}

/// Uploads a typed POD slice into a WGPU buffer.
fn create_buffer<T: bytemuck::NoUninit>(
  device: &wgpu::Device,
  label: &'static str,
  contents: &[T],
  usage: wgpu::BufferUsages,
) -> wgpu::Buffer {
  device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
    label: Some(label),
    contents: bytemuck::cast_slice(contents),
    usage,
  })
}

/// Builds a low-poly unit sphere with shared seam vertices.
fn sphere_mesh(latitudes: u16, longitudes: u16) -> (Vec<[f32; 6]>, Vec<u16>) {
  let mut vertices = Vec::with_capacity(((latitudes + 1) * (longitudes + 1)) as usize);
  for latitude in 0..=latitudes {
    let theta = PI * f32::from(latitude) / f32::from(latitudes);
    let (sin_theta, cos_theta) = theta.sin_cos();
    for longitude in 0..=longitudes {
      let phi = 2.0 * PI * f32::from(longitude) / f32::from(longitudes);
      let (sin_phi, cos_phi) = phi.sin_cos();
      let position = [sin_theta * cos_phi, cos_theta, sin_theta * sin_phi];
      vertices.push([
        position[0],
        position[1],
        position[2],
        position[0],
        position[1],
        position[2],
      ]);
    }
  }

  let mut indices = Vec::with_capacity((latitudes * longitudes * 6) as usize);
  let row = longitudes + 1;
  for latitude in 0..latitudes {
    for longitude in 0..longitudes {
      let top_left = latitude * row + longitude;
      let bottom_left = top_left + row;
      indices.extend_from_slice(&[
        top_left,
        bottom_left,
        top_left + 1,
        top_left + 1,
        bottom_left,
        bottom_left + 1,
      ]);
    }
  }
  (vertices, indices)
}

/// Builds an open-ended unit cylinder aligned with the local Y axis.
fn cylinder_mesh(segments: u16) -> (Vec<[f32; 6]>, Vec<u16>) {
  let mut vertices = Vec::with_capacity(((segments + 1) * 2) as usize);
  for segment in 0..=segments {
    let angle = 2.0 * PI * f32::from(segment) / f32::from(segments);
    let (sin_angle, cos_angle) = angle.sin_cos();
    for y in [-0.5, 0.5] {
      vertices.push([cos_angle, y, sin_angle, cos_angle, 0.0, sin_angle]);
    }
  }

  let mut indices = Vec::with_capacity((segments * 6) as usize);
  for segment in 0..segments {
    let lower = segment * 2;
    indices.extend_from_slice(&[lower, lower + 1, lower + 2, lower + 2, lower + 1, lower + 3]);
  }
  (vertices, indices)
}

/// Converts scene atoms into tightly packed GPU instance rows.
fn atom_instances(scene: &StructureScene) -> Vec<[f32; 8]> {
  scene
    .atoms
    .iter()
    .map(|atom| {
      let radius = element_radius(atom.element);
      let color = element_color(atom.element);
      [
        atom.position[0],
        atom.position[1],
        atom.position[2],
        radius,
        color[0],
        color[1],
        color[2],
        1.0,
      ]
    })
    .collect()
}

/// Converts scene bonds into tightly packed GPU instance rows.
fn bond_instances(scene: &StructureScene) -> Vec<[f32; 12]> {
  scene
    .bonds
    .iter()
    .map(|bond| {
      let [start, end] = bond.positions;
      [
        start[0], start[1], start[2], 0.11, end[0], end[1], end[2], 0.0, 0.62, 0.65, 0.72, 1.0,
      ]
    })
    .collect()
}

/// Centers source coordinates and fits their enclosing bounds to unit scale.
fn fit_transform(scene: &StructureScene) -> glam::Mat4 {
  let center = glam::Vec3::from_array(scene.bounds.center());
  let radius = scene.bounds.radius().max(1.0);
  glam::Mat4::from_scale(glam::Vec3::splat(1.25 / radius)) * glam::Mat4::from_translation(-center)
}

/// Returns a compact CPK-inspired atom radius in ångströms.
fn element_radius(element: ElementCategory) -> f32 {
  match element {
    ElementCategory::Hydrogen => 0.24,
    ElementCategory::Carbon => 0.38,
    ElementCategory::Nitrogen => 0.36,
    ElementCategory::Oxygen => 0.34,
    ElementCategory::Phosphorus | ElementCategory::Sulfur => 0.43,
    ElementCategory::Halogen | ElementCategory::Metal => 0.42,
    ElementCategory::Other => 0.37,
  }
}

/// Returns a CPK-inspired linear RGB color for an element family.
fn element_color(element: ElementCategory) -> [f32; 3] {
  match element {
    ElementCategory::Hydrogen => [0.92, 0.92, 0.92],
    ElementCategory::Carbon => [0.42, 0.45, 0.50],
    ElementCategory::Nitrogen => [0.20, 0.36, 0.92],
    ElementCategory::Oxygen => [0.90, 0.18, 0.18],
    ElementCategory::Phosphorus => [0.95, 0.48, 0.10],
    ElementCategory::Sulfur => [0.92, 0.78, 0.10],
    ElementCategory::Halogen => [0.18, 0.76, 0.30],
    ElementCategory::Metal => [0.56, 0.42, 0.80],
    ElementCategory::Other => [0.78, 0.36, 0.68],
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn generated_mesh_indices_stay_inside_vertex_tables() {
    let (sphere_vertices, sphere_indices) = sphere_mesh(SPHERE_LATITUDES, SPHERE_LONGITUDES);
    let (cylinder_vertices, cylinder_indices) = cylinder_mesh(BOND_SEGMENTS);

    assert!(
      sphere_indices
        .iter()
        .all(|index| usize::from(*index) < sphere_vertices.len())
    );
    assert!(
      cylinder_indices
        .iter()
        .all(|index| usize::from(*index) < cylinder_vertices.len())
    );
  }
}
