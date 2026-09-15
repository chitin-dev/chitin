//! Numerical and behavioral tests for the sampled-grid SES pipeline.

use super::*;
use crate::structure::{PdbParser, StructureScene};

fn one_atom_scene() -> StructureScene {
  let parsed = PdbParser::new()
    .parse_bytes(b"ATOM      1  C   GLY A   1       0.000   0.000   0.000  1.00 10.00           C  \nEND\n")
    .unwrap_or_else(|error| panic!("SES fixture should parse: {error}"));
  StructureScene::from_first_model(&parsed.structure)
    .unwrap_or_else(|error| panic!("SES fixture should produce a scene: {error}"))
}

/// Generates the first domain mesh used by focused numerical tests.
fn default_mesh(scene: &StructureScene) -> SurfaceMesh {
  generate_molecular_surface(scene, MolecularSurfaceRequest::default())
    .domains
    .into_iter()
    .next()
    .map_or_else(SurfaceMesh::default, |domain| domain.mesh)
}

/// Counts edges that do not have exactly two incident triangles.
fn invalid_edge_count(mesh: &SurfaceMesh) -> usize {
  let mut edge_use_counts = HashMap::new();
  for triangle in mesh.indices.chunks_exact(3) {
    for [first, second] in [
      [triangle[0], triangle[1]],
      [triangle[1], triangle[2]],
      [triangle[2], triangle[0]],
    ] {
      let edge = if first < second {
        (first, second)
      } else {
        (second, first)
      };
      *edge_use_counts.entry(edge).or_insert(0_u8) += 1;
    }
  }
  edge_use_counts.values().filter(|count| **count != 2).count()
}

#[test]
fn ses_mesh_should_generate_triangles_for_one_atom() {
  let mesh = default_mesh(&one_atom_scene());
  assert!(!mesh.indices.is_empty());
}

#[test]
fn ses_mesh_should_contain_only_finite_vertices() {
  let mesh = default_mesh(&one_atom_scene());
  assert!(
    mesh
      .vertices
      .iter()
      .all(|vertex| vertex.iter().all(|value| value.is_finite()))
  );
}

#[test]
fn ses_mesh_should_not_contain_grid_scale_spikes() {
  let mesh = default_mesh(&one_atom_scene());
  let longest_edge = mesh
    .indices
    .chunks_exact(3)
    .flat_map(|triangle| {
      [
        [triangle[0], triangle[1]],
        [triangle[1], triangle[2]],
        [triangle[2], triangle[0]],
      ]
    })
    .map(|edge| {
      vertex_position(&mesh.vertices[edge[0] as usize]).distance(vertex_position(&mesh.vertices[edge[1] as usize]))
    })
    .fold(0.0, f32::max);
  assert!(
    longest_edge <= 3.0 * SesParameters::default().grid_spacing(),
    "surface edge {longest_edge} exceeds the contour-cell scale"
  );
}

#[test]
fn ses_mesh_should_be_watertight_for_one_atom() {
  let mesh = default_mesh(&one_atom_scene());
  assert_eq!(
    invalid_edge_count(&mesh),
    0,
    "surface contains open or non-manifold edges"
  );
}

#[test]
fn contour_should_preserve_sliver_triangles_near_grid_samples() {
  let bounds_min = glam::Vec3::splat(-2.0);
  let spacing = 1.0;
  let dimensions = [5, 5, 5];
  let field = sample_field(bounds_min, spacing, dimensions, |position| {
    position.length() - (1.0 + 1.0e-5)
  });

  let mesh = contour_field(bounds_min, spacing, dimensions, &field);

  assert!(!mesh.indices.is_empty());
  assert_eq!(
    invalid_edge_count(&mesh),
    0,
    "discarding contour slivers opened the mesh"
  );
}

#[test]
fn ses_mesh_should_use_outward_distance_field_normals() {
  let mesh = default_mesh(&one_atom_scene());
  let minimum_alignment = mesh
    .vertices
    .iter()
    .map(|vertex| {
      let position = vertex_position(vertex).normalize_or_zero();
      let normal = glam::Vec3::new(vertex[3], vertex[4], vertex[5]);
      position.dot(normal)
    })
    .fold(1.0, f32::min);
  assert!(minimum_alignment > 0.0, "surface contains inward-facing normals");
}

#[test]
fn ses_mesh_winding_should_agree_with_vertex_normals() {
  let mesh = default_mesh(&one_atom_scene());
  let inconsistent_triangle_count = mesh
    .indices
    .chunks_exact(3)
    .filter(|triangle| {
      let [a, b, c] = [triangle[0] as usize, triangle[1] as usize, triangle[2] as usize];
      let face_normal = (vertex_position(&mesh.vertices[b]) - vertex_position(&mesh.vertices[a]))
        .cross(vertex_position(&mesh.vertices[c]) - vertex_position(&mesh.vertices[a]));
      let vertex_normal = [a, b, c]
        .into_iter()
        .map(|index| glam::Vec3::from_slice(&mesh.vertices[index][3..6]))
        .fold(glam::Vec3::ZERO, |sum, normal| sum + normal);
      face_normal.dot(vertex_normal) <= 0.0
    })
    .count();
  assert_eq!(
    inconsistent_triangle_count, 0,
    "surface contains triangles opposite to their outward normals"
  );
}

#[test]
fn ses_mesh_should_ignore_solvent_atoms() {
  let parsed = PdbParser::new()
    .parse_bytes(b"HETATM    1  O   HOH A   1       0.000   0.000   0.000  1.00 10.00           O  \nEND\n")
    .unwrap_or_else(|error| panic!("solvent fixture should parse: {error}"));
  let scene = StructureScene::from_first_model(&parsed.structure)
    .unwrap_or_else(|error| panic!("solvent fixture should produce a scene: {error}"));
  let mesh = default_mesh(&scene);
  assert!(mesh.vertices.is_empty());
}

#[test]
fn ses_mesh_should_keep_chain_surfaces_independent() {
  let pdb = b"ATOM      1  CA  GLY A   1       0.000   0.000   0.000  1.00 10.00           C  \nATOM      2  CA  GLY B   1       0.000   0.000   0.000  1.00 10.00           C  \nEND\n";
  let parsed = PdbParser::new()
    .parse_bytes(pdb)
    .unwrap_or_else(|error| panic!("multi-chain fixture should parse: {error}"));
  let scene = StructureScene::from_first_model(&parsed.structure)
    .unwrap_or_else(|error| panic!("multi-chain fixture should produce a scene: {error}"));

  let surface = generate_molecular_surface(&scene, MolecularSurfaceRequest::default());

  assert_eq!(surface.domains.len(), 2);
}

#[test]
fn parallel_surface_generation_should_preserve_domain_order() {
  let pdb = b"ATOM      1  CA  GLY B   1       0.000   0.000   0.000  1.00 10.00           C  \nATOM      2  CA  GLY A   1      10.000   0.000   0.000  1.00 10.00           C  \nEND\n";
  let parsed = PdbParser::new()
    .parse_bytes(pdb)
    .unwrap_or_else(|error| panic!("multi-chain fixture should parse: {error}"));
  let scene = StructureScene::from_first_model(&parsed.structure)
    .unwrap_or_else(|error| panic!("multi-chain fixture should produce a scene: {error}"));

  let surface = generate_molecular_surface(&scene, MolecularSurfaceRequest::default());
  let domain_chain_ids = surface
    .domains
    .iter()
    .filter_map(|domain| domain.chain_id)
    .collect::<Vec<_>>();
  let scene_chain_ids = scene.atoms.iter().map(|atom| atom.chain_id).collect::<Vec<_>>();

  assert_eq!(domain_chain_ids, scene_chain_ids);
}

#[test]
fn parallel_field_sampling_should_preserve_row_major_order() {
  let dimensions = [3, 2, 2];
  let field = sample_field(glam::Vec3::ZERO, 1.0, dimensions, |position| {
    position.x + 10.0 * position.y + 100.0 * position.z
  });

  assert_eq!(
    field,
    vec![
      0.0, 1.0, 2.0, 10.0, 11.0, 12.0, 100.0, 101.0, 102.0, 110.0, 111.0, 112.0
    ]
  );
}

#[test]
fn inner_surface_field_should_fill_the_sas_exterior() {
  let probe_field = vec![1.0, -0.5, 1.0];
  let sas_field = vec![-1.0, 0.5, 1.0];

  let field = field::compose_inner_surface_field(&probe_field, &sas_field);

  assert_eq!(field, vec![1.0, -0.5, -1.0]);
}

#[test]
fn ses_mesh_should_exclude_hetero_residues_from_polymer_surfaces() {
  let pdb = b"ATOM      1  CA  GLY A   1       0.000   0.000   0.000  1.00 10.00           C  \nHETATM    2  C1  LIG A   2      20.000   0.000   0.000  1.00 10.00           C  \nEND\n";
  let parsed = PdbParser::new()
    .parse_bytes(pdb)
    .unwrap_or_else(|error| panic!("polymer-ligand fixture should parse: {error}"));
  let scene = StructureScene::from_first_model(&parsed.structure)
    .unwrap_or_else(|error| panic!("polymer-ligand fixture should produce a scene: {error}"));
  let mesh = default_mesh(&scene);

  assert!(mesh.vertices.iter().all(|vertex| vertex[0] < 10.0));
}

#[test]
fn all_non_solvent_scope_should_include_hetero_residues_with_polymers() {
  let pdb = b"ATOM      1  CA  GLY A   1       0.000   0.000   0.000  1.00 10.00           C  \nHETATM    2  C1  LIG A   2      20.000   0.000   0.000  1.00 10.00           C  \nEND\n";
  let parsed = PdbParser::new()
    .parse_bytes(pdb)
    .unwrap_or_else(|error| panic!("polymer-ligand fixture should parse: {error}"));
  let scene = StructureScene::from_first_model(&parsed.structure)
    .unwrap_or_else(|error| panic!("polymer-ligand fixture should produce a scene: {error}"));
  let surface = generate_molecular_surface(
    &scene,
    MolecularSurfaceRequest {
      atom_scope: SurfaceAtomScope::AllNonSolvent,
      ..MolecularSurfaceRequest::default()
    },
  );

  assert!(surface.domains[0].mesh.vertices.iter().any(|vertex| vertex[0] > 10.0));
}

#[test]
fn ses_mesh_should_support_non_solvent_hetero_only_scenes() {
  let pdb = b"HETATM    1  C1  LIG A   1       0.000   0.000   0.000  1.00 10.00           C  \nEND\n";
  let parsed = PdbParser::new()
    .parse_bytes(pdb)
    .unwrap_or_else(|error| panic!("ligand fixture should parse: {error}"));
  let scene = StructureScene::from_first_model(&parsed.structure)
    .unwrap_or_else(|error| panic!("ligand fixture should produce a scene: {error}"));

  assert!(!default_mesh(&scene).vertices.is_empty());
}

#[test]
fn unified_partition_should_calculate_one_domain_for_multiple_chains() {
  let pdb = b"ATOM      1  CA  GLY A   1       0.000   0.000   0.000  1.00 10.00           C  \nATOM      2  CA  GLY B   1       1.000   0.000   0.000  1.00 10.00           C  \nEND\n";
  let parsed = PdbParser::new()
    .parse_bytes(pdb)
    .unwrap_or_else(|error| panic!("multi-chain fixture should parse: {error}"));
  let scene = StructureScene::from_first_model(&parsed.structure)
    .unwrap_or_else(|error| panic!("multi-chain fixture should produce a scene: {error}"));
  let surface = generate_molecular_surface(
    &scene,
    MolecularSurfaceRequest {
      partition: SurfacePartition::Unified,
      ..MolecularSurfaceRequest::default()
    },
  );

  assert_eq!(surface.domains.len(), 1);
}

#[test]
fn traced_final_surface_should_match_normal_generation() {
  let scene = one_atom_scene();
  let request = MolecularSurfaceRequest {
    partition: SurfacePartition::Unified,
    ..MolecularSurfaceRequest::default()
  };

  let generated = generate_molecular_surface(&scene, request);
  let traced = trace_molecular_surface(&scene, request);

  assert_eq!(traced.domains.len(), generated.domains.len());
  assert!(
    traced
      .domains
      .iter()
      .zip(&generated.domains)
      .all(|(trace, domain)| { trace.chain_id == domain.chain_id && trace.final_surface == domain.mesh })
  );
}

#[test]
fn trace_fields_should_contain_one_value_per_grid_sample() {
  let trace = trace_molecular_surface(
    &one_atom_scene(),
    MolecularSurfaceRequest {
      partition: SurfacePartition::Unified,
      ..MolecularSurfaceRequest::default()
    },
  );
  let domain = &trace.domains[0];
  let sas_sample_count = domain.sas_field.dimensions.into_iter().product::<usize>();
  let probe_sample_count = domain.probe_field.dimensions.into_iter().product::<usize>();

  assert_eq!(domain.sas_field.values.len(), sas_sample_count);
  assert_eq!(domain.probe_field.values.len(), probe_sample_count);
}

#[test]
fn trace_inner_surface_should_exclude_the_raw_outer_sheet() {
  let trace = trace_molecular_surface(
    &one_atom_scene(),
    MolecularSurfaceRequest {
      partition: SurfacePartition::Unified,
      ..MolecularSurfaceRequest::default()
    },
  );
  let domain = &trace.domains[0];

  assert!(!domain.inner_probe_surface.indices.is_empty());
  assert!(domain.inner_probe_surface.indices.len() < domain.raw_probe_surface.indices.len());
}

#[test]
fn trace_smoothing_should_preserve_inner_surface_topology() {
  let trace = trace_molecular_surface(
    &one_atom_scene(),
    MolecularSurfaceRequest {
      partition: SurfacePartition::Unified,
      ..MolecularSurfaceRequest::default()
    },
  );
  let domain = &trace.domains[0];

  assert_eq!(
    domain.smoothed_inner_surface.indices,
    domain.inner_probe_surface.indices
  );
  assert_eq!(
    domain.smoothed_inner_surface.vertices.len(),
    domain.inner_probe_surface.vertices.len()
  );
}

#[test]
fn ses_parameters_should_reject_non_positive_probe_radius() {
  assert_eq!(
    SesParameters::new(0.0, 0.5),
    Err(MolecularSurfaceParameterError::InvalidProbeRadius(0.0))
  );
}

#[test]
fn ses_parameters_should_reject_non_finite_grid_spacing() {
  assert!(matches!(
    SesParameters::new(1.4, f32::NAN),
    Err(MolecularSurfaceParameterError::InvalidGridSpacing(value)) if value.is_nan()
  ));
}
