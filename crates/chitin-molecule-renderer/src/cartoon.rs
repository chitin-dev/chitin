//! CPU tessellation of smooth polymer cartoon ribbons.

use chitin_bio::structure::{PolymerTrace, PolymerTraceKind, StructureScene};

/// Default number of interpolated rings per residue.
const SAMPLES_PER_RESIDUE: usize = 20;
/// Number of vertices around each round cross-section.
const RING_SIDES: usize = 24;
/// Log target used for cartoon frame diagnostics.
const CARTOON_LOG_TARGET: &str = "chitin_molecule_renderer::cartoon";
/// Minimum squared length accepted while constructing local frames.
const FRAME_EPSILON_SQUARED: f32 = 1.0e-8;
/// Half of the default 2.0-ångström ribbon width.
pub(crate) const CARTOON_HALF_WIDTH: f32 = 1.0;
/// Half of the default 0.4-ångström ribbon thickness.
const CARTOON_HALF_THICKNESS: f32 = 0.2;
/// Width multiplier at the base of a terminal beta-strand arrow.
const STRAND_ARROW_SCALE: f32 = 2.0;
/// Width multiplier at the tip of a terminal beta-strand arrow.
const STRAND_ARROW_TIP_SCALE: f32 = 0.2;
/// Lighting profile applied around a cartoon cross-section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CrossSectionStyle {
  /// Smooth twenty-four-sided ellipse used for helices and coils.
  Round,
  /// Four flat faces used for beta sheets.
  Square,
}

/// Dimensions, profile, and color evaluated for one centerline sample.
#[derive(Debug, Clone, Copy, PartialEq)]
struct CrossSection {
  /// Distance from the centerline to either ribbon edge.
  half_width: f32,
  /// Distance from the centerline to either ribbon face.
  half_thickness: f32,
  /// Shape and normal policy around the cross-section.
  style: CrossSectionStyle,
  /// Linear RGB color supplied to the cartoon shader.
  color: [f32; 3],
}

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

/// Orthonormal basis used to place one ribbon cross-section.
#[derive(Debug, Clone, Copy)]
struct RibbonFrame {
  /// Unit direction along the sampled centerline.
  tangent: glam::Vec3,
  /// Unit direction from one ribbon edge to the other.
  width: glam::Vec3,
  /// Unit normal of the broad ribbon face.
  face: glam::Vec3,
}

/// Tessellates every protein trace into a smooth, oriented cartoon surface.
///
/// # Parameters
///
/// * `scene` supplies renderer-neutral C-alpha centers, guide atoms, and
///   secondary-structure categories.
/// * `color` is the linear RGB carbon palette color shared with stick mode.
///
/// # Returns
///
/// A single indexed triangle mesh. Catmull-Rom interpolation smooths the
/// centerline, parallel transport keeps neighboring rings coherent, and the
/// selected cross-section distinguishes coils, helices, and strands.
pub(crate) fn cartoon_mesh(scene: &StructureScene, color: [f32; 3]) -> CartoonMesh {
  if log::log_enabled!(target: CARTOON_LOG_TARGET, log::Level::Debug) {
    let helix_residues = scene
      .polymer_traces
      .iter()
      .flat_map(|trace| trace.points.iter())
      .filter(|point| point.kind == PolymerTraceKind::Helix)
      .count();
    log::debug!(
      target: CARTOON_LOG_TARGET,
      "cartoon tessellation: traces={}, helix_residues={}, samples_per_residue={}, ring_sides={}, helix_half_width={:.3} A, helix_half_thickness={:.3} A",
      scene.polymer_traces.len(),
      helix_residues,
      SAMPLES_PER_RESIDUE,
      RING_SIDES,
      CARTOON_HALF_WIDTH,
      CARTOON_HALF_THICKNESS,
    );
  }
  let mut mesh = CartoonMesh::default();
  for trace in &scene.polymer_traces {
    append_trace_mesh(&mut mesh, trace, color);
  }
  mesh
}

/// Appends one chain-continuous ribbon without joining it to prior traces.
fn append_trace_mesh(mesh: &mut CartoonMesh, trace: &PolymerTrace, color: [f32; 3]) {
  let samples = sample_trace(trace);
  if samples.len() < 2 {
    return;
  }

  let first_vertex = mesh.vertices.len() as u32;
  let frames = ribbon_frames(trace, &samples);
  for (sample, frame) in samples.iter().copied().zip(frames) {
    let section = cross_section(sample, color);

    for side in 0..RING_SIDES {
      let [width_factor, thickness_factor, width_normal, thickness_normal] = section_vertex(section, side);
      let radial =
        frame.width * (width_factor * section.half_width) + frame.face * (thickness_factor * section.half_thickness);
      let surface_normal = (frame.width * width_normal + frame.face * thickness_normal).normalize_or_zero();
      let position = sample.position + radial;
      mesh.vertices.push([
        position.x,
        position.y,
        position.z,
        surface_normal.x,
        surface_normal.y,
        surface_normal.z,
        section.color[0],
        section.color[1],
        section.color[2],
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
      && points[index + 1].kind == PolymerTraceKind::Strand
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
      STRAND_ARROW_TIP_SCALE
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

/// Builds continuous frames and aligns each complete alpha-helix segment.
///
/// # Parameters
///
/// * `trace` supplies residue-level control points and secondary structure.
/// * `samples` supplies the interpolated centerline used by the final mesh.
///
/// # Returns
///
/// One orthonormal frame per sample. General polymer sections use parallel
/// transport from the backbone guide, while alpha-helical sections distribute
/// the twist between smoothed inward-facing residue anchors over each segment.
fn ribbon_frames(trace: &PolymerTrace, samples: &[TraceSample]) -> Vec<RibbonFrame> {
  let tangents: Vec<_> = (0..samples.len()).map(|index| sample_tangent(samples, index)).collect();
  let mut frames = Vec::with_capacity(samples.len());
  let mut previous_width = initial_width(samples[0], samples[1]);
  let mut previous_tangent = tangents[0];

  for (index, (&tangent, sample)) in tangents.iter().zip(samples).enumerate() {
    let guide = sample.guide - sample.position;
    let width = if index == 0 {
      project_unit_normal(previous_width, tangent)
        .or_else(|| project_unit_normal(guide, tangent))
        .unwrap_or_else(|| tangent.any_orthonormal_vector())
    } else {
      parallel_transport_normal(previous_width, previous_tangent, tangent)
        .or_else(|| project_unit_normal(guide, tangent))
        .unwrap_or_else(|| tangent.any_orthonormal_vector())
    };
    let face = tangent.cross(width).normalize_or(tangent.any_orthonormal_vector());
    frames.push(RibbonFrame { tangent, width, face });
    previous_width = width;
    previous_tangent = tangent;
  }

  align_helix_frames(trace, &mut frames);
  frames
}

/// Chooses the first ribbon width axis from the guide atom and centerline tangent.
fn initial_width(first: TraceSample, second: TraceSample) -> glam::Vec3 {
  let tangent = (second.position - first.position).normalize_or(glam::Vec3::Z);
  let projected_guide = first.guide - first.position - tangent * (first.guide - first.position).dot(tangent);
  if projected_guide.length_squared() > FRAME_EPSILON_SQUARED {
    projected_guide.normalize()
  } else {
    tangent.any_orthonormal_vector()
  }
}

/// Projects and normalizes a vector in the plane perpendicular to a tangent.
fn project_unit_normal(vector: glam::Vec3, tangent: glam::Vec3) -> Option<glam::Vec3> {
  let projected = vector - tangent * vector.dot(tangent);
  (projected.length_squared() > FRAME_EPSILON_SQUARED).then(|| projected.normalize())
}

/// Parallel-transports a normal between neighboring centerline tangents.
fn parallel_transport_normal(
  normal: glam::Vec3,
  previous_tangent: glam::Vec3,
  tangent: glam::Vec3,
) -> Option<glam::Vec3> {
  let axis = previous_tangent.cross(tangent);
  let axis_length = axis.length();
  let tangent_dot = previous_tangent.dot(tangent).clamp(-1.0, 1.0);
  let transported = if axis_length * axis_length > FRAME_EPSILON_SQUARED {
    let rotation = glam::Quat::from_axis_angle(axis / axis_length, axis_length.atan2(tangent_dot));
    rotation * normal
  } else if tangent_dot >= 0.0 {
    normal
  } else {
    return None;
  };
  project_unit_normal(transported, tangent)
}

/// Aligns complete alpha-helix segments to smoothed residue-level face normals.
fn align_helix_frames(trace: &PolymerTrace, frames: &mut [RibbonFrame]) {
  let control_normals = helix_control_width_normals(trace);
  for segment in 0..trace.points.len() - 1 {
    if trace.points[segment].kind != PolymerTraceKind::Helix
      || trace.points[segment + 1].kind != PolymerTraceKind::Helix
    {
      continue;
    }
    let (Some(start_width), Some(end_width)) = (control_normals[segment], control_normals[segment + 1]) else {
      continue;
    };
    let start = segment * SAMPLES_PER_RESIDUE;
    let end = (segment + 1) * SAMPLES_PER_RESIDUE;
    if let Some(twist) = smooth_helix_segment(&mut frames[start..=end], start_width, end_width) {
      log::debug!(
        target: CARTOON_LOG_TARGET,
        "cartoon helix frame: control_segment={}..{}, sample_segment={}..={}, start_width={start_width:?}, end_width={end_width:?}, distributed_width_twist={:.2} deg",
        segment,
        segment + 1,
        start,
        end,
        twist.to_degrees(),
      );
    }
  }
}

/// Computes, fills, and smooths path-plane width axes at helical residues.
///
/// The control-plane direction is derived from the cross product of neighboring
/// path segments.
/// The broad face points inward after crossing the tangent with this axis.
/// This avoids treating the noisy second derivative of interpolated C-alpha
/// points as a direct frame rotation.
fn helix_control_width_normals(trace: &PolymerTrace) -> Vec<Option<glam::Vec3>> {
  let mut normals = vec![None; trace.points.len()];
  for (index, normal) in normals
    .iter_mut()
    .enumerate()
    .take(trace.points.len().saturating_sub(1))
    .skip(1)
  {
    if trace.points[index].kind != PolymerTraceKind::Helix {
      continue;
    }
    let before = glam::Vec3::from_array(trace.points[index - 1].position);
    let position = glam::Vec3::from_array(trace.points[index].position);
    let after = glam::Vec3::from_array(trace.points[index + 1].position);
    let incoming = (position - before).normalize_or_zero();
    let outgoing = (after - position).normalize_or_zero();
    let tangent = (after - before).normalize_or(glam::Vec3::Z);
    let Some(mut width) = project_unit_normal(outgoing.cross(incoming), tangent) else {
      continue;
    };
    let inward = project_unit_normal(outgoing - incoming, tangent);
    if inward.is_some_and(|face| tangent.cross(width).dot(face) < 0.0) {
      width = -width;
    }
    *normal = Some(width);
  }

  let mut start = 0;
  while start < trace.points.len() {
    if trace.points[start].kind != PolymerTraceKind::Helix {
      start += 1;
      continue;
    }
    let mut end = start + 1;
    while end < trace.points.len() && trace.points[end].kind == PolymerTraceKind::Helix {
      end += 1;
    }
    smooth_helix_control_run(trace, &mut normals, start..end);
    start = end;
  }
  normals
}

/// Fills missing endpoints and smooths one contiguous residue-level helix run.
fn smooth_helix_control_run(trace: &PolymerTrace, normals: &mut [Option<glam::Vec3>], range: std::ops::Range<usize>) {
  let Some(first_valid) = range.clone().find(|&index| normals[index].is_some()) else {
    return;
  };
  let first_normal = normals[first_valid].unwrap_or(glam::Vec3::ZERO);
  normals[range.start..first_valid].fill(Some(first_normal));

  let mut previous_valid = first_valid;
  for index in first_valid + 1..range.end {
    let Some(mut next_normal) = normals[index] else {
      continue;
    };
    let previous_normal = normals[previous_valid].unwrap_or(first_normal);
    if previous_normal.dot(next_normal) < 0.0 {
      next_normal = -next_normal;
      normals[index] = Some(next_normal);
    }
    let gap = index - previous_valid;
    for offset in 1..gap {
      let fraction = offset as f32 / gap as f32;
      normals[previous_valid + offset] = Some(previous_normal.lerp(next_normal, fraction).normalize());
    }
    previous_valid = index;
  }
  let last_normal = normals[previous_valid].unwrap_or(first_normal);
  normals[previous_valid + 1..range.end].fill(Some(last_normal));

  let unsmoothed = normals[range.clone()].to_vec();
  for index in range.clone() {
    let local = index - range.start;
    let current = unsmoothed[local].unwrap_or(first_normal);
    let before = unsmoothed[local.saturating_sub(1)].unwrap_or(current);
    let after = unsmoothed[(local + 1).min(unsmoothed.len() - 1)].unwrap_or(current);
    let tangent = control_tangent(trace, index);
    normals[index] = project_unit_normal(before + current * 2.0 + after, tangent).or(Some(current));
  }
}

/// Returns the centerline tangent at one residue control point.
fn control_tangent(trace: &PolymerTrace, index: usize) -> glam::Vec3 {
  let before = glam::Vec3::from_array(trace.points[index.saturating_sub(1)].position);
  let after = glam::Vec3::from_array(trace.points[(index + 1).min(trace.points.len() - 1)].position);
  (after - before).normalize_or(glam::Vec3::Z)
}

/// Parallel-transports one helix segment and smoothly meets its ending width anchor.
fn smooth_helix_segment(frames: &mut [RibbonFrame], start_width: glam::Vec3, end_width: glam::Vec3) -> Option<f32> {
  let first = frames.first().copied()?;
  let last = frames.last().copied()?;
  let start_width = project_unit_normal(start_width, first.tangent)?;
  let end_width = project_unit_normal(end_width, last.tangent)?;

  let mut transported = Vec::with_capacity(frames.len());
  transported.push(start_width);
  for index in 1..frames.len() {
    let width = parallel_transport_normal(transported[index - 1], frames[index - 1].tangent, frames[index].tangent)
      .unwrap_or(transported[index - 1]);
    transported.push(width);
  }

  let end_transport = transported.last().copied().unwrap_or(start_width);
  let twist = signed_angle_around(end_transport, end_width, last.tangent);
  let denominator = frames.len().saturating_sub(1).max(1) as f32;
  for (index, (frame, transported_face)) in frames.iter_mut().zip(transported).enumerate() {
    let fraction = index as f32 / denominator;
    let smooth_fraction = fraction * fraction * (3.0 - 2.0 * fraction);
    let rotation = glam::Quat::from_axis_angle(frame.tangent, twist * smooth_fraction);
    let width = project_unit_normal(rotation * transported_face, frame.tangent).unwrap_or(transported_face);
    frame.width = width;
    frame.face = frame.tangent.cross(width).normalize_or(frame.face);
  }
  Some(twist)
}

/// Returns the signed rotation from one normal to another around a tangent.
fn signed_angle_around(from: glam::Vec3, to: glam::Vec3, tangent: glam::Vec3) -> f32 {
  tangent.dot(from.cross(to)).atan2(from.dot(to).clamp(-1.0, 1.0))
}

/// Tapers a terminal beta-strand from the wide arrow base to its narrow tip.
fn strand_arrow_scale(t: f32) -> f32 {
  STRAND_ARROW_SCALE * (1.0 - t) + STRAND_ARROW_TIP_SCALE * t
}

/// Returns the dimensions, profile, and color for one sampled section.
fn cross_section(sample: TraceSample, color: [f32; 3]) -> CrossSection {
  match sample.kind {
    PolymerTraceKind::Coil => CrossSection {
      half_width: CARTOON_HALF_THICKNESS,
      half_thickness: CARTOON_HALF_THICKNESS,
      style: CrossSectionStyle::Round,
      color,
    },
    PolymerTraceKind::Helix => CrossSection {
      half_width: CARTOON_HALF_WIDTH,
      half_thickness: CARTOON_HALF_THICKNESS,
      style: CrossSectionStyle::Round,
      color,
    },
    PolymerTraceKind::Strand => CrossSection {
      half_width: CARTOON_HALF_WIDTH * sample.strand_width_scale,
      half_thickness: CARTOON_HALF_THICKNESS,
      style: CrossSectionStyle::Square,
      color,
    },
  }
}

/// Returns local position factors and a profile normal for one ring vertex.
fn section_vertex(section: CrossSection, side: usize) -> [f32; 4] {
  match section.style {
    CrossSectionStyle::Round => {
      let angle = std::f32::consts::TAU * side as f32 / RING_SIDES as f32;
      [
        angle.cos(),
        angle.sin(),
        angle.cos() / section.half_width.max(f32::EPSILON),
        angle.sin() / section.half_thickness.max(f32::EPSILON),
      ]
    }
    CrossSectionStyle::Square => {
      const FACE_COUNT: usize = 4;
      const VERTICES_PER_FACE: usize = RING_SIDES / FACE_COUNT;
      let face = side / VERTICES_PER_FACE;
      let fraction = (side % VERTICES_PER_FACE) as f32 / (VERTICES_PER_FACE - 1) as f32;
      match face {
        0 => [1.0 - 2.0 * fraction, 1.0, 0.0, 1.0],
        1 => [-1.0, 1.0 - 2.0 * fraction, -1.0, 0.0],
        2 => [-1.0 + 2.0 * fraction, -1.0, 0.0, -1.0],
        _ => [1.0, -1.0 + 2.0 * fraction, 1.0, 0.0],
      }
    }
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
    let mesh = cartoon_mesh(&scene, [0.76, 0.71, 0.62]);

    assert!(!mesh.vertices.is_empty());
    assert!(!mesh.indices.is_empty());
    assert_eq!(&mesh.vertices[0][6..9], &[0.76, 0.71, 0.62]);
    assert!(mesh.vertices.iter().flatten().all(|component| component.is_finite()));
    assert!(mesh.indices.iter().all(|index| (*index as usize) < mesh.vertices.len()));
  }

  #[test]
  fn default_sections_should_match_default_dimensions() {
    let sample = TraceSample {
      position: glam::Vec3::ZERO,
      guide: glam::Vec3::Y,
      kind: PolymerTraceKind::Coil,
      strand_width_scale: 1.0,
    };

    let color = [0.76, 0.71, 0.62];
    let coil = cross_section(sample, color);
    let helix = cross_section(
      TraceSample {
        kind: PolymerTraceKind::Helix,
        ..sample
      },
      color,
    );
    let strand = cross_section(
      TraceSample {
        kind: PolymerTraceKind::Strand,
        ..sample
      },
      color,
    );

    assert_eq!(
      (
        coil.half_width,
        helix.half_width,
        strand.half_width,
        strand.half_thickness
      ),
      (
        CARTOON_HALF_THICKNESS,
        CARTOON_HALF_WIDTH,
        CARTOON_HALF_WIDTH,
        CARTOON_HALF_THICKNESS,
      )
    );
  }

  #[test]
  fn default_sections_should_use_default_profiles() {
    let sample = TraceSample {
      position: glam::Vec3::ZERO,
      guide: glam::Vec3::Y,
      kind: PolymerTraceKind::Helix,
      strand_width_scale: 1.0,
    };
    let color = [0.76, 0.71, 0.62];

    assert_eq!(
      (
        cross_section(sample, color).style,
        cross_section(
          TraceSample {
            kind: PolymerTraceKind::Coil,
            ..sample
          },
          color
        )
        .style,
        cross_section(
          TraceSample {
            kind: PolymerTraceKind::Strand,
            ..sample
          },
          color
        )
        .style,
      ),
      (
        CrossSectionStyle::Round,
        CrossSectionStyle::Round,
        CrossSectionStyle::Square
      )
    );
  }

  #[test]
  fn strand_arrow_should_taper_from_wide_base_to_narrow_tip() {
    assert_eq!(
      (strand_arrow_scale(0.0), strand_arrow_scale(1.0)),
      (STRAND_ARROW_SCALE, STRAND_ARROW_TIP_SCALE)
    );
  }

  #[test]
  fn helix_face_normal_should_follow_the_inside_of_a_turn() {
    let pdb = b"ATOM      1  CA  GLY A   1       1.000   0.000   0.000  1.00 10.00           C  \nATOM      2  O   GLY A   1       1.000   1.000   0.000  1.00 10.00           O  \nATOM      3  CA  ALA A   2       0.000   1.000   0.000  1.00 10.00           C  \nATOM      4  O   ALA A   2       0.000   2.000   0.000  1.00 10.00           O  \nATOM      5  CA  SER A   3      -1.000   0.000   0.000  1.00 10.00           C  \nATOM      6  O   SER A   3      -1.000   1.000   0.000  1.00 10.00           O  \nEND\n";
    let parsed = PdbParser::new()
      .parse_bytes(pdb)
      .unwrap_or_else(|error| panic!("helix fixture should parse: {error}"));
    let mut scene = StructureScene::from_first_model(&parsed.structure)
      .unwrap_or_else(|error| panic!("helix fixture should produce a scene: {error}"));
    let trace = &mut scene.polymer_traces[0];
    for point in &mut trace.points {
      point.kind = PolymerTraceKind::Helix;
    }

    let normals = helix_control_width_normals(trace);
    let Some(width) = normals[1] else {
      panic!("curved helix should have a control-point width axis")
    };
    let tangent = control_tangent(trace, 1);
    let face = tangent.cross(width);

    assert!(face.dot(glam::Vec3::NEG_Y) > 0.99);
  }

  #[test]
  fn smooth_helix_segment_should_distribute_twist_without_frame_jumps() {
    let mut frames = (0..=SAMPLES_PER_RESIDUE)
      .map(|_| RibbonFrame {
        tangent: glam::Vec3::Z,
        width: glam::Vec3::Y,
        face: glam::Vec3::X,
      })
      .collect::<Vec<_>>();

    assert!(smooth_helix_segment(&mut frames, glam::Vec3::X, glam::Vec3::Y).is_some());

    let minimum_alignment = frames
      .windows(2)
      .map(|pair| pair[0].face.dot(pair[1].face))
      .fold(1.0_f32, f32::min);
    assert!(
      minimum_alignment > 0.99,
      "adjacent face normals aligned by {minimum_alignment}"
    );
  }

  #[test]
  fn smooth_helix_segment_should_reach_the_ending_anchor() {
    let mut frames = (0..=SAMPLES_PER_RESIDUE)
      .map(|_| RibbonFrame {
        tangent: glam::Vec3::Z,
        width: glam::Vec3::Y,
        face: glam::Vec3::X,
      })
      .collect::<Vec<_>>();

    assert!(smooth_helix_segment(&mut frames, glam::Vec3::X, glam::Vec3::Y).is_some());

    let Some(last) = frames.last() else {
      panic!("helix fixture should contain frames")
    };
    assert!(last.face.dot(glam::Vec3::NEG_X) > 0.99);
  }
}
