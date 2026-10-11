//! Source-space intersections against the geometry actually displayed by the renderer.

use crate::{RepresentationLayers, appearance::ColorCache};
use chitin_bio::structure::{AtomId, ResidueId, StructureScene};
use glam::{Mat4, Vec3};
use std::collections::{HashMap, HashSet};

/// A visible hit; unowned surfaces must not select atoms behind them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MolecularPick {
  Miss,
  Residue(ResidueId),
  UnmappedSurface,
}

#[derive(Default)]
pub(crate) struct PickingGeometry {
  pub(crate) surface_boxes: crate::box_selection::BoxSelectionIndex,
  pub(crate) cartoon_boxes: crate::box_selection::BoxSelectionIndex,
  pub atom_residues: HashMap<AtomId, ResidueId>,
  pub atoms: Vec<AtomId>,
  pub bonds: Vec<[AtomId; 2]>,
  pub cartoon_indices: Vec<u32>,
  pub cartoon_residues: Vec<Option<ResidueId>>,
  pub surface_indices: Vec<u32>,
}

impl PickingGeometry {
  pub fn rebuild_surface_boxes(&mut self, colors: &ColorCache) {
    self.surface_boxes = crate::box_selection::BoxSelectionIndex::new(&self.surface_indices, &colors.surface);
  }
  pub fn rebuild_cartoon_boxes(&mut self, colors: &ColorCache) {
    self.cartoon_boxes = crate::box_selection::BoxSelectionIndex::new(&self.cartoon_indices, &colors.cartoon);
  }
  pub fn pick(
    &self,
    ndc: [f32; 2],
    mvp: Mat4,
    colors: &ColorCache,
    scene: &StructureScene,
    layers: RepresentationLayers,
  ) -> MolecularPick {
    let inverse = mvp.inverse();
    let origin = inverse.project_point3(Vec3::new(ndc[0], ndc[1], 0.0));
    let far = inverse.project_point3(Vec3::new(ndc[0], ndc[1], 1.0));
    if !origin.is_finite() || !far.is_finite() {
      return MolecularPick::Miss;
    }
    let direction = (far - origin).normalize_or_zero();
    let mut nearest = origin.distance(far);
    let mut result = MolecularPick::Miss;
    let opacity = layers.appearances().map(|appearance| appearance.opacity());
    let residue = |id: AtomId| {
      scene
        .atoms
        .iter()
        .find(|atom| atom.atom_id == id)
        .map(|atom| atom.residue_id)
    };
    if layers.atom_style().is_some() && opacity[0] > 0.0 {
      for (row, id) in colors.atoms.iter().zip(&self.atoms) {
        if let Some(distance) = sphere(origin, direction, Vec3::from_slice(&row[..3]), row[3])
          && distance < nearest
          && let Some(id) = residue(*id)
        {
          nearest = distance;
          result = MolecularPick::Residue(id);
        }
      }
      for (row, ids) in colors.bonds.iter().zip(&self.bonds) {
        let a = Vec3::from_slice(&row[..3]);
        let b = Vec3::from_slice(&row[4..7]);
        if let Some((distance, fraction)) = cylinder(origin, direction, a, b, row[3])
          && distance < nearest
          && let Some(id) = residue(ids[usize::from(fraction >= 0.5)])
        {
          nearest = distance;
          result = MolecularPick::Residue(id);
        }
      }
    }
    if layers.polymer_style().is_some() && opacity[1] > 0.0 {
      for indices in self.cartoon_indices.as_chunks::<3>().0 {
        let rows = indices.map(|index| Vec3::from_slice(&colors.cartoon[index as usize][..3]));
        if let Some((distance, barycentric)) = triangle(origin, direction, rows, false)
          && distance < nearest
        {
          let index = (0..3)
            .max_by(|a, b| barycentric[*a].total_cmp(&barycentric[*b]))
            .unwrap_or_default();
          if let Some(Some(id)) = self.cartoon_residues.get(indices[index] as usize) {
            nearest = distance;
            result = MolecularPick::Residue(*id);
          }
        }
      }
    }
    if layers.surface_style().is_some() && opacity[2] > 0.0 {
      for indices in self.surface_indices.as_chunks::<3>().0 {
        let rows = indices.map(|index| Vec3::from_slice(&colors.surface[index as usize][..3]));
        if let Some((distance, barycentric)) = triangle(origin, direction, rows, true)
          && distance < nearest
        {
          nearest = distance;
          let owners = indices.map(|index| colors.surface_owner(index as usize).map(|(_, residue)| residue));
          let owner = (0..3)
            .max_by(|a, b| {
              let weight = |i: usize| {
                (0..3)
                  .filter(|j| owners[*j] == owners[i])
                  .map(|j| barycentric[j])
                  .sum::<f32>()
              };
              weight(*a).total_cmp(&weight(*b))
            })
            .and_then(|index| owners[index]);
          result = owner.map_or(MolecularPick::UnmappedSurface, MolecularPick::Residue);
        }
      }
    }
    result
  }

  /// Box selection is through-selection of visible representations, not hidden layers.
  pub fn residues_in_rect(
    &self,
    min: [f32; 2],
    max: [f32; 2],
    mvp: Mat4,
    colors: &ColorCache,
    layers: RepresentationLayers,
  ) -> HashSet<ResidueId> {
    let inside = |position: &[f32]| {
      let p = mvp * Vec3::from_slice(&position[..3]).extend(1.);
      if !p.is_finite() || p.w <= 0. {
        return false;
      }
      let p = p.truncate() / p.w;
      p.z >= 0. && p.z <= 1. && p.x >= min[0] && p.x <= max[0] && p.y >= min[1] && p.y <= max[1]
    };
    let opacity = layers.appearances().map(|a| a.opacity());
    let surface_visible = layers.surface_style().is_some() && opacity[2] > 0. && !self.surface_indices.is_empty();
    let surface_residues = if surface_visible {
      colors.surface_residues().collect::<HashSet<_>>()
    } else {
      HashSet::new()
    };
    let mut residues = HashSet::new();
    if surface_visible {
      self.surface_boxes.visit(mvp, min, max, |indices, contained| {
        let owners = indices.map(|index| colors.surface_owner(index as usize).map(|(_, residue)| residue));
        if owners.iter().flatten().any(|residue| !residues.contains(residue))
          && (contained
            || triangle_in_rect(
              indices.map(|index| Vec3::from_slice(&colors.surface[index as usize][..3])),
              mvp,
              min,
              max,
            ))
        {
          residues.extend(owners.into_iter().flatten());
        }
      });
    }
    if layers.polymer_style().is_some() && opacity[1] > 0. {
      self.cartoon_boxes.visit(mvp, min, max, |indices, contained| {
        let owners = indices.map(|index| {
          self
            .cartoon_residues
            .get(index as usize)
            .copied()
            .flatten()
            .filter(|residue| !surface_residues.contains(residue))
        });
        if owners.iter().flatten().any(|residue| !residues.contains(residue))
          && (contained
            || triangle_in_rect(
              indices.map(|index| Vec3::from_slice(&colors.cartoon[index as usize][..3])),
              mvp,
              min,
              max,
            ))
        {
          residues.extend(owners.into_iter().flatten());
        }
      });
    }
    if layers.atom_style().is_some() && opacity[0] > 0. {
      for (row, atom) in colors.atoms.iter().zip(&self.atoms) {
        if let Some(residue) = self.atom_residues.get(atom)
          && !surface_residues.contains(residue)
          && inside(row)
        {
          residues.insert(*residue);
        }
      }
    }
    residues
  }
}

// Clip in homogeneous coordinates: boxes can overlap a large triangle without
// containing any vertex, and camera near-plane crossings must not select behind it.
pub(crate) fn triangle_in_rect(vertices: [Vec3; 3], mvp: Mat4, min: [f32; 2], max: [f32; 2]) -> bool {
  // A convex triangle clipped by six planes has at most nine vertices. Keep
  // scratch space on the stack: this runs over display meshes while dragging.
  let mut polygon = [glam::Vec4::ZERO; 12];
  polygon[..3].copy_from_slice(&vertices.map(|vertex| mvp * vertex.extend(1.)));
  let mut count = 3;
  if polygon[..count].iter().any(|p| !p.is_finite()) {
    return false;
  }
  let planes = [
    glam::Vec4::new(1., 0., 0., -min[0]),
    glam::Vec4::new(-1., 0., 0., max[0]),
    glam::Vec4::new(0., 1., 0., -min[1]),
    glam::Vec4::new(0., -1., 0., max[1]),
    glam::Vec4::new(0., 0., 1., 0.),
    glam::Vec4::new(0., 0., -1., 1.),
  ];
  for plane in planes {
    if count == 0 {
      return false;
    }
    let mut clipped = [glam::Vec4::ZERO; 12];
    let mut clipped_count = 0;
    let mut previous = polygon[count - 1];
    let mut previous_distance = plane.dot(previous);
    for &current in &polygon[..count] {
      let distance = plane.dot(current);
      if (distance >= 0.) != (previous_distance >= 0.) && distance != 0. && previous_distance != 0. {
        clipped[clipped_count] = previous.lerp(current, previous_distance / (previous_distance - distance));
        clipped_count += 1;
      }
      if distance >= 0. {
        clipped[clipped_count] = current;
        clipped_count += 1;
      }
      previous = current;
      previous_distance = distance;
    }
    polygon = clipped;
    count = clipped_count;
  }
  polygon[..count].iter().any(|p| p.w > 0.)
}

fn sphere(origin: Vec3, direction: Vec3, center: Vec3, radius: f32) -> Option<f32> {
  let offset = origin - center;
  let b = offset.dot(direction);
  let c = offset.length_squared() - radius * radius;
  let discriminant = b * b - c;
  if discriminant < 0.0 {
    return None;
  }
  let distance = -b - discriminant.sqrt();
  (distance >= 0.0).then_some(distance)
}
fn cylinder(origin: Vec3, direction: Vec3, a: Vec3, b: Vec3, radius: f32) -> Option<(f32, f32)> {
  let axis = b - a;
  let offset = origin - a;
  let length = axis.length_squared();
  let ad = axis.dot(direction);
  let ao = axis.dot(offset);
  let od = direction.dot(offset);
  let qa = length - ad * ad;
  let qb = length * od - ao * ad;
  let qc = length * offset.length_squared() - ao * ao - radius * radius * length;
  let discriminant = qb * qb - qa * qc;
  if discriminant < 0.0 || qa < 1.0e-6 {
    return None;
  }
  let distance = (-qb - discriminant.sqrt()) / qa;
  let height = ao + distance * ad;
  (distance >= 0.0 && height >= 0.0 && height <= length).then_some((distance, height / length))
}
fn triangle(origin: Vec3, direction: Vec3, [a, b, c]: [Vec3; 3], cull: bool) -> Option<(f32, [f32; 3])> {
  let e1 = b - a;
  let e2 = c - a;
  let p = direction.cross(e2);
  let determinant = e1.dot(p);
  if (cull && determinant < 1.0e-7) || determinant.abs() < 1.0e-7 {
    return None;
  }
  let offset = origin - a;
  let u = offset.dot(p) / determinant;
  let q = offset.cross(e1);
  let v = direction.dot(q) / determinant;
  let distance = e2.dot(q) / determinant;
  (u >= 0.0 && v >= 0.0 && u + v <= 1.0 && distance >= 0.0).then_some((distance, [1.0 - u - v, u, v]))
}

#[cfg(test)]
mod tests {
  use super::*;
  #[test]
  fn intersections_should_hit_geometry_not_nearby_screen_space_centers() {
    assert_eq!(sphere(Vec3::new(0., 0., 5.), -Vec3::Z, Vec3::ZERO, 1.), Some(4.));
    assert_eq!(sphere(Vec3::new(2., 0., 5.), -Vec3::Z, Vec3::ZERO, 1.), None);
    assert!(cylinder(Vec3::new(0., 0., 5.), -Vec3::Z, -Vec3::X, Vec3::X, 0.2).is_some());
    assert!(triangle(Vec3::new(0., 0., 5.), -Vec3::Z, [-Vec3::X, Vec3::X, Vec3::Y], true).is_some());
  }

  #[test]
  fn box_should_intersect_triangle_interiors_and_clip_depth() {
    let triangle = [
      Vec3::new(-1., -1., 0.5),
      Vec3::new(1., -1., 0.5),
      Vec3::new(0., 1., 0.5),
    ];
    assert!(triangle_in_rect(triangle, Mat4::IDENTITY, [-0.05, -0.05], [0.05, 0.05]));
    assert!(!triangle_in_rect(triangle, Mat4::IDENTITY, [0.8, 0.8], [0.9, 0.9]));
    assert!(!triangle_in_rect(
      triangle.map(|p| p - Vec3::Z),
      Mat4::IDENTITY,
      [-0.05, -0.05],
      [0.05, 0.05]
    ));
    assert!(!triangle_in_rect(
      triangle.map(|p| p + Vec3::Z),
      Mat4::IDENTITY,
      [-0.05, -0.05],
      [0.05, 0.05]
    ));
    let crossing = [triangle[0] - Vec3::Z, triangle[1], triangle[2]];
    assert!(triangle_in_rect(crossing, Mat4::IDENTITY, [-0.05, -0.05], [0.05, 0.05]));
  }

  #[test]
  fn visible_surface_should_block_picking_hidden_atoms_and_zero_opacity_should_not_pick()
  -> Result<(), Box<dyn std::error::Error>> {
    let parsed = chitin_bio::structure::PdbParser::new()
      .parse_bytes(b"ATOM      1  CA  GLY A  42       0.000   0.000   0.500  1.00 10.00           C  \nEND\n")?;
    let scene = StructureScene::from_first_model(&parsed.structure)?;
    let atom = scene.atoms.first().ok_or("fixture atom")?;
    let mut colors = ColorCache::new(&scene);
    colors.set_atoms(vec![[0., 0., 0.5, 0.1, 1., 1., 1., 1.]]);
    colors.set_surface(
      vec![
        [-1., -1., 0.2, 0., 0., -1., 1., 1., 1.],
        [-1., 1., 0.2, 0., 0., -1., 1., 1., 1.],
        [1., -1., 0.2, 0., 0., -1., 1., 1., 1.],
      ],
      None,
    );
    let geometry = PickingGeometry {
      atoms: vec![atom.atom_id],
      surface_indices: vec![0, 1, 2],
      ..Default::default()
    };
    let layers = RepresentationLayers::atom(crate::AtomStyle::Sphere).with_surface(crate::SurfaceStyle::Solid);
    assert_eq!(
      geometry.pick([0., 0.], Mat4::IDENTITY, &colors, &scene, layers),
      MolecularPick::UnmappedSurface
    );
    let mut appearance = layers.appearances();
    appearance[2] = appearance[2].with_opacity(0.).ok_or("valid opacity")?;
    let layers = layers.with_appearances(appearance);
    assert_eq!(
      geometry.pick([0., 0.], Mat4::IDENTITY, &colors, &scene, layers),
      MolecularPick::Residue(atom.residue_id)
    );
    appearance[0] = appearance[0].with_opacity(0.).ok_or("valid opacity")?;
    assert_eq!(
      geometry.pick(
        [0., 0.],
        Mat4::IDENTITY,
        &colors,
        &scene,
        layers.with_appearances(appearance)
      ),
      MolecularPick::Miss
    );
    Ok(())
  }

  #[test]
  fn mapped_surface_should_pick_and_box_select_the_outer_residue() -> Result<(), Box<dyn std::error::Error>> {
    use chitin_bio::surface::{
      MolecularSurfaceArtifact, SurfaceAtomScope, SurfaceDomainArtifact, SurfaceGeometrySource, SurfaceMesh,
    };
    let parsed = chitin_bio::structure::PdbParser::new().parse_bytes(b"ATOM      1  CA  GLY A  42       0.000   0.000   0.500  1.00 10.00           C  \nATOM      2  CA  GLY B  43       0.000   0.000   0.800  1.00 10.00           C  \nEND\n")?;
    let scene = StructureScene::from_first_model(&parsed.structure)?;
    let a = scene.atoms[0];
    let b = scene.atoms[1];
    let artifact = MolecularSurfaceArtifact {
      source: SurfaceGeometrySource::Msms {
        atom_scope: SurfaceAtomScope::default(),
        probe_radius: 1.4,
        max_edge_length: 0.5,
      },
      domains: vec![SurfaceDomainArtifact {
        chain_id: Some(b.chain_id),
        mesh: SurfaceMesh {
          vertices: vec![
            [-0.1, -0.1, 0.2, 0., 0., -1.],
            [-0.1, 0.1, 0.2, 0., 0., -1.],
            [0.1, -0.1, 0.2, 0., 0., -1.],
          ],
          indices: vec![0, 1, 2],
        },
      }],
    };
    let (rows, indices) = crate::surface::surface_mesh_vertices(Some(&artifact), [1.; 3]);
    let mut colors = ColorCache::new(&scene);
    colors.set_atoms(vec![[0., 0., 0.5, 0.05, 1., 1., 1., 1.]]);
    colors.set_surface(rows, Some(&artifact));
    let mut geometry = PickingGeometry {
      atoms: vec![a.atom_id],
      atom_residues: [(a.atom_id, a.residue_id), (b.atom_id, b.residue_id)].into(),
      surface_indices: indices,
      ..Default::default()
    };
    geometry.rebuild_surface_boxes(&colors);
    let layers = RepresentationLayers::atom(crate::AtomStyle::Sphere).with_surface(crate::SurfaceStyle::Solid);
    assert_eq!(
      geometry.pick([0., 0.], Mat4::IDENTITY, &colors, &scene, layers),
      MolecularPick::Residue(b.residue_id)
    );
    assert_eq!(
      geometry.residues_in_rect([-0.2, -0.2], [0.2, 0.2], Mat4::IDENTITY, &colors, layers),
      [a.residue_id, b.residue_id].into()
    );
    // A residue covered by a surface is not selected through its inner atom.
    let covered = PickingGeometry {
      atoms: vec![b.atom_id],
      ..geometry
    };
    assert_eq!(
      covered.residues_in_rect([-0.01, -0.01], [0.01, 0.01], Mat4::IDENTITY, &colors, layers),
      [b.residue_id].into()
    );
    let mut appearance = layers.appearances();
    appearance[2] = appearance[2].with_opacity(0.).ok_or("opacity")?;
    assert_eq!(
      covered.residues_in_rect(
        [-0.01, -0.01],
        [0.01, 0.01],
        Mat4::IDENTITY,
        &colors,
        layers.with_appearances(appearance)
      ),
      [b.residue_id].into()
    );
    Ok(())
  }
}
