//! Depth-tested selection silhouette; independent of molecular colors and opacity.

use chitin_bio::structure::{AtomId, ResidueId, selection::AtomSelection};
use chitin_wgpu::RenderTargetSize;
use std::collections::{HashMap, HashSet};
use wgpu::util::DeviceExt as _;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum SelectionOwner {
  Unmapped,
  Atom(AtomId),
  Bond([AtomId; 2]),
  Residue(ResidueId),
}

/// Geometry stores stable lookup indices, never a copy of the live selection.
pub(crate) struct SelectionLookup {
  owners: Vec<SelectionOwner>,
  indices: [Vec<u32>; 4],
}

impl SelectionLookup {
  pub fn new(
    picking: &crate::picking::PickingGeometry,
    colors: &crate::appearance::ColorCache,
    surface_visible: bool,
  ) -> Self {
    let covered: HashSet<_> = if surface_visible {
      colors.surface_residues().collect()
    } else {
      HashSet::new()
    };
    let atom_owner = |id: AtomId| {
      if picking
        .atom_residues
        .get(&id)
        .is_some_and(|residue| covered.contains(residue))
      {
        SelectionOwner::Unmapped
      } else {
        SelectionOwner::Atom(id)
      }
    };
    let mut owners = vec![SelectionOwner::Unmapped];
    let mut slots = HashMap::from([(SelectionOwner::Unmapped, 0)]);
    let mut slot = |owner| {
      *slots.entry(owner).or_insert_with(|| {
        let index = owners.len() as u32;
        owners.push(owner);
        index
      })
    };
    let atoms = picking.atoms.iter().map(|&id| slot(atom_owner(id))).collect();
    let bonds = picking
      .bonds
      .iter()
      .map(|&ids| {
        slot(if ids.iter().any(|&id| atom_owner(id) == SelectionOwner::Unmapped) {
          SelectionOwner::Unmapped
        } else {
          SelectionOwner::Bond(ids)
        })
      })
      .collect();
    let cartoon = picking
      .cartoon_residues
      .iter()
      .map(|id| {
        slot(match id {
          Some(id) if !covered.contains(id) => SelectionOwner::Residue(*id),
          _ => SelectionOwner::Unmapped,
        })
      })
      .collect();
    let surface = (0..colors.surface.len())
      .map(|index| {
        slot(
          colors
            .surface_owner(index)
            .map_or(SelectionOwner::Unmapped, |(id, _)| SelectionOwner::Atom(id)),
        )
      })
      .collect();
    Self {
      owners,
      indices: [atoms, bonds, cartoon, surface],
    }
  }

  pub fn flags(&self, atoms: &AtomSelection, residues: &HashSet<ResidueId>) -> Vec<f32> {
    self
      .owners
      .iter()
      .map(|owner| {
        f32::from(match owner {
          SelectionOwner::Unmapped => false,
          SelectionOwner::Atom(id) => atoms.contains(id),
          SelectionOwner::Bond(ids) => ids.iter().all(|id| atoms.contains(id)),
          SelectionOwner::Residue(id) => residues.contains(id),
        })
      })
      .collect()
  }

  pub fn vertex_buffers(&self, device: &wgpu::Device) -> [wgpu::Buffer; 4] {
    self.indices.each_ref().map(|indices| {
      device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("chitin_selection_owner_indices"),
        contents: if indices.is_empty() {
          bytemuck::bytes_of(&0_u32)
        } else {
          bytemuck::cast_slice(indices)
        },
        usage: wgpu::BufferUsages::VERTEX,
      })
    })
  }
}

pub(crate) fn lookup_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
  device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
    label: Some("chitin_selection_lookup_layout"),
    entries: &[wgpu::BindGroupLayoutEntry {
      binding: 0,
      visibility: wgpu::ShaderStages::VERTEX,
      ty: wgpu::BindingType::Buffer {
        ty: wgpu::BufferBindingType::Storage { read_only: true },
        has_dynamic_offset: false,
        min_binding_size: None,
      },
      count: None,
    }],
  })
}

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
  use super::*;

  fn fixture() -> Result<chitin_bio::structure::StructureScene, Box<dyn std::error::Error>> {
    let parsed = chitin_bio::structure::PdbParser::new().parse_bytes(b"ATOM      1  CA  GLY A   1       0.000   0.000   0.000  1.00 10.00           C  \nATOM      2  O   GLY A   1       0.000   1.000   0.000  1.00 10.00           O  \nEND\n")?;
    Ok(chitin_bio::structure::StructureScene::from_first_model(
      &parsed.structure,
    )?)
  }

  #[test]
  fn lookup_should_preserve_atom_bond_and_whole_residue_selection() -> Result<(), Box<dyn std::error::Error>> {
    let scene = fixture()?;
    let [a, b] = [scene.atoms[0], scene.atoms[1]];
    let mut colors = crate::appearance::ColorCache::new(&scene);
    colors.set_cartoon(vec![[0.; 9]; 30], vec![0; 30]);
    let picking = crate::picking::PickingGeometry {
      atoms: vec![a.atom_id, b.atom_id],
      bonds: vec![[a.atom_id, b.atom_id]],
      cartoon_residues: vec![Some(a.residue_id); 30],
      ..Default::default()
    };
    let lookup = SelectionLookup::new(&picking, &colors, false);
    let flags = lookup.flags(&[a.atom_id].into_iter().collect(), &HashSet::new());
    let project = |rows: &[u32], flags: &[f32]| rows.iter().map(|&index| flags[index as usize]).collect::<Vec<_>>();
    assert_eq!(project(&lookup.indices[0], &flags), vec![1., 0.]);
    assert_eq!(project(&lookup.indices[1], &flags), vec![0.]);
    assert_eq!(project(&lookup.indices[2], &flags), vec![0.; 30]);
    let flags = lookup.flags(&[a.atom_id, b.atom_id].into_iter().collect(), &[a.residue_id].into());
    assert_eq!(project(&lookup.indices[1], &flags), vec![1.]);
    assert_eq!(project(&lookup.indices[2], &flags), vec![1.; 30]);
    assert_eq!(lookup.owners.len(), 5);
    Ok(())
  }

  #[test]
  fn visible_surface_should_suppress_inner_highlights_but_not_occlusion() -> Result<(), Box<dyn std::error::Error>> {
    use chitin_bio::surface::{MolecularSurfaceArtifact, SurfaceDomainArtifact, SurfaceGeometrySource, SurfaceMesh};
    let scene = fixture()?;
    let a = scene.atoms[0];
    let artifact = MolecularSurfaceArtifact {
      source: SurfaceGeometrySource::Msms {
        atom_scope: Default::default(),
        probe_radius: 1.4,
        max_edge_length: 0.5,
      },
      domains: vec![SurfaceDomainArtifact {
        chain_id: Some(a.chain_id),
        mesh: SurfaceMesh {
          vertices: vec![[0., 0., -1., 0., 0., -1.]; 3],
          indices: vec![0, 1, 2],
        },
      }],
    };
    let mut colors = crate::appearance::ColorCache::new(&scene);
    colors.set_surface(vec![[0., 0., -1., 0., 0., -1., 1., 1., 1.]; 3], Some(&artifact));
    let picking = crate::picking::PickingGeometry {
      atoms: vec![a.atom_id],
      atom_residues: [(a.atom_id, a.residue_id)].into(),
      cartoon_residues: vec![Some(a.residue_id); 3],
      ..Default::default()
    };
    let lookup = SelectionLookup::new(&picking, &colors, true);
    let flags = lookup.flags(&[a.atom_id].into_iter().collect(), &[a.residue_id].into());
    assert!(
      lookup.indices[0]
        .iter()
        .chain(&lookup.indices[2])
        .all(|&index| flags[index as usize] == 0.)
    );
    assert!(lookup.indices[3].iter().all(|&index| flags[index as usize] == 1.));
    let inner = SelectionLookup::new(&picking, &colors, false);
    let flags = inner.flags(&[a.atom_id].into_iter().collect(), &[a.residue_id].into());
    assert_eq!(flags[inner.indices[0][0] as usize], 1.);
    Ok(())
  }

  #[test]
  #[ignore = "release-mode selection upload benchmark; no GPU required"]
  fn selection_lookup_update_benchmark() -> Result<(), Box<dyn std::error::Error>> {
    let scene = fixture()?;
    let residue = scene.atoms[0].residue_id;
    let mut colors = crate::appearance::ColorCache::new(&scene);
    colors.set_cartoon(vec![[0.; 9]; 1_000_000], vec![0; 1_000_000]);
    let picking = crate::picking::PickingGeometry {
      cartoon_residues: vec![Some(residue); 1_000_000],
      ..Default::default()
    };
    let lookup = SelectionLookup::new(&picking, &colors, false);
    let residues: HashSet<_> = [residue].into();
    let atoms = AtomSelection::new();
    let started = std::time::Instant::now();
    for _ in 0..100 {
      std::hint::black_box(
        picking
          .cartoon_residues
          .iter()
          .map(|id| f32::from(id.is_some_and(|id| residues.contains(&id))))
          .collect::<Vec<_>>(),
      );
    }
    eprintln!(
      "1M vertices, 100 old per-vertex updates: {:?}; upload/update: 4000000 bytes",
      started.elapsed()
    );
    let started = std::time::Instant::now();
    for _ in 0..100 {
      std::hint::black_box(lookup.flags(&atoms, &residues));
    }
    eprintln!(
      "1M vertices, 100 shared-owner updates: {:?}; upload/update: {} bytes",
      started.elapsed(),
      lookup.owners.len() * 4
    );
    assert_eq!(lookup.owners.len(), 2);
    Ok(())
  }

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
