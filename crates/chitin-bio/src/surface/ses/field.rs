//! Regular-grid sampling and signed-distance evaluation for SES construction.

use rayon::prelude::*;

use super::{
  DISTANCE_FIELD_RANGE,
  spatial::{AtomGrid, PointGrid},
};

/// Samples a scalar field at every point of a regular Cartesian grid.
///
/// # Parameters
///
/// * `bounds_min`, `spacing`, and `dimensions` define the sampled grid.
/// * `sample` computes the signed value at one source-space position.
///
/// # Returns
///
/// X-major scalar samples with Z as the outermost coordinate.
pub(super) fn sample_field(
  bounds_min: glam::Vec3,
  spacing: f32,
  dimensions: [usize; 3],
  sample: impl Fn(glam::Vec3) -> f32 + Sync,
) -> Vec<f32> {
  let mut field = vec![0.0; dimensions[0] * dimensions[1] * dimensions[2]];
  let plane_size = dimensions[0] * dimensions[1];
  field.par_chunks_mut(plane_size).enumerate().for_each(|(z, plane)| {
    for y in 0..dimensions[1] {
      for x in 0..dimensions[0] {
        plane[y * dimensions[0] + x] = sample(grid_position(x, y, z, bounds_min, spacing));
      }
    }
  });
  field
}

/// Converts a source-space extent and sample spacing into grid dimensions.
pub(super) fn grid_dimensions(extent: glam::Vec3, spacing: f32) -> [usize; 3] {
  [
    (extent.x / spacing).ceil() as usize + 1,
    (extent.y / spacing).ceil() as usize + 1,
    (extent.z / spacing).ceil() as usize + 1,
  ]
}

/// Converts a three-dimensional grid coordinate into an X-major index.
pub(super) fn grid_index(x: usize, y: usize, z: usize, dimensions: [usize; 3]) -> usize {
  (z * dimensions[1] + y) * dimensions[0] + x
}

/// Converts a grid coordinate into its source-space position.
pub(super) fn grid_position(x: usize, y: usize, z: usize, bounds_min: glam::Vec3, spacing: f32) -> glam::Vec3 {
  bounds_min + glam::Vec3::new(x as f32 * spacing, y as f32 * spacing, z as f32 * spacing)
}

/// Returns the truncated signed distance to the expanded atom union.
pub(super) fn atom_surface_distance(position: glam::Vec3, atom_grid: &AtomGrid, expansion: f32, spacing: f32) -> f32 {
  let limit = DISTANCE_FIELD_RANGE * spacing;
  let mut distance = limit;
  atom_grid.for_each_nearby(position, atom_grid.max_radius + expansion + limit, |atom| {
    distance = distance.min(position.distance(atom.position) - atom.radius - expansion);
  });
  distance
}

/// Returns the truncated signed distance to the union of probe spheres.
pub(super) fn probe_surface_distance(
  position: glam::Vec3,
  probe_grid: &PointGrid,
  probe_radius: f32,
  spacing: f32,
) -> f32 {
  let limit = DISTANCE_FIELD_RANGE * spacing;
  let mut distance = limit;
  probe_grid.for_each_nearby(position, |probe_center| {
    distance = distance.min(position.distance(probe_center) - probe_radius);
  });
  distance
}

/// Fills the SAS exterior into the probe field before inner-surface contouring.
///
/// If `g` is the signed distance to the union of boundary probe spheres and
/// `f` is the first-pass SAS field, `min(g, -f)` is negative both inside the
/// probe shell and throughout the SAS exterior. Its zero contour therefore
/// contains the inner solvent-excluded boundary without the outer offset sheet.
///
/// # Parameters
///
/// * `probe_field` contains signed distances to the boundary probe spheres.
/// * `sas_field` contains signed distances to the expanded atom union.
///
/// # Returns
///
/// A field whose zero contour is the molecular-side probe boundary.
pub(super) fn compose_inner_surface_field(probe_field: &[f32], sas_field: &[f32]) -> Vec<f32> {
  probe_field
    .par_iter()
    .zip(sas_field.par_iter())
    .map(|(probe, sas)| probe.min(-sas))
    .collect()
}

/// Evaluates the gradient of a trilinearly interpolated scalar field.
///
/// Coordinates are clamped to the final grid cell so positions moved slightly
/// by mesh smoothing still receive a stable outward normal.
///
/// # Parameters
///
/// * `position` is the source-space point at which the gradient is evaluated.
/// * `bounds_min`, `spacing`, and `dimensions` describe the scalar grid.
/// * `field` contains the signed grid samples.
///
/// # Returns
///
/// The source-space gradient, pointing toward increasing field values.
pub(super) fn field_gradient(
  position: glam::Vec3,
  bounds_min: glam::Vec3,
  spacing: f32,
  dimensions: [usize; 3],
  field: &[f32],
) -> glam::Vec3 {
  let maximum = glam::Vec3::new(
    dimensions[0].saturating_sub(1) as f32,
    dimensions[1].saturating_sub(1) as f32,
    dimensions[2].saturating_sub(1) as f32,
  );
  let coordinate = ((position - bounds_min) / spacing).clamp(glam::Vec3::ZERO, maximum);
  let x0 = (coordinate.x.floor() as usize).min(dimensions[0] - 2);
  let y0 = (coordinate.y.floor() as usize).min(dimensions[1] - 2);
  let z0 = (coordinate.z.floor() as usize).min(dimensions[2] - 2);
  let fraction = coordinate - glam::Vec3::new(x0 as f32, y0 as f32, z0 as f32);
  let value = |x: usize, y: usize, z: usize| field[grid_index(x, y, z, dimensions)];
  let interpolate = |a: f32, b: f32, factor: f32| a + (b - a) * factor;

  let dx0 = interpolate(
    value(x0 + 1, y0, z0) - value(x0, y0, z0),
    value(x0 + 1, y0 + 1, z0) - value(x0, y0 + 1, z0),
    fraction.y,
  );
  let dx1 = interpolate(
    value(x0 + 1, y0, z0 + 1) - value(x0, y0, z0 + 1),
    value(x0 + 1, y0 + 1, z0 + 1) - value(x0, y0 + 1, z0 + 1),
    fraction.y,
  );
  let dy0 = interpolate(
    value(x0, y0 + 1, z0) - value(x0, y0, z0),
    value(x0 + 1, y0 + 1, z0) - value(x0 + 1, y0, z0),
    fraction.x,
  );
  let dy1 = interpolate(
    value(x0, y0 + 1, z0 + 1) - value(x0, y0, z0 + 1),
    value(x0 + 1, y0 + 1, z0 + 1) - value(x0 + 1, y0, z0 + 1),
    fraction.x,
  );
  let dz0 = interpolate(
    value(x0, y0, z0 + 1) - value(x0, y0, z0),
    value(x0 + 1, y0, z0 + 1) - value(x0 + 1, y0, z0),
    fraction.x,
  );
  let dz1 = interpolate(
    value(x0, y0 + 1, z0 + 1) - value(x0, y0 + 1, z0),
    value(x0 + 1, y0 + 1, z0 + 1) - value(x0 + 1, y0 + 1, z0),
    fraction.x,
  );

  glam::Vec3::new(
    interpolate(dx0, dx1, fraction.z),
    interpolate(dy0, dy1, fraction.z),
    interpolate(dz0, dz1, fraction.y),
  ) / spacing
}
