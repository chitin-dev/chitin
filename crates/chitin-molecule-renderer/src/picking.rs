//! Source-space intersections against the geometry actually displayed by the renderer.

use crate::{RepresentationLayers, appearance::ColorCache};
use chitin_bio::structure::{AtomId, ResidueId, StructureScene};
use glam::{Mat4, Vec3};
use std::{
  collections::{HashMap, HashSet},
  sync::Arc,
};

/// Immutable CPU picking snapshot, cheap to share with a background worker.
pub struct BoxSelectionQuery {
  pub(crate) geometry: Arc<PickingGeometry>,
  pub(crate) colors: Arc<ColorCache>,
  pub(crate) model_view: Mat4,
  pub(crate) projection: Mat4,
  pub(crate) layers: RepresentationLayers,
}

impl BoxSelectionQuery {
  /// Queries foreground residues and expands them to whole-residue atom identities.
  pub fn select(
    &self,
    min: [f32; 2],
    max: [f32; 2],
    depth: ForegroundSelectionDepth,
  ) -> chitin_bio::structure::selection::AtomSelection {
    let residues = self.geometry.foreground_residues_in_rect(
      min,
      max,
      self.projection * self.model_view,
      &self.colors,
      self.layers,
      ForegroundDepthWindow {
        axis: self.model_view.row(2).truncate().normalize_or_zero(),
        thickness: depth,
      },
    );
    self
      .geometry
      .atom_residues
      .iter()
      .filter_map(|(&atom, residue)| {
        (residues.contains(residue) && self.geometry.visibility.atom(atom)).then_some(atom)
      })
      .collect()
  }
}

/// A visible hit; unowned surfaces must not select atoms behind them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MolecularPick {
  Miss,
  Residue(ResidueId),
  UnmappedSurface,
}

/// Camera-depth thickness of foreground box selection, measured in source ångströms.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ForegroundSelectionDepth(f32);

impl ForegroundSelectionDepth {
  /// Rejects non-finite or negative depths; zero keeps only the nearest depth.
  pub fn new(angstroms: f32) -> Option<Self> {
    (angstroms.is_finite() && angstroms >= 0.).then_some(Self(angstroms))
  }

  pub fn angstroms(self) -> f32 {
    self.0
  }
}

impl Default for ForegroundSelectionDepth {
  fn default() -> Self {
    Self(5.)
  }
}

pub(crate) struct ForegroundDepthWindow {
  /// Unit source-space axis pointing along the camera's positive Z axis.
  pub axis: Vec3,
  pub thickness: ForegroundSelectionDepth,
}

struct VisibleHit {
  owner: MolecularPick,
  position: Vec3,
}

struct BoxHits {
  depths: HashMap<ResidueId, f32>,
  nearest: f32,
  window: Option<ForegroundDepthWindow>,
}

impl BoxHits {
  fn new(window: Option<ForegroundDepthWindow>) -> Self {
    Self {
      depths: HashMap::new(),
      nearest: f32::INFINITY,
      window,
    }
  }

  fn depth(&self, position: Vec3) -> f32 {
    self.window.as_ref().map_or(0., |window| -window.axis.dot(position))
  }

  fn needs_probe(&self, owners: [Option<ResidueId>; 3], lower_depth: f32) -> bool {
    // Once a foreground witness exists, geometry wholly beyond the slab cannot
    // contribute. Its bound also cannot contain a nearer foreground witness.
    if self
      .window
      .as_ref()
      .is_some_and(|window| lower_depth > self.nearest + window.thickness.angstroms())
    {
      return false;
    }
    self.nearest > lower_depth
      || owners
        .iter()
        .flatten()
        .any(|id| self.depths.get(id).is_none_or(|depth| *depth > lower_depth))
  }

  fn record(&mut self, hit: VisibleHit, owners: [Option<ResidueId>; 3]) {
    let depth = self.depth(hit.position);
    if !depth.is_finite() {
      return;
    }
    self.nearest = self.nearest.min(depth);
    if let MolecularPick::Residue(id) = hit.owner
      && owners.contains(&Some(id))
    {
      self
        .depths
        .entry(id)
        .and_modify(|old| *old = old.min(depth))
        .or_insert(depth);
    }
  }

  fn finish(self) -> HashSet<ResidueId> {
    let limit = self
      .window
      .map_or(f32::INFINITY, |window| self.nearest + window.thickness.angstroms());
    self
      .depths
      .into_iter()
      .filter_map(|(id, depth)| (depth <= limit).then_some(id))
      .collect()
  }
}

#[derive(Clone, Default)]
pub(crate) struct PickingGeometry {
  pub visibility: Arc<crate::visibility::VisibilityMask>,
  pub(crate) atom_boxes: crate::box_selection::BoxSelectionIndex,
  pub(crate) bond_boxes: crate::box_selection::BoxSelectionIndex,
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
  pub fn rebuild_atom_boxes(&mut self, colors: &ColorCache) {
    self.atom_boxes = crate::box_selection::BoxSelectionIndex::from_bounds(colors.atoms.iter().map(|row| {
      let center = Vec3::from_slice(&row[..3]);
      let radius = Vec3::splat(row[3]);
      [center - radius, center + radius]
    }));
    self.bond_boxes = crate::box_selection::BoxSelectionIndex::from_bounds(colors.bonds.iter().map(|row| {
      let a = Vec3::from_slice(&row[..3]);
      let b = Vec3::from_slice(&row[4..7]);
      let radius = Vec3::splat(row[3]);
      [a.min(b) - radius, a.max(b) + radius]
    }));
  }
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
    _scene: &StructureScene,
    layers: RepresentationLayers,
  ) -> MolecularPick {
    self.pick_with_inverse(ndc, mvp.inverse(), colors, layers)
  }

  fn pick_with_inverse(
    &self,
    ndc: [f32; 2],
    inverse: Mat4,
    colors: &ColorCache,
    layers: RepresentationLayers,
  ) -> MolecularPick {
    self
      .hit_with_inverse(ndc, inverse, colors, layers)
      .map_or(MolecularPick::Miss, |hit| hit.owner)
  }

  fn hit_with_inverse(
    &self,
    ndc: [f32; 2],
    inverse: Mat4,
    colors: &ColorCache,
    layers: RepresentationLayers,
  ) -> Option<VisibleHit> {
    let origin = inverse.project_point3(Vec3::new(ndc[0], ndc[1], 0.0));
    let far = inverse.project_point3(Vec3::new(ndc[0], ndc[1], 1.0));
    if !origin.is_finite() || !far.is_finite() {
      return None;
    }
    let direction = (far - origin).normalize_or_zero();
    let mut nearest = origin.distance(far);
    let mut result = MolecularPick::Miss;
    let opacity = layers.appearances().map(|appearance| appearance.opacity());
    let residue = |id: AtomId| self.atom_residues.get(&id).copied();
    // The outer surface usually supplies a short depth bound, pruning hidden
    // atom/cartoon trees before traversing their triangles.
    if layers.surface_style().is_some() && opacity[2] > 0.0 {
      nearest = self
        .surface_boxes
        .visit_ray(origin, direction, nearest, |indices, mut nearest| {
          if !self.visibility.surface(indices, colors) {
            return nearest;
          }
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
          nearest
        });
    }
    if layers.atom_style().is_some() && opacity[0] > 0.0 {
      nearest = self
        .atom_boxes
        .visit_ray(origin, direction, nearest, |indices, mut nearest| {
          let index = indices[0] as usize / 2;
          let row = &colors.atoms[index];
          let id = self.atoms[index];
          if !self.visibility.atom(id) {
            return nearest;
          }
          if let Some(distance) = sphere(origin, direction, Vec3::from_slice(&row[..3]), row[3])
            && distance < nearest
            && let Some(id) = residue(id)
          {
            nearest = distance;
            result = MolecularPick::Residue(id);
          }
          nearest
        });
      nearest = self
        .bond_boxes
        .visit_ray(origin, direction, nearest, |indices, mut nearest| {
          let index = indices[0] as usize / 2;
          let row = &colors.bonds[index];
          let ids = self.bonds[index];
          if !self.visibility.bond(ids) {
            return nearest;
          }
          let a = Vec3::from_slice(&row[..3]);
          let b = Vec3::from_slice(&row[4..7]);
          if let Some((distance, fraction)) = cylinder(origin, direction, a, b, row[3])
            && distance < nearest
            && let Some(id) = residue(ids[usize::from(fraction >= 0.5)])
          {
            nearest = distance;
            result = MolecularPick::Residue(id);
          }
          nearest
        });
    }
    if layers.polymer_style().is_some() && opacity[1] > 0.0 {
      nearest = self
        .cartoon_boxes
        .visit_ray(origin, direction, nearest, |indices, mut nearest| {
          if !self.visibility.cartoon(indices, &self.cartoon_residues) {
            return nearest;
          }
          let rows = indices.map(|index| Vec3::from_slice(&colors.cartoon[index as usize][..3]));
          if let Some((distance, barycentric)) = triangle(origin, direction, rows, false)
            && distance < nearest
          {
            let index = (0..3)
              .max_by(|a, b| barycentric[*a].total_cmp(&barycentric[*b]))
              .unwrap_or_default();
            nearest = distance;
            result = self
              .cartoon_residues
              .get(indices[index] as usize)
              .copied()
              .flatten()
              .map_or(MolecularPick::UnmappedSurface, MolecularPick::Residue);
          }
          nearest
        });
    }
    (result != MolecularPick::Miss).then_some(VisibleHit {
      owner: result,
      position: origin + direction * nearest,
    })
  }

  /// Selects only residues confirmed by a closest hit inside the rectangle.
  /// Interior probes are conservative: very small visible slivers can be missed.
  #[cfg(test)]
  pub fn residues_in_rect(
    &self,
    min: [f32; 2],
    max: [f32; 2],
    mvp: Mat4,
    colors: &ColorCache,
    layers: RepresentationLayers,
  ) -> HashSet<ResidueId> {
    self.box_residues(min, max, mvp, colors, layers, None)
  }

  pub fn foreground_residues_in_rect(
    &self,
    min: [f32; 2],
    max: [f32; 2],
    mvp: Mat4,
    colors: &ColorCache,
    layers: RepresentationLayers,
    window: ForegroundDepthWindow,
  ) -> HashSet<ResidueId> {
    self.box_residues(min, max, mvp, colors, layers, Some(window))
  }

  fn box_residues(
    &self,
    min: [f32; 2],
    max: [f32; 2],
    mvp: Mat4,
    colors: &ColorCache,
    layers: RepresentationLayers,
    window: Option<ForegroundDepthWindow>,
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
    let inverse = mvp.inverse();
    let surface_visible = layers.surface_style().is_some() && opacity[2] > 0. && !self.surface_indices.is_empty();
    let surface_residues = if surface_visible {
      colors.surface_residues().collect::<HashSet<_>>()
    } else {
      HashSet::new()
    };
    let mut hits = BoxHits::new(window);
    if surface_visible {
      self.surface_boxes.visit(mvp, min, max, |indices, _contained| {
        if !self.visibility.surface(indices, colors) {
          return;
        }
        let owners = indices.map(|index| colors.surface_owner(index as usize).map(|(_, residue)| residue));
        let vertices = indices.map(|index| Vec3::from_slice(&colors.surface[index as usize][..3]));
        let lower = vertices
          .map(|p| hits.depth(p))
          .into_iter()
          .fold(f32::INFINITY, f32::min);
        if hits.needs_probe(owners, lower) {
          self.visible_triangle_residues(
            vertices, owners, true, mvp, inverse, min, max, colors, layers, &mut hits,
          );
        }
      });
    }
    if layers.polymer_style().is_some() && opacity[1] > 0. {
      self.cartoon_boxes.visit(mvp, min, max, |indices, _contained| {
        if !self.visibility.cartoon(indices, &self.cartoon_residues) {
          return;
        }
        let owners = indices.map(|index| {
          self
            .cartoon_residues
            .get(index as usize)
            .copied()
            .flatten()
            .filter(|residue| !surface_residues.contains(residue))
        });
        let vertices = indices.map(|index| Vec3::from_slice(&colors.cartoon[index as usize][..3]));
        let lower = vertices
          .map(|p| hits.depth(p))
          .into_iter()
          .fold(f32::INFINITY, f32::min);
        if hits.needs_probe(owners, lower) {
          self.visible_triangle_residues(
            vertices, owners, false, mvp, inverse, min, max, colors, layers, &mut hits,
          );
        }
      });
    }
    if layers.atom_style().is_some() && opacity[0] > 0. {
      for (row, atom) in colors.atoms.iter().zip(&self.atoms) {
        if !self.visibility.atom(*atom) {
          continue;
        }
        if let Some(residue) = self.atom_residues.get(atom)
          && !surface_residues.contains(residue)
          && hits.needs_probe([Some(*residue); 3], hits.depth(Vec3::from_slice(&row[..3])) - row[3])
          && inside(row)
        {
          let p = mvp.project_point3(Vec3::from_slice(&row[..3]));
          if let Some(hit) = self.hit_with_inverse([p.x, p.y], inverse, colors, layers) {
            hits.record(hit, [Some(*residue); 3]);
          }
        }
      }
    }
    hits.finish()
  }

  #[allow(clippy::too_many_arguments)]
  fn visible_triangle_residues(
    &self,
    vertices: [Vec3; 3],
    owners: [Option<ResidueId>; 3],
    cull: bool,
    mvp: Mat4,
    inverse: Mat4,
    min: [f32; 2],
    max: [f32; 2],
    colors: &ColorCache,
    layers: RepresentationLayers,
    hits: &mut BoxHits,
  ) {
    let (polygon, count) = clipped_triangle(vertices, mvp, min, max);
    if count == 0 || polygon[..count].iter().any(|p| p.w <= 0.) {
      return;
    }
    let points = polygon.map(|p| p.truncate() / p.w);
    if cull {
      let area = (0..count)
        .map(|i| {
          let a = points[i];
          let b = points[(i + 1) % count];
          a.x * b.y - a.y * b.x
        })
        .sum::<f32>();
      if area >= 0. {
        return;
      }
    }
    let center = points[..count].iter().copied().sum::<Vec3>() / count as f32;
    let lower = vertices
      .map(|p| hits.depth(p))
      .into_iter()
      .fold(f32::INFINITY, f32::min);
    // Stay inside the clipped polygon rather than sampling shared triangle edges.
    // A witness farther back cannot suppress a later, nearer witness of the same residue.
    for point in std::iter::once(center).chain(points[..count].iter().map(|p| center.lerp(*p, 0.9))) {
      if let Some(hit) = self.hit_with_inverse([point.x, point.y], inverse, colors, layers) {
        hits.record(hit, owners);
      }
      if !hits.needs_probe(owners, lower) {
        break;
      }
    }
  }
}

// Clip in homogeneous coordinates: boxes can overlap a large triangle without
// containing any vertex, and camera near-plane crossings must not select behind it.
#[cfg(test)]
pub(crate) fn triangle_in_rect(vertices: [Vec3; 3], mvp: Mat4, min: [f32; 2], max: [f32; 2]) -> bool {
  let (polygon, count) = clipped_triangle(vertices, mvp, min, max);
  polygon[..count].iter().any(|p| p.w > 0.)
}

fn clipped_triangle(vertices: [Vec3; 3], mvp: Mat4, min: [f32; 2], max: [f32; 2]) -> ([glam::Vec4; 12], usize) {
  // A convex triangle clipped by six planes has at most nine vertices. Keep
  // scratch space on the stack: this runs over display meshes while dragging.
  let mut polygon = [glam::Vec4::ZERO; 12];
  polygon[..3].copy_from_slice(&vertices.map(|vertex| mvp * vertex.extend(1.)));
  let mut count = 3;
  if polygon[..count].iter().any(|p| !p.is_finite()) {
    return (polygon, 0);
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
      return (polygon, 0);
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
  (polygon, count)
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
  fn foreground_depth_should_reject_invalid_values() {
    for value in [-1., f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
      assert!(ForegroundSelectionDepth::new(value).is_none());
    }
    assert_eq!(ForegroundSelectionDepth::default().angstroms(), 5.);
    assert!(ForegroundSelectionDepth::new(0.).is_some());
  }

  #[test]
  fn foreground_box_should_exclude_visible_distant_fragments_regardless_of_traversal_order()
  -> Result<(), Box<dyn std::error::Error>> {
    let scene = two_residue_scene()?;
    let a = scene.atoms[0].residue_id;
    let b = scene.atoms[1].residue_id;
    for reversed in [false, true] {
      let mut colors = ColorCache::new(&scene);
      let (first, second) = if reversed {
        ((0.5, 0.7, b), (-0.5, 0.2, a))
      } else {
        ((-0.5, 0.2, a), (0.5, 0.7, b))
      };
      colors.set_cartoon(
        mesh_triangle(first.0, first.1)
          .into_iter()
          .chain(mesh_triangle(second.0, second.1))
          .collect(),
        vec![0; 6],
      );
      let mut geometry = PickingGeometry {
        cartoon_indices: (0..6).collect(),
        cartoon_residues: [Some(first.2); 3].into_iter().chain([Some(second.2); 3]).collect(),
        ..Default::default()
      };
      geometry.rebuild_cartoon_boxes(&colors);
      let layers = RepresentationLayers::default().with_polymer(crate::PolymerStyle::Cartoon);
      assert_eq!(
        geometry.residues_in_rect([-1., -1.], [1., 1.], Mat4::IDENTITY, &colors, layers),
        [a, b].into()
      );
      assert_eq!(
        geometry.foreground_residues_in_rect(
          [-1., -1.],
          [1., 1.],
          Mat4::IDENTITY,
          &colors,
          layers,
          ForegroundDepthWindow {
            axis: -Vec3::Z,
            thickness: ForegroundSelectionDepth::new(0.1).ok_or("depth")?
          }
        ),
        [a].into()
      );
      // With the near fragment outside the box, the distant fragment becomes its foreground.
      assert_eq!(
        geometry.foreground_residues_in_rect(
          [0.2, -0.4],
          [0.9, 0.4],
          Mat4::IDENTITY,
          &colors,
          layers,
          ForegroundDepthWindow {
            axis: -Vec3::Z,
            thickness: ForegroundSelectionDepth::new(0.1).ok_or("depth")?
          }
        ),
        [b].into()
      );
    }
    Ok(())
  }

  #[test]
  fn foreground_box_should_use_angstrom_depth_independent_of_fit_and_zoom() -> Result<(), Box<dyn std::error::Error>> {
    let scene = two_residue_scene()?;
    let a = scene.atoms[0];
    let b = scene.atoms[1];
    let mut colors = ColorCache::new(&scene);
    colors.set_atoms(vec![
      [-2., 0., -10., 0.2, 1., 1., 1., 1.],
      [2., 0., -20., 0.2, 1., 1., 1., 1.],
    ]);
    let mut geometry = PickingGeometry {
      atoms: vec![a.atom_id, b.atom_id],
      atom_residues: [(a.atom_id, a.residue_id), (b.atom_id, b.residue_id)].into(),
      ..Default::default()
    };
    geometry.rebuild_atom_boxes(&colors);
    let projection = glam::camera::rh::proj::directx::perspective(0.7, 1., 0.1, 100.);
    for scale in [0.02, 0.1, 1.] {
      for distance in [2., 4.] {
        let model_view = Mat4::from_translation(Vec3::new(0., 0., -distance))
          * Mat4::from_rotation_y(0.1)
          * Mat4::from_scale(Vec3::splat(scale));
        let axis = model_view.row(2).truncate().normalize();
        let layers = RepresentationLayers::atom(crate::AtomStyle::Sphere);
        assert_eq!(
          geometry.foreground_residues_in_rect(
            [-1., -1.],
            [1., 1.],
            projection * model_view,
            &colors,
            layers,
            ForegroundDepthWindow {
              axis,
              thickness: Default::default()
            }
          ),
          [a.residue_id].into()
        );
        assert_eq!(
          geometry.foreground_residues_in_rect(
            [-1., -1.],
            [1., 1.],
            projection * model_view,
            &colors,
            layers,
            ForegroundDepthWindow {
              axis,
              thickness: ForegroundSelectionDepth::new(15.).ok_or("depth")?
            }
          ),
          [a.residue_id, b.residue_id].into()
        );
      }
    }
    Ok(())
  }

  #[test]
  fn nearer_hit_of_the_same_residue_should_update_the_depth_anchor() -> Result<(), Box<dyn std::error::Error>> {
    let scene = two_residue_scene()?;
    let a = scene.atoms[0].residue_id;
    let b = scene.atoms[1].residue_id;
    let mut hits = BoxHits::new(Some(ForegroundDepthWindow {
      axis: -Vec3::Z,
      thickness: Default::default(),
    }));
    for (id, depth) in [(a, 30.), (b, 12.), (a, 10.)] {
      hits.record(
        VisibleHit {
          owner: MolecularPick::Residue(id),
          position: Vec3::Z * depth,
        },
        [Some(id); 3],
      );
    }
    assert_eq!(hits.finish(), [a, b].into());
    Ok(())
  }

  #[test]
  fn foreground_surface_box_should_exclude_visible_far_domains() -> Result<(), Box<dyn std::error::Error>> {
    use chitin_bio::surface::{MolecularSurfaceArtifact, SurfaceDomainArtifact, SurfaceGeometrySource, SurfaceMesh};
    let scene = two_residue_scene()?;
    let a = scene.atoms[0];
    let b = scene.atoms[1];
    let artifact = MolecularSurfaceArtifact {
      source: SurfaceGeometrySource::Msms {
        atom_scope: Default::default(),
        probe_radius: 1.4,
        max_edge_length: 0.5,
      },
      domains: [(-0.5, 0.2, a.chain_id), (0.5, 0.7, b.chain_id)]
        .map(|(x, z, chain)| SurfaceDomainArtifact {
          chain_id: Some(chain),
          mesh: SurfaceMesh {
            vertices: mesh_triangle(x, z)
              .map(|row| [row[0], row[1], row[2], row[3], row[4], row[5]])
              .into(),
            indices: vec![0, 1, 2],
          },
        })
        .into(),
    };
    let (rows, indices) = crate::surface::surface_mesh_vertices(Some(&artifact), [1.; 3]);
    let mut colors = ColorCache::new(&scene);
    colors.set_surface(rows, Some(&artifact));
    let mut geometry = PickingGeometry {
      surface_indices: indices,
      ..Default::default()
    };
    geometry.rebuild_surface_boxes(&colors);
    let layers = RepresentationLayers::default().with_surface(crate::SurfaceStyle::Solid);
    assert_eq!(
      geometry.foreground_residues_in_rect(
        [-1., -1.],
        [1., 1.],
        Mat4::IDENTITY,
        &colors,
        layers,
        ForegroundDepthWindow {
          axis: -Vec3::Z,
          thickness: ForegroundSelectionDepth::new(0.1).ok_or("depth")?
        }
      ),
      [a.residue_id].into()
    );
    Ok(())
  }

  #[test]
  fn foreground_depth_boundary_should_be_inclusive_and_unowned_hits_should_anchor_the_range()
  -> Result<(), Box<dyn std::error::Error>> {
    let scene = two_residue_scene()?;
    let id = scene.atoms[0].residue_id;
    for depth in [15., 15.01] {
      let mut hits = BoxHits::new(Some(ForegroundDepthWindow {
        axis: -Vec3::Z,
        thickness: Default::default(),
      }));
      hits.record(
        VisibleHit {
          owner: MolecularPick::UnmappedSurface,
          position: Vec3::Z * 10.,
        },
        [None; 3],
      );
      hits.record(
        VisibleHit {
          owner: MolecularPick::Residue(id),
          position: Vec3::Z * depth,
        },
        [Some(id); 3],
      );
      assert_eq!(hits.finish().contains(&id), depth == 15.);
    }
    Ok(())
  }

  fn two_residue_scene() -> Result<StructureScene, Box<dyn std::error::Error>> {
    let parsed = chitin_bio::structure::PdbParser::new().parse_bytes(b"ATOM      1  CA  GLY A  42       0.000   0.000   0.500  1.00 10.00           C  \nATOM      2  CA  GLY B  43       0.000   0.000   0.800  1.00 10.00           C  \nEND\n")?;
    Ok(StructureScene::from_first_model(&parsed.structure)?)
  }

  #[test]
  fn hidden_atoms_should_not_occlude_or_enter_mouse_selection() -> Result<(), Box<dyn std::error::Error>> {
    let scene = two_residue_scene()?;
    let a = scene.atoms[0];
    let b = scene.atoms[1];
    let mut colors = ColorCache::new(&scene);
    colors.set_atoms(vec![
      [0., 0., 0.5, 0.1, 1., 1., 1., 1.],
      [0., 0., 0.8, 0.1, 1., 1., 1., 1.],
    ]);
    let mut geometry = PickingGeometry {
      atoms: vec![a.atom_id, b.atom_id],
      atom_residues: [(a.atom_id, a.residue_id), (b.atom_id, b.residue_id)].into(),
      ..Default::default()
    };
    geometry.rebuild_atom_boxes(&colors);
    let layers = RepresentationLayers::atom(crate::AtomStyle::Sphere);
    assert_eq!(
      geometry.pick([0., 0.], Mat4::IDENTITY, &colors, &scene, layers),
      MolecularPick::Residue(a.residue_id)
    );
    geometry.visibility = Arc::new(crate::visibility::VisibilityMask::new(
      &geometry.atom_residues,
      &[a.atom_id].into(),
    ));
    assert_eq!(
      geometry.pick([0., 0.], Mat4::IDENTITY, &colors, &scene, layers),
      MolecularPick::Residue(b.residue_id)
    );
    let query = BoxSelectionQuery {
      geometry: Arc::new(geometry),
      colors: Arc::new(colors),
      model_view: Mat4::IDENTITY,
      projection: Mat4::IDENTITY,
      layers,
    };
    assert_eq!(
      query.select([-0.2, -0.2], [0.2, 0.2], Default::default()),
      [b.atom_id].into()
    );
    let mut restored = query.geometry.as_ref().clone();
    restored.visibility = Arc::default();
    assert_eq!(
      restored.pick([0., 0.], Mat4::IDENTITY, &query.colors, &scene, layers),
      MolecularPick::Residue(a.residue_id)
    );
    // An in-flight query retains its original visibility snapshot.
    assert_eq!(
      query.select([-0.2, -0.2], [0.2, 0.2], Default::default()),
      [b.atom_id].into()
    );
    Ok(())
  }

  #[test]
  fn depth_pruning_should_keep_the_boundary_and_allow_later_nearer_witnesses() -> Result<(), Box<dyn std::error::Error>>
  {
    let scene = two_residue_scene()?;
    let [a, b] = [scene.atoms[0].residue_id, scene.atoms[1].residue_id];
    let mut hits = BoxHits::new(Some(ForegroundDepthWindow {
      axis: Vec3::Z,
      thickness: Default::default(),
    }));
    assert!(hits.needs_probe([Some(a); 3], 100.));
    hits.record(
      VisibleHit {
        owner: MolecularPick::Residue(a),
        position: Vec3::new(0., 0., -10.),
      },
      [Some(a); 3],
    );
    assert!(!hits.needs_probe([Some(b); 3], 15.01));
    assert!(hits.needs_probe([Some(b); 3], 15.));
    assert!(hits.needs_probe([Some(b); 3], 0.));
    hits.record(
      VisibleHit {
        owner: MolecularPick::Residue(b),
        position: Vec3::ZERO,
      },
      [Some(b); 3],
    );
    assert_eq!(hits.finish(), [b].into());
    Ok(())
  }

  #[test]
  fn background_snapshot_should_share_geometry_and_survive_scene_updates() -> Result<(), Box<dyn std::error::Error>> {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<BoxSelectionQuery>();
    let scene = two_residue_scene()?;
    let a = scene.atoms[0];
    let mut colors = ColorCache::new(&scene);
    colors.set_atoms(vec![[0., 0., 0.5, 0.1, 1., 1., 1., 1.]]);
    let mut geometry = PickingGeometry {
      atoms: vec![a.atom_id],
      atom_residues: [(a.atom_id, a.residue_id)].into(),
      ..Default::default()
    };
    geometry.rebuild_atom_boxes(&colors);
    let mut colors = Arc::new(colors);
    let mut geometry = Arc::new(geometry);
    let query = BoxSelectionQuery {
      colors: Arc::clone(&colors),
      geometry: Arc::clone(&geometry),
      model_view: Mat4::IDENTITY,
      projection: Mat4::IDENTITY,
      layers: RepresentationLayers::atom(crate::AtomStyle::Sphere),
    };
    assert!(Arc::ptr_eq(&query.colors, &colors) && Arc::ptr_eq(&query.geometry, &geometry));
    Arc::make_mut(&mut colors).set_atoms(vec![[2., 0., 0.5, 0.1, 1., 1., 1., 1.]]);
    Arc::make_mut(&mut geometry).rebuild_atom_boxes(&colors);
    assert_eq!(
      query.select([-0.1, -0.1], [0.1, 0.1], Default::default()),
      [a.atom_id].into_iter().collect()
    );
    assert!(
      geometry
        .foreground_residues_in_rect(
          [-0.1, -0.1],
          [0.1, 0.1],
          Mat4::IDENTITY,
          &colors,
          query.layers,
          ForegroundDepthWindow {
            axis: Vec3::Z,
            thickness: Default::default()
          }
        )
        .is_empty()
    );
    Ok(())
  }

  fn mesh_triangle(x: f32, z: f32) -> [[f32; 9]; 3] {
    [
      [x - 0.4, -0.4, z, 0., 0., -1., 1., 1., 1.],
      [x - 0.4, 0.4, z, 0., 0., -1., 1., 1., 1.],
      [x + 0.4, -0.4, z, 0., 0., -1., 1., 1., 1.],
    ]
  }

  #[test]
  fn box_should_exclude_occluded_residues_but_include_exposed_parts() -> Result<(), Box<dyn std::error::Error>> {
    let scene = two_residue_scene()?;
    let a = scene.atoms[0].residue_id;
    let b = scene.atoms[1].residue_id;
    let mut colors = ColorCache::new(&scene);
    let mut rows = Vec::from(mesh_triangle(0., 0.2));
    rows.extend(mesh_triangle(0., 0.7));
    rows.extend(mesh_triangle(0.6, 0.7));
    colors.set_cartoon(rows, vec![0; 9]);
    let mut geometry = PickingGeometry {
      cartoon_indices: (0..9).collect(),
      cartoon_residues: [Some(a); 3].into_iter().chain([Some(b); 6]).collect(),
      ..Default::default()
    };
    geometry.rebuild_cartoon_boxes(&colors);
    let layers = RepresentationLayers::default().with_polymer(crate::PolymerStyle::Cartoon);
    assert_eq!(
      geometry.residues_in_rect([-0.2, -0.2], [0., 0.], Mat4::IDENTITY, &colors, layers),
      [a].into()
    );
    assert_eq!(
      geometry.residues_in_rect([-1., -1.], [1., 1.], Mat4::IDENTITY, &colors, layers),
      [a, b].into()
    );
    colors.set_surface(Vec::from(mesh_triangle(0., 0.1)), None);
    geometry.surface_indices = vec![0, 1, 2];
    geometry.rebuild_surface_boxes(&colors);
    let layers = layers.with_surface(crate::SurfaceStyle::Solid);
    assert!(
      geometry
        .residues_in_rect([-0.2, -0.2], [0., 0.], Mat4::IDENTITY, &colors, layers)
        .is_empty()
    );
    let mut appearances = layers.appearances();
    appearances[2] = appearances[2].with_opacity(0.).ok_or("opacity")?;
    assert_eq!(
      geometry.residues_in_rect(
        [-0.2, -0.2],
        [0., 0.],
        Mat4::IDENTITY,
        &colors,
        layers.with_appearances(appearances)
      ),
      [a].into()
    );
    Ok(())
  }

  #[test]
  fn atom_box_should_not_select_hidden_atoms() -> Result<(), Box<dyn std::error::Error>> {
    let scene = two_residue_scene()?;
    let mut colors = ColorCache::new(&scene);
    colors.set_atoms(vec![
      [0., 0., 0.3, 0.15, 1., 1., 1., 1.],
      [0., 0., 0.7, 0.05, 1., 1., 1., 1.],
    ]);
    let mut geometry = PickingGeometry {
      atom_residues: scene.atoms.iter().map(|a| (a.atom_id, a.residue_id)).collect(),
      atoms: scene.atoms.iter().map(|a| a.atom_id).collect(),
      ..Default::default()
    };
    geometry.rebuild_atom_boxes(&colors);
    assert_eq!(
      geometry.residues_in_rect(
        [-0.2, -0.2],
        [0.2, 0.2],
        Mat4::IDENTITY,
        &colors,
        RepresentationLayers::atom(crate::AtomStyle::Sphere)
      ),
      [scene.atoms[0].residue_id].into()
    );
    Ok(())
  }

  #[test]
  #[ignore = "release-mode CPU visibility benchmark; no GPU required"]
  fn visible_box_selection_benchmark() -> Result<(), Box<dyn std::error::Error>> {
    let scene = two_residue_scene()?;
    let mut colors = ColorCache::new(&scene);
    let rows = (0..100_000)
      .flat_map(|i| {
        let x = (i % 500) as f32 * 0.004 - 1.;
        let y = (i / 500) as f32 * 0.004 - 0.4;
        mesh_triangle(0., 0.5).map(|mut row| {
          row[0] = row[0] * 0.004 + x;
          row[1] = row[1] * 0.004 + y;
          row
        })
      })
      .collect::<Vec<_>>();
    let len = rows.len();
    colors.set_cartoon(rows, vec![0; len]);
    let mut geometry = PickingGeometry {
      cartoon_indices: (0..len as u32).collect(),
      cartoon_residues: (0..len).map(|i| Some(scene.atoms[(i / 3) % 2].residue_id)).collect(),
      ..Default::default()
    };
    geometry.rebuild_cartoon_boxes(&colors);
    let started = std::time::Instant::now();
    for _ in 0..100 {
      std::hint::black_box(geometry.residues_in_rect(
        [-0.1, -0.1],
        [-0.08, -0.08],
        Mat4::IDENTITY,
        &colors,
        RepresentationLayers::default().with_polymer(crate::PolymerStyle::Cartoon),
      ));
    }
    eprintln!(
      "100000 triangles, 100 visible small-box queries: {:?}",
      started.elapsed()
    );
    let started = std::time::Instant::now();
    std::hint::black_box(geometry.residues_in_rect(
      [-1., -1.],
      [1., 1.],
      Mat4::IDENTITY,
      &colors,
      RepresentationLayers::default().with_polymer(crate::PolymerStyle::Cartoon),
    ));
    eprintln!("100000 triangles, full-box query: {:?}", started.elapsed());
    let started = std::time::Instant::now();
    for _ in 0..100 {
      std::hint::black_box(geometry.foreground_residues_in_rect(
        [-0.1, -0.1],
        [-0.08, -0.08],
        Mat4::IDENTITY,
        &colors,
        RepresentationLayers::default().with_polymer(crate::PolymerStyle::Cartoon),
        ForegroundDepthWindow {
          axis: -Vec3::Z,
          thickness: Default::default(),
        },
      ));
    }
    eprintln!(
      "100000 triangles, 100 foreground small-box queries: {:?}",
      started.elapsed()
    );
    let mut occluder = Vec::from(mesh_triangle(0., 0.1).map(|mut row| {
      row[0] *= 4.;
      row[1] *= 4.;
      row
    }));
    occluder.extend([
      [1.6, 1.6, 0.1, 0., 0., -1., 1., 1., 1.],
      [1.6, -1.6, 0.1, 0., 0., -1., 1., 1., 1.],
      [-1.6, 1.6, 0.1, 0., 0., -1., 1., 1., 1.],
    ]);
    colors.set_surface(occluder, None);
    geometry.surface_indices = (0..6).collect();
    geometry.rebuild_surface_boxes(&colors);
    let started = std::time::Instant::now();
    let selected = geometry.residues_in_rect(
      [-1., -1.],
      [1., 1.],
      Mat4::IDENTITY,
      &colors,
      RepresentationLayers::default()
        .with_polymer(crate::PolymerStyle::Cartoon)
        .with_surface(crate::SurfaceStyle::Solid),
    );
    assert!(selected.is_empty());
    eprintln!("100000 occluded triangles, full-box query: {:?}", started.elapsed());
    Ok(())
  }
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
    let mut geometry = PickingGeometry {
      atoms: vec![atom.atom_id],
      atom_residues: [(atom.atom_id, atom.residue_id)].into(),
      surface_indices: vec![0, 1, 2],
      ..Default::default()
    };
    geometry.rebuild_atom_boxes(&colors);
    geometry.rebuild_surface_boxes(&colors);
    let layers = RepresentationLayers::atom(crate::AtomStyle::Sphere).with_surface(crate::SurfaceStyle::Solid);
    assert_eq!(
      geometry.pick([0., 0.], Mat4::IDENTITY, &colors, &scene, layers),
      MolecularPick::UnmappedSurface
    );
    let mut appearance = layers.appearances();
    appearance[2] = appearance[2].with_opacity(0.35).ok_or("valid opacity")?;
    assert_eq!(
      geometry.pick(
        [0., 0.],
        Mat4::IDENTITY,
        &colors,
        &scene,
        layers.with_appearances(appearance)
      ),
      MolecularPick::UnmappedSurface
    );
    assert!(
      geometry
        .residues_in_rect(
          [-0.05, -0.05],
          [0.05, 0.05],
          Mat4::IDENTITY,
          &colors,
          layers.with_appearances(appearance)
        )
        .is_empty()
    );
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
    let mut artifact = MolecularSurfaceArtifact {
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
    let mut rear_mesh = artifact.domains[0].mesh.clone();
    for vertex in &mut rear_mesh.vertices {
      vertex[2] = 0.7;
    }
    artifact.domains.push(SurfaceDomainArtifact {
      chain_id: Some(a.chain_id),
      mesh: rear_mesh,
    });
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
    geometry.rebuild_atom_boxes(&colors);
    geometry.rebuild_surface_boxes(&colors);
    let layers = RepresentationLayers::atom(crate::AtomStyle::Sphere).with_surface(crate::SurfaceStyle::Solid);
    assert_eq!(
      geometry.pick([0., 0.], Mat4::IDENTITY, &colors, &scene, layers),
      MolecularPick::Residue(b.residue_id)
    );
    assert_eq!(
      geometry.residues_in_rect([-0.2, -0.2], [0.2, 0.2], Mat4::IDENTITY, &colors, layers),
      [b.residue_id].into()
    );
    let mut hidden_front = geometry.clone();
    hidden_front.visibility = Arc::new(crate::visibility::VisibilityMask::new(
      &geometry.atom_residues,
      &[b.atom_id].into(),
    ));
    let draws = crate::visibility::VisibleDraws::new(&hidden_front, &colors);
    assert_eq!(draws.surface, vec![3, 4, 5], "only the rear mesh remains drawable");
    assert_eq!(
      hidden_front.pick([0., 0.], Mat4::IDENTITY, &colors, &scene, layers),
      MolecularPick::Residue(a.residue_id)
    );
    assert_eq!(
      hidden_front.residues_in_rect([-0.2, -0.2], [0.2, 0.2], Mat4::IDENTITY, &colors, layers),
      [a.residue_id].into()
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
