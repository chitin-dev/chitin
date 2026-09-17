//! Shared atom selection, radii, and domain partitioning for surface backends.

use std::collections::BTreeMap;

use crate::structure::{ChainId, ElementCategory, ResidueKind, StructureScene};

/// Atom-selection policy used before molecular-surface domains are partitioned.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceAtomScope {
  /// Use biopolymer atoms, falling back to non-solvent atoms for ligand-only scenes.
  #[default]
  BiopolymerOrNonSolvent,
  /// Use every non-solvent atom in the scene, including ligands and cofactors.
  AllNonSolvent,
}

/// Partition applied to selected atoms before independent surface calculations.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum SurfacePartition {
  /// Calculate one envelope around all selected atoms.
  Unified,
  /// Calculate one independent surface for each chain.
  #[default]
  ByChain,
}

/// One selected atom shared by numerical and analytical surface backends.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SurfaceAtom {
  /// Stable source atom index used by analytical topology.
  pub(crate) atom_index: usize,
  /// Cartesian atom position in ångströms.
  pub(crate) position: glam::DVec3,
  /// Element-dependent van der Waals radius in ångströms.
  pub(crate) radius: f64,
}

/// Selects surface atoms and partitions them into deterministic domains.
///
/// When a scene contains polymer atoms, the default scope omits hetero residues
/// to produce a biopolymer surface. Ligand-only scenes fall back to all
/// non-solvent atoms. Both implicit and analytical backends call this function,
/// so atom membership and element radii cannot silently diverge.
///
/// # Parameters
///
/// * `scene` supplies source identities, coordinates, chains, and residue classes.
/// * `atom_scope` controls whether hetero atoms accompany a polymer.
/// * `partition` selects one unified domain or one domain per chain.
///
/// # Returns
///
/// Selected atoms keyed by optional chain identity in deterministic key and
/// source-atom order.
pub(crate) fn surface_atom_groups(
  scene: &StructureScene,
  atom_scope: SurfaceAtomScope,
  partition: SurfacePartition,
) -> BTreeMap<Option<ChainId>, Vec<SurfaceAtom>> {
  let contains_polymer = scene.atoms.iter().any(|atom| atom.residue_kind == ResidueKind::Polymer);
  let mut groups = BTreeMap::new();
  for atom in scene.atoms.iter().filter(|atom| {
    !atom.is_solvent
      && match atom_scope {
        SurfaceAtomScope::BiopolymerOrNonSolvent => !contains_polymer || atom.residue_kind == ResidueKind::Polymer,
        SurfaceAtomScope::AllNonSolvent => true,
      }
  }) {
    let domain = match partition {
      SurfacePartition::Unified => None,
      SurfacePartition::ByChain => Some(atom.chain_id),
    };
    groups.entry(domain).or_insert_with(Vec::new).push(SurfaceAtom {
      atom_index: atom.atom_id.index(),
      position: glam::Vec3::from_array(atom.position).as_dvec3(),
      radius: van_der_waals_radius(atom.element),
    });
  }
  groups
}

/// Returns the shared van der Waals radius for one element category.
pub(crate) fn van_der_waals_radius(element: ElementCategory) -> f64 {
  match element {
    ElementCategory::Hydrogen => 1.20,
    ElementCategory::Carbon => 1.70,
    ElementCategory::Nitrogen => 1.55,
    ElementCategory::Oxygen => 1.52,
    ElementCategory::Phosphorus | ElementCategory::Sulfur => 1.80,
    ElementCategory::Halogen => 1.75,
    ElementCategory::Metal => 1.70,
    ElementCategory::Other => 1.50,
  }
}

#[cfg(test)]
mod tests {
  use crate::structure::{PdbParser, StructureScene};

  use super::*;

  fn scene_from_pdb(bytes: &[u8]) -> StructureScene {
    let parsed = PdbParser::new()
      .parse_bytes(bytes)
      .unwrap_or_else(|error| panic!("surface-atom fixture should parse: {error}"));
    StructureScene::from_first_model(&parsed.structure)
      .unwrap_or_else(|error| panic!("surface-atom fixture should produce a scene: {error}"))
  }

  #[test]
  fn default_scope_should_keep_polymer_and_exclude_hetero_atoms() {
    let scene = scene_from_pdb(
      b"ATOM      1  CA  GLY A   1       0.000   0.000   0.000  1.00 10.00           C  \n\
HETATM    2  C1  LIG A   2       3.000   0.000   0.000  1.00 10.00           C  \n\
END\n",
    );
    let groups = surface_atom_groups(
      &scene,
      SurfaceAtomScope::BiopolymerOrNonSolvent,
      SurfacePartition::Unified,
    );

    assert_eq!(groups.get(&None).map(Vec::len), Some(1));
  }

  #[test]
  fn all_non_solvent_scope_should_include_ligands_but_not_water() {
    let scene = scene_from_pdb(
      b"ATOM      1  CA  GLY A   1       0.000   0.000   0.000  1.00 10.00           C  \n\
HETATM    2  C1  LIG A   2       3.000   0.000   0.000  1.00 10.00           C  \n\
HETATM    3  O   HOH A   3       6.000   0.000   0.000  1.00 10.00           O  \n\
END\n",
    );
    let groups = surface_atom_groups(&scene, SurfaceAtomScope::AllNonSolvent, SurfacePartition::Unified);

    assert_eq!(groups.get(&None).map(Vec::len), Some(2));
  }

  #[test]
  fn ligand_only_scene_should_use_non_solvent_fallback() {
    let scene = scene_from_pdb(
      b"HETATM    1  C1  LIG A   1       0.000   0.000   0.000  1.00 10.00           C  \n\
HETATM    2  O   HOH A   2       3.000   0.000   0.000  1.00 10.00           O  \n\
END\n",
    );
    let groups = surface_atom_groups(
      &scene,
      SurfaceAtomScope::BiopolymerOrNonSolvent,
      SurfacePartition::Unified,
    );

    assert_eq!(groups.get(&None).map(Vec::len), Some(1));
  }

  #[test]
  fn shared_radius_table_should_preserve_analytical_decimal_values() {
    assert_eq!(van_der_waals_radius(ElementCategory::Carbon), 1.70_f64);
  }
}
