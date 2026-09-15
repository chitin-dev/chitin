//! GPU-facing packing for renderer-neutral molecular-surface meshes.

use chitin_bio::surface::MolecularSurfaceArtifact;

/// Packs all computed surface domains into the renderer's interleaved vertex layout.
pub(crate) fn surface_mesh_vertices(
  surface: Option<&MolecularSurfaceArtifact>,
  color: [f32; 3],
) -> (Vec<[f32; 9]>, Vec<u32>) {
  let Some(surface) = surface else {
    return (Vec::new(), Vec::new());
  };

  let vertex_count = surface.domains.iter().map(|domain| domain.mesh.vertices.len()).sum();
  let index_count = surface.domains.iter().map(|domain| domain.mesh.indices.len()).sum();
  let mut vertices = Vec::with_capacity(vertex_count);
  let mut indices = Vec::with_capacity(index_count);

  for domain in &surface.domains {
    let vertex_offset = vertices.len() as u32;
    vertices.extend(domain.mesh.vertices.iter().map(|vertex| {
      [
        vertex[0], vertex[1], vertex[2], vertex[3], vertex[4], vertex[5], color[0], color[1], color[2],
      ]
    }));
    indices.extend(domain.mesh.indices.iter().map(|index| index + vertex_offset));
  }

  (vertices, indices)
}

#[cfg(test)]
mod tests {
  use super::*;
  use chitin_bio::{
    structure::{PdbParser, StructureScene},
    surface::{MolecularSurfaceRequest, generate_molecular_surface},
  };

  #[test]
  fn packing_should_add_visual_color_without_changing_scientific_vertices() {
    let parsed = PdbParser::new()
      .parse_bytes(b"ATOM      1  C   GLY A   1       0.000   0.000   0.000  1.00 10.00           C  \nEND\n")
      .unwrap_or_else(|error| panic!("surface fixture should parse: {error}"));
    let scene = StructureScene::from_first_model(&parsed.structure)
      .unwrap_or_else(|error| panic!("surface fixture should produce a scene: {error}"));
    let surface = generate_molecular_surface(&scene, MolecularSurfaceRequest::default());

    let (vertices, _) = surface_mesh_vertices(Some(&surface), [0.2, 0.4, 0.6]);

    assert!(vertices.iter().all(|vertex| vertex[6..9] == [0.2, 0.4, 0.6]));
  }
}
