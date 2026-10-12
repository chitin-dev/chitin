//! Deterministic display partitions, not analytical surface-area attribution.

use super::{MolecularSurfaceArtifact, SurfaceGeometrySource, atoms::van_der_waals_radius, select_surface_scene_atoms};
use crate::structure::{AtomId, AtomSceneInstance};

struct Node {
  atom: AtomSceneInstance,
  radius: f64,
  max_radius: f64,
  axis: usize,
  children: [Option<usize>; 2],
}

#[derive(Default)]
struct SphereIndex {
  nodes: Vec<Node>,
  root: Option<usize>,
}

impl SphereIndex {
  fn new(mut atoms: Vec<AtomSceneInstance>) -> Self {
    atoms.retain(|atom| atom.position.iter().all(|value| value.is_finite()));
    let mut index = Self::default();
    index.root = index.build(&mut atoms, 0);
    index
  }

  fn build(&mut self, atoms: &mut [AtomSceneInstance], depth: usize) -> Option<usize> {
    if atoms.is_empty() {
      return None;
    }
    let axis = depth % 3;
    let middle = atoms.len() / 2;
    atoms.select_nth_unstable_by(middle, |a, b| {
      a.position[axis]
        .total_cmp(&b.position[axis])
        .then(a.atom_id.cmp(&b.atom_id))
    });
    let atom = atoms[middle];
    let (left, right) = atoms.split_at_mut(middle);
    let children = [self.build(left, depth + 1), self.build(&mut right[1..], depth + 1)];
    let radius = van_der_waals_radius(atom.element);
    let max_radius = children
      .iter()
      .flatten()
      .fold(radius, |radius, child| radius.max(self.nodes[*child].max_radius));
    let id = self.nodes.len();
    self.nodes.push(Node {
      atom,
      radius,
      max_radius,
      axis,
      children,
    });
    Some(id)
  }

  fn owner(&self, position: [f32; 3]) -> Option<AtomId> {
    if !position.iter().all(|value| value.is_finite()) {
      return None;
    }
    let mut best = (f64::INFINITY, None);
    self.visit(self.root, position, &mut best);
    best.1
  }

  fn visit(&self, id: Option<usize>, position: [f32; 3], best: &mut (f64, Option<AtomId>)) {
    let Some(id) = id else {
      return;
    };
    let node = &self.nodes[id];
    let distance = (0..3)
      .map(|axis| (f64::from(position[axis]) - f64::from(node.atom.position[axis])).powi(2))
      .sum::<f64>()
      .sqrt()
      - node.radius;
    if distance < best.0 || (distance == best.0 && best.1.is_none_or(|id| node.atom.atom_id < id)) {
      *best = (distance, Some(node.atom.atom_id));
    }
    let delta = f64::from(position[node.axis]) - f64::from(node.atom.position[node.axis]);
    let side = usize::from(delta >= 0.);
    self.visit(node.children[side], position, best);
    if let Some(far) = node.children[1 - side]
      && delta.abs() - self.nodes[far].max_radius <= best.0
    {
      self.visit(Some(far), position, best);
    }
  }
}

/// Assigns display vertices to eligible source atoms within each surface domain.
///
/// The smallest signed distance to a van der Waals sphere defines the display
/// partition; source AtomId breaks exact ties. This is a geometric interaction
/// convention shared by implicit and MSMS display meshes, not a claim of exact
/// analytical contact/toroidal/reentrant patch ownership or per-atom area.
/// Returned entries follow domain/vertex packing order. Non-finite vertices and
/// domains without eligible atoms are unowned, never assigned to another chain.
pub fn surface_vertex_atom_ids(atoms: &[AtomSceneInstance], surface: &MolecularSurfaceArtifact) -> Vec<Option<AtomId>> {
  let scope = match surface.source {
    SurfaceGeometrySource::ImplicitGrid(request) => request.atom_scope,
    SurfaceGeometrySource::Msms { atom_scope, .. } => atom_scope,
  };
  let eligible = select_surface_scene_atoms(atoms, scope).copied().collect::<Vec<_>>();
  let mut owners = Vec::with_capacity(surface.domains.iter().map(|domain| domain.mesh.vertices.len()).sum());
  for domain in &surface.domains {
    let index = SphereIndex::new(
      eligible
        .iter()
        .filter(|atom| domain.chain_id.is_none_or(|chain| atom.chain_id == chain))
        .copied()
        .collect(),
    );
    owners.extend(
      domain
        .mesh
        .vertices
        .iter()
        .map(|vertex| index.owner([vertex[0], vertex[1], vertex[2]])),
    );
  }
  owners
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::structure::{PdbParser, StructureScene};

  fn scene() -> StructureScene {
    let parsed = PdbParser::new().parse_bytes(b"ATOM      1  H   GLY A   1       0.000   0.000   0.000  1.00 10.00           H  \nATOM      2  S   CYS B   2       0.000   0.000   1.000  1.00 10.00           S  \nEND\n").unwrap_or_else(|error| panic!("fixture parses: {error}"));
    StructureScene::from_first_model(&parsed.structure).unwrap_or_else(|error| panic!("fixture scene: {error}"))
  }

  #[test]
  fn sphere_distance_should_not_confuse_nearest_atom_center_with_nearest_surface() {
    let scene = scene();
    let index = SphereIndex::new(scene.atoms.clone());
    // H is the nearer center (0.4 versus 0.6 A), but S has the nearer VDW surface.
    assert_eq!(index.owner([0., 0., 0.4]), Some(scene.atoms[1].atom_id));
  }

  #[test]
  fn tree_queries_should_match_exhaustive_sphere_distance_search() {
    let scene = scene();
    let mut atoms = Vec::new();
    for i in 0..20 {
      let mut atom = scene.atoms[i % 2];
      atom.position = [i as f32 * 0.35, (i % 3) as f32, (i % 5) as f32];
      atoms.push(atom);
    }
    let index = SphereIndex::new(atoms.clone());
    for i in -10..50 {
      let p = [i as f32 * 0.2, 0.7, 2.3];
      let expected = atoms
        .iter()
        .min_by(|a, b| {
          let distance = |atom: &AtomSceneInstance| {
            glam::DVec3::from_array(p.map(f64::from)).distance(glam::DVec3::from_array(atom.position.map(f64::from)))
              - van_der_waals_radius(atom.element)
          };
          distance(a).total_cmp(&distance(b)).then(a.atom_id.cmp(&b.atom_id))
        })
        .map(|atom| atom.atom_id);
      assert_eq!(index.owner(p), expected);
    }
  }

  #[test]
  fn invalid_vertices_should_not_acquire_an_owner() {
    let index = SphereIndex::new(scene().atoms);
    assert_eq!(index.owner([f32::NAN, 0., 0.]), None);
    assert_eq!(SphereIndex::default().owner([0.; 3]), None);
  }

  #[test]
  fn both_backends_should_respect_chain_domains_and_atom_scope() {
    use super::super::{MolecularSurfaceRequest, SurfaceAtomScope, SurfaceDomainArtifact, SurfaceMesh};
    let scene = scene();
    for source in [
      SurfaceGeometrySource::ImplicitGrid(MolecularSurfaceRequest::default()),
      SurfaceGeometrySource::Msms {
        atom_scope: SurfaceAtomScope::default(),
        probe_radius: 1.4,
        max_edge_length: 0.5,
      },
    ] {
      let mut surface = MolecularSurfaceArtifact {
        source,
        domains: vec![SurfaceDomainArtifact {
          chain_id: Some(scene.atoms[0].chain_id),
          mesh: SurfaceMesh {
            vertices: vec![[0., 0., 0.4, 0., 0., 1.]],
            indices: vec![],
          },
        }],
      };
      assert_eq!(
        surface_vertex_atom_ids(&scene.atoms, &surface),
        vec![Some(scene.atoms[0].atom_id)]
      );
      surface.domains[0].chain_id = None;
      assert_eq!(
        surface_vertex_atom_ids(&scene.atoms, &surface),
        vec![Some(scene.atoms[1].atom_id)]
      );
      assert_eq!(
        surface_vertex_atom_ids(&scene.atoms[..1], &surface),
        vec![Some(scene.atoms[0].atom_id)]
      );
    }
  }

  #[test]
  fn surface_scope_should_exclude_ligands_until_explicitly_included() {
    use super::super::{SurfaceAtomScope, SurfaceDomainArtifact, SurfaceMesh};
    let scene = scene();
    let mut atoms = scene.atoms;
    atoms[1].residue_kind = crate::structure::ResidueKind::Hetero;
    let mut surface = MolecularSurfaceArtifact {
      source: SurfaceGeometrySource::Msms {
        atom_scope: SurfaceAtomScope::BiopolymerOrNonSolvent,
        probe_radius: 1.4,
        max_edge_length: 0.5,
      },
      domains: vec![SurfaceDomainArtifact {
        chain_id: None,
        mesh: SurfaceMesh {
          vertices: vec![[0., 0., 0.4, 0., 0., 1.]],
          indices: vec![],
        },
      }],
    };
    assert_eq!(surface_vertex_atom_ids(&atoms, &surface), vec![Some(atoms[0].atom_id)]);
    surface.source = SurfaceGeometrySource::Msms {
      atom_scope: SurfaceAtomScope::AllNonSolvent,
      probe_radius: 1.4,
      max_edge_length: 0.5,
    };
    assert_eq!(surface_vertex_atom_ids(&atoms, &surface), vec![Some(atoms[1].atom_id)]);
  }
}
