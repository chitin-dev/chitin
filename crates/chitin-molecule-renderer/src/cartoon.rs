//! CPU tessellation of smooth polymer cartoon ribbons.

use chitin_bio::structure::{PolymerTrace, PolymerTraceKind, StructureScene};

/// Number of interpolated rings generated between adjacent residue centers.
const SAMPLES_PER_RESIDUE: usize = 6;
/// Number of vertices around each rounded ribbon cross-section.
const RING_SIDES: usize = 8;
/// Minimum squared length accepted while constructing local frames.
const FRAME_EPSILON_SQUARED: f32 = 1.0e-8;

/// Indexed triangle mesh generated from all continuous polymer traces.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct CartoonMesh {
  /// Interleaved source position, normal, and linear RGB rows.
  pub(crate) vertices: Vec<[f32; 9]>,
  /// Triangle-list indices into `vertices`.
  pub(crate) indices: Vec<u32>,
}

#[derive(Debug, Clone, Copy)]
struct TraceSample {
  /// Interpolated position on the polymer centerline.
  position: glam::Vec3,
  /// Interpolated backbone guide used when a frame becomes degenerate.
  guide: glam::Vec3,
  /// Cross-section family inherited from the neighboring residues.
  kind: PolymerTraceKind,
  /// Width multiplier used to taper the terminal beta-strand arrow.
  strand_width_scale: f32,
}

/// Tessellates every protein trace into a smooth, oriented cartoon surface.
///
/// # Parameters
///
/// * `scene` supplies renderer-neutral C-alpha centers, guide atoms, and
///   secondary-structure categories.
///
/// # Returns
///
/// A single indexed triangle mesh. Catmull-Rom interpolation smooths the
/// centerline, projected previous normals form a rotation-minimizing frame,
/// and the selected cross-section distinguishes coils, helices, and strands.
pub(crate) fn cartoon_mesh(scene: &StructureScene) -> CartoonMesh {
  let mut mesh = CartoonMesh::default();
  for trace in &scene.polymer_traces {
    append_trace_mesh(&mut mesh, trace);
  }
  mesh
}

/// Appends one chain-continuous ribbon without joining it to prior traces.
fn append_trace_mesh(mesh: &mut CartoonMesh, trace: &PolymerTrace) {
  let samples = sample_trace(trace);
  if samples.len() < 2 {
    return;
  }

  let first_vertex = mesh.vertices.len() as u32;
  let mut previous_normal = initial_normal(samples[0], samples[1]);
  for (index, sample) in samples.iter().copied().enumerate() {
    let tangent = sample_tangent(&samples, index);
    let normal = transported_normal(previous_normal, tangent, sample.guide - sample.position);
    let binormal = tangent.cross(normal).normalize_or_zero();
    previous_normal = normal;
    let (width, thickness, color) = cross_section(sample);

    for side in 0..RING_SIDES {
      let angle = std::f32::consts::TAU * side as f32 / RING_SIDES as f32;
      let radial = normal * (angle.cos() * width) + binormal * (angle.sin() * thickness);
      let surface_normal = (normal * (angle.cos() / width.max(f32::EPSILON))
        + binormal * (angle.sin() / thickness.max(f32::EPSILON)))
      .normalize_or_zero();
      let position = sample.position + radial;
      mesh.vertices.push([
        position.x,
        position.y,
        position.z,
        surface_normal.x,
        surface_normal.y,
        surface_normal.z,
        color[0],
        color[1],
        color[2],
      ]);
    }
  }

  for ring in 0..samples.len() - 1 {
    let current = first_vertex + (ring * RING_SIDES) as u32;
    let next = current + RING_SIDES as u32;
    for side in 0..RING_SIDES {
      let following = (side + 1) % RING_SIDES;
      let a = current + side as u32;
      let b = current + following as u32;
      let c = next + side as u32;
      let d = next + following as u32;
      mesh.indices.extend_from_slice(&[a, c, b, b, c, d]);
    }
  }
}

/// Samples a uniform Catmull-Rom centerline at fixed visual density.
fn sample_trace(trace: &PolymerTrace) -> Vec<TraceSample> {
  let points = &trace.points;
  let mut samples = Vec::with_capacity((points.len() - 1) * SAMPLES_PER_RESIDUE + 1);
  for index in 0..points.len() - 1 {
    let p0 = glam::Vec3::from_array(points[index.saturating_sub(1)].position);
    let p1 = glam::Vec3::from_array(points[index].position);
    let p2 = glam::Vec3::from_array(points[index + 1].position);
    let p3 = glam::Vec3::from_array(points[(index + 2).min(points.len() - 1)].position);
    let g0 = glam::Vec3::from_array(points[index.saturating_sub(1)].guide);
    let g1 = glam::Vec3::from_array(points[index].guide);
    let g2 = glam::Vec3::from_array(points[index + 1].guide);
    let g3 = glam::Vec3::from_array(points[(index + 2).min(points.len() - 1)].guide);
    let terminal_strand_segment = points[index].kind == PolymerTraceKind::Strand
      && points
        .get(index + 2)
        .is_none_or(|point| point.kind != PolymerTraceKind::Strand);
    for step in 0..SAMPLES_PER_RESIDUE {
      let t = step as f32 / SAMPLES_PER_RESIDUE as f32;
      let kind = if terminal_strand_segment || t < 0.5 {
        points[index].kind
      } else {
        points[index + 1].kind
      };
      samples.push(TraceSample {
        position: catmull_rom(p0, p1, p2, p3, t),
        guide: catmull_rom(g0, g1, g2, g3, t),
        kind,
        strand_width_scale: if terminal_strand_segment {
          strand_arrow_scale(t)
        } else {
          1.0
        },
      });
    }
  }
  let last = points[points.len() - 1];
  samples.push(TraceSample {
    position: glam::Vec3::from_array(last.position),
    guide: glam::Vec3::from_array(last.guide),
    kind: last.kind,
    strand_width_scale: if last.kind == PolymerTraceKind::Strand {
      0.08
    } else {
      1.0
    },
  });
  samples
}

/// Evaluates a uniform Catmull-Rom segment between `p1` and `p2`.
fn catmull_rom(p0: glam::Vec3, p1: glam::Vec3, p2: glam::Vec3, p3: glam::Vec3, t: f32) -> glam::Vec3 {
  let t2 = t * t;
  let t3 = t2 * t;
  0.5
    * ((2.0 * p1) + (-p0 + p2) * t + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2 + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3)
}

/// Returns a stable tangent from neighboring centerline samples.
fn sample_tangent(samples: &[TraceSample], index: usize) -> glam::Vec3 {
  let before = samples[index.saturating_sub(1)].position;
  let after = samples[(index + 1).min(samples.len() - 1)].position;
  (after - before).normalize_or(glam::Vec3::Z)
}

/// Chooses the first ribbon normal from the guide atom and centerline tangent.
fn initial_normal(first: TraceSample, second: TraceSample) -> glam::Vec3 {
  let tangent = (second.position - first.position).normalize_or(glam::Vec3::Z);
  let projected_guide = first.guide - first.position - tangent * (first.guide - first.position).dot(tangent);
  if projected_guide.length_squared() > FRAME_EPSILON_SQUARED {
    projected_guide.normalize()
  } else {
    tangent.any_orthonormal_vector()
  }
}

/// Projects the previous normal onto a new tangent plane to minimize ribbon twist.
fn transported_normal(previous: glam::Vec3, tangent: glam::Vec3, guide: glam::Vec3) -> glam::Vec3 {
  let projected_previous = previous - tangent * previous.dot(tangent);
  let projected_guide = guide - tangent * guide.dot(tangent);
  if projected_previous.length_squared() > FRAME_EPSILON_SQUARED {
    projected_previous.normalize()
  } else if projected_guide.length_squared() > FRAME_EPSILON_SQUARED {
    projected_guide.normalize()
  } else {
    tangent.any_orthonormal_vector()
  }
}

/// Widens the shoulder and tapers the tip of a terminal beta-strand arrow.
fn strand_arrow_scale(t: f32) -> f32 {
  if t < 0.55 {
    1.0 + (1.35 - 1.0) * (t / 0.55)
  } else {
    1.35 + (0.08 - 1.35) * ((t - 0.55) / 0.45)
  }
}

/// Returns half-width, half-thickness, and color for one sampled section.
fn cross_section(sample: TraceSample) -> (f32, f32, [f32; 3]) {
  match sample.kind {
    PolymerTraceKind::Coil => (0.18, 0.18, [0.55, 0.67, 0.78]),
    PolymerTraceKind::Helix => (0.62, 0.16, [0.88, 0.24, 0.38]),
    PolymerTraceKind::Strand => (0.72 * sample.strand_width_scale, 0.09, [0.96, 0.72, 0.16]),
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use chitin_bio::structure::{PdbParser, StructureScene};

  #[test]
  fn cartoon_mesh_should_be_finite_and_indexed() {
    let pdb = b"ATOM      1  CA  GLY A   1       0.000   0.000   0.000  1.00 10.00           C  \nATOM      2  O   GLY A   1       0.000   1.000   0.000  1.00 10.00           O  \nATOM      3  CA  ALA A   2       3.800   0.300   0.000  1.00 10.00           C  \nATOM      4  O   ALA A   2       3.800   1.300   0.000  1.00 10.00           O  \nATOM      5  CA  SER A   3       7.500   0.000   0.400  1.00 10.00           C  \nATOM      6  O   SER A   3       7.500   1.000   0.400  1.00 10.00           O  \nEND\n";
    let parsed = PdbParser::new()
      .parse_bytes(pdb)
      .unwrap_or_else(|error| panic!("cartoon fixture should parse: {error}"));
    let scene = StructureScene::from_first_model(&parsed.structure)
      .unwrap_or_else(|error| panic!("cartoon fixture should produce a scene: {error}"));
    let mesh = cartoon_mesh(&scene);

    assert!(!mesh.vertices.is_empty());
    assert!(!mesh.indices.is_empty());
    assert!(mesh.vertices.iter().flatten().all(|component| component.is_finite()));
    assert!(mesh.indices.iter().all(|index| (*index as usize) < mesh.vertices.len()));
  }
}
