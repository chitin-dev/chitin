//! Instanced atom and bond rendering for molecular structure scenes.

use std::sync::Arc;

use chitin_bio::structure::{ElementCategory, StructureScene};
use wgpu::util::DeviceExt;

use crate::{DepthTarget, RenderTargetSize};

/// WGSL shader shared by the atom and bond pipelines.
const SHADER: &str = include_str!("molecule.wgsl");
/// Default ChimeraX-inspired stick cylinder radius in source ångström units.
const DEFAULT_STICK_RADIUS: f32 = 0.20;
/// Fraction of the normalized scene radius reserved for the fitted structure.
const FIT_RADIUS: f32 = 0.90;
/// Number of rendered frames between camera-dependent diagnostic samples.
const DIAGNOSTIC_FRAME_INTERVAL: u64 = 120;
/// Maximum number of atom centers sampled by lightweight diagnostics.
const DIAGNOSTIC_POSITION_LIMIT: usize = 256;
/// Atom-radius multiplier for the reduced balls used by ball-and-stick mode.
///
/// Sphere mode uses the full element radius below. Ball-and-stick deliberately
/// scales those physical radii down so the bonds remain the dominant visual
/// structure while the atom centers are still visible.
const DEFAULT_BALL_RADIUS_SCALE: f32 = 0.25;
/// Approximate van der Waals radius for hydrogen, in ångströms.
const HYDROGEN_VDW_RADIUS: f32 = 1.20;
/// Approximate van der Waals radius for carbon, in ångströms.
const CARBON_VDW_RADIUS: f32 = 1.70;
/// Approximate van der Waals radius for nitrogen, in ångströms.
const NITROGEN_VDW_RADIUS: f32 = 1.55;
/// Approximate van der Waals radius for oxygen, in ångströms.
const OXYGEN_VDW_RADIUS: f32 = 1.52;
/// Approximate van der Waals radius for phosphorus, in ångströms.
const PHOSPHORUS_VDW_RADIUS: f32 = 1.80;
/// Approximate van der Waals radius for sulfur, in ångströms.
const SULFUR_VDW_RADIUS: f32 = 1.80;
/// Representative van der Waals radius for halogens, in ångströms.
const HALOGEN_VDW_RADIUS: f32 = 1.75;
/// Representative van der Waals radius for metals, in ångströms.
const METAL_VDW_RADIUS: f32 = 1.70;
/// Fallback van der Waals radius for unclassified elements, in ångströms.
const OTHER_VDW_RADIUS: f32 = 1.50;
/// Byte size of the molecule uniform block shared by both pipelines.
const UNIFORM_BUFFER_SIZE: u64 = 272;
/// Byte offset at which the lighting and material vectors begin.
const MATERIAL_UNIFORM_OFFSET: u64 = 192;
/// Byte offset of the per-frame absolute depth-cue vector.
const DEPTH_CUE_UNIFORM_OFFSET: u64 = 240;

/// Geometry-specific pipeline settings shared by atom and bond pipelines.
struct PipelineConfig {
  /// Target color format.
  color_format: wgpu::TextureFormat,
  /// Mesh and instance vertex layout.
  instance_layout: wgpu::VertexBufferLayout<'static>,
  /// Analytic fragment entry point for this surface type.
  fragment_entry: &'static str,
}

/// Shader output used to isolate one stage of molecule shading.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MoleculeDebugMode {
  /// Renders the complete material, lighting, and depth-cue result.
  #[default]
  Final = 0,
  /// Encodes scene-space normals as RGB.
  Normal = 1,
  /// Displays only the primary diffuse-light factor.
  KeyDiffuse = 2,
  /// Displays only the secondary diffuse-light factor.
  FillDiffuse = 3,
  /// Displays only the specular-light factor.
  Specular = 4,
  /// Displays the depth-cue blend factor as grayscale.
  DepthCue = 5,
  /// Displays element colors without lighting or depth cue.
  ElementColor = 6,
}

/// Atom-level representation used by the molecular renderer.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum AtomRepresentation {
  /// Draw atoms with the bond radius and show bonds.
  #[default]
  Stick,
  /// Draw larger element-colored atoms together with bonds.
  BallAndStick,
  /// Draw element-colored atoms without bonds.
  Sphere,
}

impl AtomRepresentation {
  /// Parses a command-line representation name.
  pub fn from_name(name: &str) -> Option<Self> {
    match name.trim().to_ascii_lowercase().as_str() {
      "stick" => Some(Self::Stick),
      "ball-and-stick" | "ball_and_stick" | "ballandstick" => Some(Self::BallAndStick),
      "sphere" | "spheres" => Some(Self::Sphere),
      _ => None,
    }
  }
}

impl MoleculeDebugMode {
  /// Parses a diagnostic mode name used by the desktop example environment.
  pub fn from_name(name: &str) -> Option<Self> {
    match name.trim().to_ascii_lowercase().as_str() {
      "final" => Some(Self::Final),
      "normal" => Some(Self::Normal),
      "key-diffuse" | "key_diffuse" => Some(Self::KeyDiffuse),
      "fill-diffuse" | "fill_diffuse" => Some(Self::FillDiffuse),
      "specular" => Some(Self::Specular),
      "depth-cue" | "depth_cue" => Some(Self::DepthCue),
      "element-color" | "element_color" => Some(Self::ElementColor),
      _ => None,
    }
  }

  /// Returns the numeric value consumed by the WGSL shader.
  fn shader_value(self) -> f32 {
    u8::from(self) as f32
  }
}

impl From<MoleculeDebugMode> for u8 {
  fn from(mode: MoleculeDebugMode) -> Self {
    mode as Self
  }
}

/// CPU-side state used for low-frequency renderer diagnostics.
struct MoleculeDiagnostics {
  /// Number of frames observed by the renderer.
  frame_count: u64,
  /// Representative source-space atom centers.
  sample_positions: Vec<glam::Vec3>,
  /// Depth-cue start, end, and maximum strength.
  depth_cue: [f32; 3],
}

/// Radius and linear RGB color for one ball-and-stick element family.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BallAndStickElementStyle {
  /// Sphere radius in source ångström units.
  pub radius: f32,
  /// Linear RGB surface color.
  pub color: [f32; 3],
}

/// Element appearance mapping used by the ball-and-stick representation.
#[derive(Debug, Clone, PartialEq)]
pub struct BallAndStickPalette {
  /// Hydrogen appearance.
  pub hydrogen: BallAndStickElementStyle,
  /// Carbon appearance.
  pub carbon: BallAndStickElementStyle,
  /// Nitrogen appearance.
  pub nitrogen: BallAndStickElementStyle,
  /// Oxygen appearance.
  pub oxygen: BallAndStickElementStyle,
  /// Phosphorus appearance.
  pub phosphorus: BallAndStickElementStyle,
  /// Sulfur appearance.
  pub sulfur: BallAndStickElementStyle,
  /// Halogen appearance.
  pub halogen: BallAndStickElementStyle,
  /// Metal appearance.
  pub metal: BallAndStickElementStyle,
  /// Missing or unclassified element appearance.
  pub other: BallAndStickElementStyle,
}

impl BallAndStickPalette {
  /// Returns the visual style assigned to an element family.
  pub fn for_element(&self, element: ElementCategory) -> BallAndStickElementStyle {
    match element {
      ElementCategory::Hydrogen => self.hydrogen,
      ElementCategory::Carbon => self.carbon,
      ElementCategory::Nitrogen => self.nitrogen,
      ElementCategory::Oxygen => self.oxygen,
      ElementCategory::Phosphorus => self.phosphorus,
      ElementCategory::Sulfur => self.sulfur,
      ElementCategory::Halogen => self.halogen,
      ElementCategory::Metal => self.metal,
      ElementCategory::Other => self.other,
    }
  }
}

/// Diffuse and specular lighting parameters for ball-and-stick surfaces.
#[derive(Debug, Clone, PartialEq)]
pub struct BallAndStickMaterial {
  /// Direction in which the primary light travels in camera space.
  pub key_light_direction: [f32; 3],
  /// Strength of the primary diffuse light.
  pub key_light_strength: f32,
  /// Direction in which the secondary light travels in camera space.
  pub fill_light_direction: [f32; 3],
  /// Strength of the secondary diffuse light.
  pub fill_light_strength: f32,
  /// Fraction of directional light reflected diffusely.
  pub diffuse_reflectivity: f32,
  /// Direction-independent surface illumination.
  pub ambient_strength: f32,
  /// Strength of the soft white specular highlight.
  pub specular_strength: f32,
  /// Specular exponent controlling highlight size.
  pub shininess: f32,
}

/// Central visual configuration for the ball-and-stick representation.
///
/// This type intentionally owns only presentation values. It does not alter
/// molecular topology, inferred bonds, camera placement, or selection state.
#[derive(Debug, Clone, PartialEq)]
pub struct BallAndStickStyle {
  /// Element-specific van der Waals sphere radii and CPK-inspired colors.
  pub palette: BallAndStickPalette,
  /// Cylinder radius shared by source and inferred bonds, in ångströms.
  pub bond_radius: f32,
  /// Multiplier applied to palette radii in ball-and-stick mode.
  pub ball_radius_scale: f32,
  /// Black-background clear color expressed as linear RGB components.
  pub background_color: [f64; 3],
  /// Fraction of normalized depth at which surfaces begin fading to the background.
  pub depth_cue_start: f32,
  /// Fraction of normalized depth at which surfaces fully reach the background.
  pub depth_cue_end: f32,
  /// Maximum blend toward the background used by the depth cue.
  pub depth_cue_strength: f32,
  /// Non-metallic diffuse and specular material parameters.
  pub material: BallAndStickMaterial,
}

impl Default for BallAndStickStyle {
  fn default() -> Self {
    Self {
      palette: BallAndStickPalette {
        hydrogen: BallAndStickElementStyle {
          radius: HYDROGEN_VDW_RADIUS,
          color: [0.94, 0.94, 0.92],
        },
        carbon: BallAndStickElementStyle {
          radius: CARBON_VDW_RADIUS,
          color: [0.76, 0.71, 0.62],
        },
        nitrogen: BallAndStickElementStyle {
          radius: NITROGEN_VDW_RADIUS,
          color: [0.12, 0.30, 0.95],
        },
        oxygen: BallAndStickElementStyle {
          radius: OXYGEN_VDW_RADIUS,
          color: [0.95, 0.10, 0.08],
        },
        phosphorus: BallAndStickElementStyle {
          radius: PHOSPHORUS_VDW_RADIUS,
          color: [0.98, 0.46, 0.08],
        },
        sulfur: BallAndStickElementStyle {
          radius: SULFUR_VDW_RADIUS,
          color: [0.98, 0.78, 0.06],
        },
        halogen: BallAndStickElementStyle {
          radius: HALOGEN_VDW_RADIUS,
          color: [0.18, 0.82, 0.28],
        },
        metal: BallAndStickElementStyle {
          radius: METAL_VDW_RADIUS,
          color: [0.60, 0.48, 0.86],
        },
        other: BallAndStickElementStyle {
          radius: OTHER_VDW_RADIUS,
          color: [0.82, 0.42, 0.72],
        },
      },
      bond_radius: DEFAULT_STICK_RADIUS,
      ball_radius_scale: DEFAULT_BALL_RADIUS_SCALE,
      background_color: [0.003, 0.003, 0.005],
      depth_cue_start: 0.25,
      depth_cue_end: 1.0,
      depth_cue_strength: 0.18,
      material: BallAndStickMaterial {
        key_light_direction: [0.577, -0.577, -0.577],
        key_light_strength: 0.93,
        fill_light_direction: [-0.2, -0.2, -0.959],
        fill_light_strength: 0.26,
        diffuse_reflectivity: 0.58,
        ambient_strength: 0.46,
        specular_strength: 0.10,
        shininess: 28.0,
      },
    }
  }
}

/// Instanced renderer for atoms and source or inferred scene bonds.
///
/// One shared billboard quad is uploaded per renderer. Fragment shaders solve
/// analytic sphere and cylinder intersections and write their exact depth.
pub struct MoleculeRenderer {
  /// Pipeline for instanced atom spheres.
  atom_pipeline: wgpu::RenderPipeline,
  /// Pipeline for instanced bond cylinders.
  bond_pipeline: wgpu::RenderPipeline,
  /// Shared billboard vertices used by both analytic surface pipelines.
  quad_vertex_buffer: wgpu::Buffer,
  /// Shared billboard triangle indices.
  quad_index_buffer: wgpu::Buffer,
  /// Number of indices in `quad_index_buffer`.
  quad_index_count: u32,
  /// Per-atom position, radius, and color data.
  atom_instance_buffer: wgpu::Buffer,
  /// Number of atom instances to draw.
  atom_count: u32,
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
  /// Source-space center used to derive the visible linear depth interval.
  scene_center: glam::Vec3,
  /// Radius of the fitted scene used to derive the visible depth interval.
  fitted_radius: f32,
  /// Clear color selected by the representation style.
  background_color: wgpu::Color,
  /// Size-dependent depth target.
  depth: DepthTarget,
  /// Shared WGPU device used to resize and encode frames.
  device: Arc<wgpu::Device>,
  /// Shared WGPU queue used to update uniforms and submit frames.
  queue: Arc<wgpu::Queue>,
  /// Current render target size in physical pixels.
  size: RenderTargetSize,
  /// Shader output selected for visual diagnostics.
  debug_mode: MoleculeDebugMode,
  /// Low-frequency numeric diagnostics for the current scene.
  diagnostics: MoleculeDiagnostics,
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
    Self::new_with_representation(
      device,
      queue,
      size,
      color_format,
      scene,
      AtomRepresentation::default(),
      &BallAndStickStyle::default(),
    )
  }

  /// Creates renderer resources with a caller-selected ball-and-stick style.
  ///
  /// # Parameters
  ///
  /// * `device` creates WGPU buffers, shaders, and pipelines.
  /// * `queue` updates uniforms and submits encoded frames.
  /// * `size` is the initial render target size in physical pixels.
  /// * `color_format` is the UI-owned target texture format.
  /// * `scene` supplies renderer-neutral atom, bond, and bounds data.
  /// * `style` supplies atom radii, bond thickness, colors, and material light.
  ///
  /// # Returns
  ///
  /// A renderer ready to draw `scene` with the requested visual parameters.
  pub fn new_with_style(
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    size: RenderTargetSize,
    color_format: wgpu::TextureFormat,
    scene: &StructureScene,
    style: &BallAndStickStyle,
  ) -> Self {
    Self::new_with_representation(
      device,
      queue,
      size,
      color_format,
      scene,
      AtomRepresentation::default(),
      style,
    )
  }

  /// Creates renderer resources for an atom-level representation and style.
  ///
  /// # Parameters
  ///
  /// * `device` creates WGPU buffers, shaders, and pipelines.
  /// * `queue` updates uniforms and submits encoded frames.
  /// * `size` is the initial render target size in physical pixels.
  /// * `color_format` is the UI-owned target texture format.
  /// * `scene` supplies renderer-neutral atom, bond, and bounds data.
  /// * `representation` selects stick, ball-and-stick, or sphere rendering.
  /// * `style` supplies atom radii, bond thickness, colors, and material light.
  ///
  /// # Returns
  ///
  /// A renderer ready to draw `scene` with the requested representation.
  pub fn new_with_representation(
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    size: RenderTargetSize,
    color_format: wgpu::TextureFormat,
    scene: &StructureScene,
    representation: AtomRepresentation,
    style: &BallAndStickStyle,
  ) -> Self {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
      label: Some("chitin_molecule_shader"),
      source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
      label: Some("chitin_molecule_uniforms"),
      size: UNIFORM_BUFFER_SIZE,
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
      PipelineConfig {
        color_format,
        instance_layout: atom_instance_layout(),
        fragment_entry: "atom_fragment",
      },
    );
    let bond_pipeline = create_pipeline(
      &device,
      &pipeline_layout,
      &shader,
      "chitin_molecule_bond_pipeline",
      "bond_vertex",
      PipelineConfig {
        color_format,
        instance_layout: bond_instance_layout(),
        fragment_entry: "bond_fragment",
      },
    );

    let (quad_vertices, quad_indices) = billboard_quad_mesh();
    let atom_instances = atom_instances(scene, style, representation);
    let bond_instances = bond_instances(scene, style, representation);
    let bond_count = bond_instances.len() as u32;
    let quad_vertex_buffer = create_buffer(
      &device,
      "chitin_molecule_billboard_vertices",
      &quad_vertices,
      wgpu::BufferUsages::VERTEX,
    );
    let quad_index_buffer = create_buffer(
      &device,
      "chitin_molecule_billboard_indices",
      &quad_indices,
      wgpu::BufferUsages::INDEX,
    );
    let atom_instance_buffer = create_buffer(
      &device,
      "chitin_molecule_atom_instances",
      &atom_instances,
      wgpu::BufferUsages::VERTEX,
    );
    // WGPU buffers cannot have a zero-byte binding range. Keep a single dummy
    // row when the scene has no bonds, while drawing zero rows.
    let bond_buffer_data = if bond_instances.is_empty() {
      vec![[0.0; 16]]
    } else {
      bond_instances
    };
    let bond_instance_buffer = create_buffer(
      &device,
      "chitin_molecule_bond_instances",
      &bond_buffer_data,
      wgpu::BufferUsages::VERTEX,
    );
    queue.write_buffer(
      &uniform_buffer,
      MATERIAL_UNIFORM_OFFSET,
      bytemuck::cast_slice(&material_uniform(style)),
    );
    let surface_radius = representation_surface_radius(scene, style, representation);
    let (fit_transform, scene_center, fitted_radius) = fit_geometry(scene, surface_radius);
    let depth = DepthTarget::new(&device, size);
    let diagnostics = MoleculeDiagnostics {
      frame_count: 0,
      sample_positions: diagnostic_positions(scene),
      depth_cue: [style.depth_cue_start, style.depth_cue_end, style.depth_cue_strength],
    };

    log::debug!(
      target: "chitin_wgpu::molecule",
      "molecule renderer initialized: representation={representation:?}, atoms={}, bonds={}, bond_radius={:.3} A, key={:?}@{:.3}, fill={:?}@{:.3}, ambient={:.3}, diffuse={:.3}, specular={:.3}, shininess={:.1}, depth_cue=[{:.3}, {:.3}]@{:.3}",
      scene.atoms.len(),
      scene.bonds.len(),
      style.bond_radius,
      style.material.key_light_direction,
      style.material.key_light_strength,
      style.material.fill_light_direction,
      style.material.fill_light_strength,
      style.material.ambient_strength,
      style.material.diffuse_reflectivity,
      style.material.specular_strength,
      style.material.shininess,
      style.depth_cue_start,
      style.depth_cue_end,
      style.depth_cue_strength,
    );

    Self {
      atom_pipeline,
      bond_pipeline,
      quad_vertex_buffer,
      quad_index_buffer,
      quad_index_count: quad_indices.len() as u32,
      atom_instance_buffer,
      atom_count: scene.atoms.len() as u32,
      bond_instance_buffer,
      bond_count,
      uniform_buffer,
      bind_group,
      fit_transform,
      scene_center,
      fitted_radius,
      background_color: wgpu::Color {
        r: style.background_color[0],
        g: style.background_color[1],
        b: style.background_color[2],
        a: 1.0,
      },
      depth,
      device,
      queue,
      size,
      debug_mode: MoleculeDebugMode::Final,
      diagnostics,
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

  /// Selects the shader output used for molecule diagnostics.
  pub fn set_debug_mode(&mut self, mode: MoleculeDebugMode) {
    if self.debug_mode == mode {
      return;
    }
    self.debug_mode = mode;
    log::info!(target: "chitin_wgpu::molecule", "molecule debug mode changed to {mode:?}");
  }

  /// Draws the molecule with camera-space lighting and linear depth cueing.
  ///
  /// # Parameters
  ///
  /// * `view` is the UI-owned WGPU color render target.
  /// * `camera_view` transforms fitted scene coordinates into camera space.
  /// * `projection` transforms camera-space coordinates into clip space.
  ///
  /// # Returns
  ///
  /// The queue submission index used by the UI surface presentation path.
  pub fn render(
    &mut self,
    view: &wgpu::TextureView,
    camera_view: glam::Mat4,
    projection: glam::Mat4,
  ) -> wgpu::SubmissionIndex {
    let model_view = camera_view * self.fit_transform;
    let mvp_matrix = projection * model_view;
    let depth_cue = self.depth_cue_uniform(model_view);
    self.log_diagnostics(model_view, depth_cue);
    let mvp = mvp_matrix.to_cols_array_2d();
    let model_view = model_view.to_cols_array_2d();
    self
      .queue
      .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&mvp));
    self
      .queue
      .write_buffer(&self.uniform_buffer, 64, bytemuck::bytes_of(&model_view));
    let projection_uniform = projection.to_cols_array_2d();
    self
      .queue
      .write_buffer(&self.uniform_buffer, 128, bytemuck::bytes_of(&projection_uniform));
    self.queue.write_buffer(
      &self.uniform_buffer,
      DEPTH_CUE_UNIFORM_OFFSET,
      bytemuck::bytes_of(&depth_cue),
    );

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
            load: wgpu::LoadOp::Clear(self.background_color),
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
        pass.set_vertex_buffer(0, self.quad_vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, self.bond_instance_buffer.slice(..));
        pass.set_index_buffer(self.quad_index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        pass.draw_indexed(0..self.quad_index_count, 0, 0..self.bond_count);
      }

      pass.set_pipeline(&self.atom_pipeline);
      pass.set_bind_group(0, &self.bind_group, &[]);
      pass.set_vertex_buffer(0, self.quad_vertex_buffer.slice(..));
      pass.set_vertex_buffer(1, self.atom_instance_buffer.slice(..));
      pass.set_index_buffer(self.quad_index_buffer.slice(..), wgpu::IndexFormat::Uint16);
      pass.draw_indexed(0..self.quad_index_count, 0, 0..self.atom_count);
    }

    self.queue.submit(std::iter::once(encoder.finish()))
  }

  /// Returns the absolute camera-space interval used by the depth cue.
  fn depth_cue_uniform(&self, model_view: glam::Mat4) -> [f32; 4] {
    let center_depth = -model_view.transform_point3(self.scene_center).z;
    let near_depth = (center_depth - self.fitted_radius).max(f32::EPSILON);
    let far_depth = (center_depth + self.fitted_radius).max(near_depth + f32::EPSILON);
    let [start_fraction, end_fraction, strength] = self.diagnostics.depth_cue;
    let [start_depth, end_depth] = linear_depth_cue_range(near_depth, far_depth, start_fraction, end_fraction);
    [start_depth, end_depth, strength, self.debug_mode.shader_value()]
  }

  /// Logs low-frequency linear depth and cue statistics for representative atoms.
  fn log_diagnostics(&mut self, model_view: glam::Mat4, depth_cue: [f32; 4]) {
    self.diagnostics.frame_count = self.diagnostics.frame_count.wrapping_add(1);
    let frame = self.diagnostics.frame_count;
    if !log::log_enabled!(target: "chitin_wgpu::molecule", log::Level::Debug)
      || (frame != 1 && !frame.is_multiple_of(DIAGNOSTIC_FRAME_INTERVAL))
      || self.diagnostics.sample_positions.is_empty()
    {
      return;
    }

    let [cue_start, cue_end, cue_strength, _] = depth_cue;
    let mut depth_min = f32::INFINITY;
    let mut depth_max = f32::NEG_INFINITY;
    let mut depth_sum = 0.0;
    let mut cue_min = f32::INFINITY;
    let mut cue_max = f32::NEG_INFINITY;
    let mut cue_sum = 0.0;

    for position in &self.diagnostics.sample_positions {
      let view_position = model_view.transform_point3(*position);
      let depth = -view_position.z;
      let cue = smoothstep(cue_start, cue_end, depth) * cue_strength;
      depth_min = depth_min.min(depth);
      depth_max = depth_max.max(depth);
      depth_sum += depth;
      cue_min = cue_min.min(cue);
      cue_max = cue_max.max(cue);
      cue_sum += cue;
    }

    let sample_count = self.diagnostics.sample_positions.len() as f32;
    log::debug!(
      target: "chitin_wgpu::molecule",
      "molecule frame diagnostics: frame={frame}, mode={:?}, samples={}, view_depth=[{depth_min:.5}, mean={:.5}, {depth_max:.5}], cue_range=[{cue_start:.5}, {cue_end:.5}], depth_cue=[{cue_min:.5}, mean={:.5}, {cue_max:.5}]",
      self.debug_mode,
      self.diagnostics.sample_positions.len(),
      depth_sum / sample_count,
      cue_sum / sample_count,
    );
  }
}

/// Evaluates the Hermite interpolation used by WGSL `smoothstep`.
fn smoothstep(edge_start: f32, edge_end: f32, value: f32) -> f32 {
  if edge_start >= edge_end {
    return f32::from(value >= edge_end);
  }
  let fraction = ((value - edge_start) / (edge_end - edge_start)).clamp(0.0, 1.0);
  fraction * fraction * (3.0 - 2.0 * fraction)
}

/// Converts fractional depth-cue settings into an ordered linear interval.
///
/// # Parameters
///
/// * `near_depth` and `far_depth` bound the fitted scene in camera space.
/// * `start_fraction` and `end_fraction` locate the cue within those bounds.
///
/// # Returns
///
/// An increasing pair of absolute positive camera-space depths.
fn linear_depth_cue_range(near_depth: f32, far_depth: f32, start_fraction: f32, end_fraction: f32) -> [f32; 2] {
  let span = (far_depth - near_depth).max(f32::EPSILON);
  let start = near_depth + span * start_fraction.clamp(0.0, 1.0);
  let end = near_depth + span * end_fraction.clamp(0.0, 1.0);
  [start, end.max(start + f32::EPSILON)]
}

/// Samples atom centers uniformly across a scene for frame diagnostics.
///
/// # Parameters
///
/// * `scene` supplies the ordered atom centers to sample.
///
/// # Returns
///
/// At most [`DIAGNOSTIC_POSITION_LIMIT`] source-space positions distributed
/// across the complete atom array.
fn diagnostic_positions(scene: &StructureScene) -> Vec<glam::Vec3> {
  let stride = scene.atoms.len().div_ceil(DIAGNOSTIC_POSITION_LIMIT).max(1);
  scene
    .atoms
    .iter()
    .step_by(stride)
    .take(DIAGNOSTIC_POSITION_LIMIT)
    .map(|atom| glam::Vec3::from_array(atom.position))
    .collect()
}

/// Creates the uniform bind group layout shared by both pipelines.
fn create_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
  device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
    label: Some("chitin_molecule_bind_group_layout"),
    entries: &[wgpu::BindGroupLayoutEntry {
      binding: 0,
      visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
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
  config: PipelineConfig,
) -> wgpu::RenderPipeline {
  device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
    label: Some(label),
    layout: Some(layout),
    vertex: wgpu::VertexState {
      module: shader,
      entry_point: Some(vertex_entry),
      buffers: &[Some(mesh_vertex_layout()), Some(config.instance_layout)],
      compilation_options: Default::default(),
    },
    fragment: Some(wgpu::FragmentState {
      module: shader,
      entry_point: Some(config.fragment_entry),
      targets: &[Some(wgpu::ColorTargetState {
        format: config.color_format,
        blend: None,
        write_mask: wgpu::ColorWrites::ALL,
      })],
      compilation_options: Default::default(),
    }),
    primitive: wgpu::PrimitiveState {
      topology: wgpu::PrimitiveTopology::TriangleList,
      front_face: wgpu::FrontFace::Ccw,
      cull_mode: None,
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
    array_stride: 64,
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
      wgpu::VertexAttribute {
        offset: 48,
        shader_location: 5,
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

/// Builds the two triangles used as an analytic-surface billboard.
fn billboard_quad_mesh() -> ([[f32; 6]; 4], [u16; 6]) {
  (
    [
      [-1.0, -1.0, 0.0, 0.0, 0.0, 0.0],
      [1.0, -1.0, 0.0, 0.0, 0.0, 0.0],
      [-1.0, 1.0, 0.0, 0.0, 0.0, 0.0],
      [1.0, 1.0, 0.0, 0.0, 0.0, 0.0],
    ],
    [0, 1, 2, 2, 1, 3],
  )
}

/// Converts scene atoms into tightly packed GPU instance rows.
fn atom_instances(
  scene: &StructureScene,
  style: &BallAndStickStyle,
  representation: AtomRepresentation,
) -> Vec<[f32; 8]> {
  let mut maximum_bond_radii = vec![0.0_f32; scene.atoms.len()];
  for bond in &scene.bonds {
    for atom_id in bond.atom_ids {
      if let Some(radius) = maximum_bond_radii.get_mut(atom_id.index()) {
        *radius = (*radius).max(style.bond_radius);
      }
    }
  }

  scene
    .atoms
    .iter()
    .enumerate()
    .map(|(index, atom)| {
      let visual = style.palette.for_element(atom.element);
      let radius = match representation {
        AtomRepresentation::Sphere => visual.radius,
        AtomRepresentation::Stick => maximum_bond_radii[index].max(style.bond_radius),
        AtomRepresentation::BallAndStick => (visual.radius * style.ball_radius_scale).max(style.bond_radius),
      };
      [
        atom.position[0],
        atom.position[1],
        atom.position[2],
        radius,
        visual.color[0],
        visual.color[1],
        visual.color[2],
        1.0,
      ]
    })
    .collect()
}

/// Converts bonds into continuous cylinders with endpoint element colors.
///
/// # Parameters
///
/// * `scene` supplies endpoint coordinates and stable atom identities.
/// * `style` supplies the shared cylinder radius and endpoint palette.
///
/// # Returns
///
/// One packed cylinder per bond. The shader selects the endpoint color on each
/// side of the midpoint without splitting the geometry.
fn bond_instances(
  scene: &StructureScene,
  style: &BallAndStickStyle,
  representation: AtomRepresentation,
) -> Vec<[f32; 16]> {
  if representation == AtomRepresentation::Sphere {
    return Vec::new();
  }
  let table_len = scene
    .atoms
    .iter()
    .map(|atom| atom.atom_id.index())
    .max()
    .map_or(0, |index| index + 1);
  let mut elements = vec![ElementCategory::Other; table_len];
  for atom in &scene.atoms {
    elements[atom.atom_id.index()] = atom.element;
  }

  let mut instances = Vec::with_capacity(scene.bonds.len());
  for bond in &scene.bonds {
    let [start, end] = bond.positions;
    let start_element = elements
      .get(bond.atom_ids[0].index())
      .copied()
      .unwrap_or(ElementCategory::Other);
    let end_element = elements
      .get(bond.atom_ids[1].index())
      .copied()
      .unwrap_or(ElementCategory::Other);
    let start_color = style.palette.for_element(start_element).color;
    let end_color = style.palette.for_element(end_element).color;
    instances.push(bond_instance(start, end, style.bond_radius, start_color, end_color));
  }
  instances
}

/// Packs one colored cylinder instance for the molecule vertex shader.
///
/// # Parameters
///
/// * `start` and `end` are Cartesian cylinder endpoints in ångströms.
/// * `radius` is the cylinder radius in ångströms.
/// * `start_color` and `end_color` are the endpoint linear RGB colors.
///
/// # Returns
///
/// A tightly packed GPU instance row matching `BondInstance` in WGSL.
fn bond_instance(start: [f32; 3], end: [f32; 3], radius: f32, start_color: [f32; 3], end_color: [f32; 3]) -> [f32; 16] {
  [
    start[0],
    start[1],
    start[2],
    radius,
    end[0],
    end[1],
    end[2],
    0.0,
    start_color[0],
    start_color[1],
    start_color[2],
    1.0,
    end_color[0],
    end_color[1],
    end_color[2],
    1.0,
  ]
}

/// Packs style-controlled lighting values after the frame matrix uniforms.
fn material_uniform(style: &BallAndStickStyle) -> [[f32; 4]; 5] {
  let material = &style.material;
  [
    [
      material.key_light_direction[0],
      material.key_light_direction[1],
      material.key_light_direction[2],
      material.key_light_strength,
    ],
    [
      material.fill_light_direction[0],
      material.fill_light_direction[1],
      material.fill_light_direction[2],
      material.fill_light_strength,
    ],
    [
      material.ambient_strength,
      material.specular_strength,
      material.shininess,
      material.diffuse_reflectivity,
    ],
    [
      style.depth_cue_start,
      style.depth_cue_end,
      style.depth_cue_strength,
      MoleculeDebugMode::Final.shader_value(),
    ],
    [
      style.background_color[0] as f32,
      style.background_color[1] as f32,
      style.background_color[2] as f32,
      1.0,
    ],
  ]
}

/// Returns the largest rendered surface radius for the selected representation.
fn representation_surface_radius(
  scene: &StructureScene,
  style: &BallAndStickStyle,
  representation: AtomRepresentation,
) -> f32 {
  scene.atoms.iter().fold(style.bond_radius, |maximum, atom| {
    let element_radius = style.palette.for_element(atom.element).radius;
    let rendered_radius = match representation {
      AtomRepresentation::Stick => style.bond_radius,
      AtomRepresentation::BallAndStick => element_radius * style.ball_radius_scale,
      AtomRepresentation::Sphere => element_radius,
    };
    maximum.max(rendered_radius)
  })
}

/// Centers source coordinates and derives fitted bounds for camera effects.
///
/// # Parameters
///
/// * `scene` supplies the source-space center and radius.
/// * `surface_radius` expands the fitted bounds to include rendered surfaces.
///
/// # Returns
///
/// The source-to-fitted transform, source center, and fitted surface radius.
fn fit_geometry(scene: &StructureScene, surface_radius: f32) -> (glam::Mat4, glam::Vec3, f32) {
  let center = glam::Vec3::from_array(scene.bounds.center());
  let source_radius = scene.bounds.radius();
  let scale = FIT_RADIUS / source_radius.max(1.0);
  let transform = glam::Mat4::from_scale(glam::Vec3::splat(scale)) * glam::Mat4::from_translation(-center);
  let fitted_radius = (source_radius + surface_radius) * scale;
  (transform, center, fitted_radius)
}

#[cfg(test)]
mod tests {
  use super::*;
  use chitin_bio::structure::{PdbParser, StructureScene};

  /// Parses a two-atom inferred bond for instance-data tests.
  fn bonded_scene(second_element: &str) -> StructureScene {
    let pdb = format!(
      "ATOM      1  C   GLY A   1       0.000   0.000   0.000  1.00 10.00           C  \nATOM      2  X   GLY A   1       1.400   0.000   0.000  1.00 10.00          {second_element:>2}  \nEND\n"
    );
    let parsed = PdbParser::new()
      .parse_bytes(pdb.as_bytes())
      .unwrap_or_else(|error| panic!("ball-and-stick fixture should parse: {error}"));
    StructureScene::from_first_model(&parsed.structure)
      .unwrap_or_else(|error| panic!("ball-and-stick fixture should produce a scene: {error}"))
  }

  #[test]
  fn billboard_indices_should_stay_inside_vertex_table() {
    let (vertices, indices) = billboard_quad_mesh();

    assert!(indices.iter().all(|index| usize::from(*index) < vertices.len()));
  }

  #[test]
  fn heteronuclear_bond_should_remain_one_continuous_instance() {
    let scene = bonded_scene("O");
    let instances = bond_instances(&scene, &BallAndStickStyle::default(), AtomRepresentation::Stick);

    assert_eq!(instances.len(), 1);
  }

  #[test]
  fn homonuclear_bond_should_remain_one_instance() {
    let scene = bonded_scene("C");
    let instances = bond_instances(&scene, &BallAndStickStyle::default(), AtomRepresentation::Stick);

    assert_eq!(instances.len(), 1);
  }

  #[test]
  fn heteronuclear_bond_should_pack_both_endpoint_colors() {
    let scene = bonded_scene("O");
    let style = BallAndStickStyle::default();
    let instances = bond_instances(&scene, &style, AtomRepresentation::Stick);
    let colors = (
      [instances[0][8], instances[0][9], instances[0][10]],
      [instances[0][12], instances[0][13], instances[0][14]],
    );

    assert_eq!(colors, (style.palette.carbon.color, style.palette.oxygen.color));
  }

  #[test]
  fn sphere_mode_should_use_element_physical_radii() {
    let style = BallAndStickStyle::default();
    assert!(style.palette.carbon.radius > style.bond_radius);
  }

  #[test]
  fn stick_atom_junctions_should_match_cylinder_radius() {
    let scene = bonded_scene("O");
    let style = BallAndStickStyle::default();

    let instances = atom_instances(&scene, &style, AtomRepresentation::Stick);

    assert_eq!(instances[0][3], style.bond_radius);
  }

  #[test]
  fn ball_and_stick_atoms_should_be_larger_than_stick_atoms() {
    let scene = bonded_scene("O");
    let style = BallAndStickStyle::default();
    let stick = atom_instances(&scene, &style, AtomRepresentation::Stick);
    let ball_and_stick = atom_instances(&scene, &style, AtomRepresentation::BallAndStick);

    assert!(ball_and_stick[0][3] > stick[0][3]);
  }

  #[test]
  fn sphere_atoms_should_be_larger_than_ball_and_stick_atoms() {
    let scene = bonded_scene("O");
    let style = BallAndStickStyle::default();
    let sphere = atom_instances(&scene, &style, AtomRepresentation::Sphere);
    let ball_and_stick = atom_instances(&scene, &style, AtomRepresentation::BallAndStick);

    assert!(sphere[0][3] > ball_and_stick[0][3]);
  }

  #[test]
  fn sphere_representation_should_hide_bonds() {
    let scene = bonded_scene("O");
    let style = BallAndStickStyle::default();

    assert!(bond_instances(&scene, &style, AtomRepresentation::Sphere).is_empty());
  }

  #[test]
  fn atom_representation_should_parse_cli_names() {
    assert_eq!(
      AtomRepresentation::from_name("ball-and-stick"),
      Some(AtomRepresentation::BallAndStick)
    );
    assert_eq!(
      AtomRepresentation::from_name("sphere"),
      Some(AtomRepresentation::Sphere)
    );
    assert_eq!(AtomRepresentation::default(), AtomRepresentation::Stick);
  }

  #[test]
  fn debug_mode_should_parse_hyphenated_name() {
    assert_eq!(
      MoleculeDebugMode::from_name("key-diffuse"),
      Some(MoleculeDebugMode::KeyDiffuse)
    );
  }

  #[test]
  fn smoothstep_should_clamp_values_outside_the_interval() {
    assert_eq!((smoothstep(0.5, 1.0, 0.25), smoothstep(0.5, 1.0, 1.25)), (0.0, 1.0));
  }

  #[test]
  fn linear_depth_cue_range_should_map_fractions_into_scene_depth() {
    assert_eq!(linear_depth_cue_range(2.0, 4.0, 0.5, 1.0), [3.0, 4.0]);
  }

  #[test]
  fn molecule_shader_should_parse_and_validate() {
    let module = wgpu::naga::front::wgsl::parse_str(SHADER)
      .unwrap_or_else(|error| panic!("molecule shader should parse: {error}"));
    let mut validator = wgpu::naga::valid::Validator::new(
      wgpu::naga::valid::ValidationFlags::all(),
      wgpu::naga::valid::Capabilities::all(),
    );

    validator
      .validate(&module)
      .unwrap_or_else(|error| panic!("molecule shader should validate: {error}"));
  }
}
