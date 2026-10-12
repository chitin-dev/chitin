//! Cached visibility predicates shared by GPU draw geometry and CPU picking.

use crate::{appearance::ColorCache, picking::PickingGeometry};
use chitin_bio::structure::{AtomId, ResidueId, selection::AtomSelection};
use std::{
  collections::{HashMap, HashSet},
  ops::Range,
};

#[derive(Clone, Default)]
pub(crate) struct VisibilityMask {
  atoms: HashSet<AtomId>,
  residues: HashSet<ResidueId>,
  all_hidden: bool,
}

impl VisibilityMask {
  pub(crate) fn new(owners: &HashMap<AtomId, ResidueId>, hidden: &AtomSelection) -> Self {
    let mut residues: HashSet<_> = hidden.iter().filter_map(|atom| owners.get(atom).copied()).collect();
    for (atom, residue) in owners {
      if !hidden.contains(atom) {
        residues.remove(residue);
      }
    }
    Self {
      atoms: hidden.iter().copied().collect(),
      residues,
      all_hidden: !owners.is_empty() && owners.keys().all(|atom| hidden.contains(atom)),
    }
  }

  pub(crate) fn atom(&self, atom: AtomId) -> bool {
    !self.atoms.contains(&atom)
  }
  pub(crate) fn bond(&self, atoms: [AtomId; 2]) -> bool {
    atoms.into_iter().all(|atom| self.atom(atom))
  }
  pub(crate) fn surface(&self, indices: [u32; 3], colors: &ColorCache) -> bool {
    if self.all_hidden {
      return false;
    }
    indices.into_iter().all(|index| {
      colors
        .surface_owner(index as usize)
        .is_none_or(|(atom, _)| self.atom(atom))
    })
  }
  pub(crate) fn cartoon(&self, indices: [u32; 3], owners: &[Option<ResidueId>]) -> bool {
    if self.all_hidden {
      return false;
    }
    indices.into_iter().all(|index| {
      owners
        .get(index as usize)
        .copied()
        .flatten()
        .is_none_or(|residue| !self.residues.contains(&residue))
    })
  }
}

#[derive(Default)]
pub(crate) struct VisibleDraws {
  pub atoms: Vec<Range<u32>>,
  pub bonds: Vec<Range<u32>>,
  pub cartoon: Vec<u32>,
  pub surface: Vec<u32>,
}

impl VisibleDraws {
  pub(crate) fn new(geometry: &PickingGeometry, colors: &ColorCache) -> Self {
    let mask = &geometry.visibility;
    if mask.atoms.is_empty() {
      return Self {
        atoms: ranges(std::iter::repeat_n(true, geometry.atoms.len())),
        bonds: ranges(std::iter::repeat_n(true, geometry.bonds.len())),
        cartoon: geometry.cartoon_indices.clone(),
        surface: geometry.surface_indices.clone(),
      };
    }
    Self {
      atoms: ranges(geometry.atoms.iter().map(|atom| mask.atom(*atom))),
      bonds: ranges(geometry.bonds.iter().map(|atoms| mask.bond(*atoms))),
      cartoon: triangles(&geometry.cartoon_indices, |indices| {
        mask.cartoon(indices, &geometry.cartoon_residues)
      }),
      surface: triangles(&geometry.surface_indices, |indices| mask.surface(indices, colors)),
    }
  }
}

fn ranges(visible: impl IntoIterator<Item = bool>) -> Vec<Range<u32>> {
  let mut ranges: Vec<Range<u32>> = Vec::new();
  for (index, visible) in visible.into_iter().enumerate() {
    if !visible {
      continue;
    }
    let index = index as u32;
    if let Some(last) = ranges.last_mut()
      && last.end == index
    {
      last.end += 1;
    } else {
      ranges.push(index..index + 1);
    }
  }
  ranges
}

fn triangles(indices: &[u32], visible: impl Fn([u32; 3]) -> bool) -> Vec<u32> {
  indices
    .as_chunks::<3>()
    .0
    .iter()
    .filter(|row| visible(**row))
    .flatten()
    .copied()
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;
  #[test]
  fn instance_ranges_should_coalesce_and_keep_source_offsets() {
    assert_eq!(ranges([true, true, false, true, false]), vec![0..2, 3..4]);
    assert!(ranges([false, false]).is_empty());
  }
  #[test]
  fn triangle_filter_should_remove_whole_triangles_without_reindexing() {
    assert_eq!(
      triangles(&[0, 1, 2, 3, 4, 5], |triangle| !triangle.contains(&1)),
      vec![3, 4, 5]
    );
  }

  #[test]
  fn cartoon_should_hide_only_fully_hidden_residues_and_restore() -> Result<(), Box<dyn std::error::Error>> {
    let parsed = chitin_bio::structure::PdbParser::new().parse_bytes(b"ATOM      1  CA  GLY A  42       1.000   2.000   3.000  1.00 10.00           C  \nATOM      2  N   GLY A  42       0.000   2.000   3.000  1.00 10.00           N  \nEND\n")?;
    let scene = chitin_bio::structure::StructureScene::from_first_model(&parsed.structure)?;
    let owners = scene.atoms.iter().map(|atom| (atom.atom_id, atom.residue_id)).collect();
    let a = scene.atoms[0];
    let residue = vec![Some(a.residue_id); 3];
    let partial = VisibilityMask::new(&owners, &[a.atom_id].into());
    assert!(!partial.atom(a.atom_id));
    assert!(partial.cartoon([0, 1, 2], &residue));
    let hidden = scene.atoms.iter().map(|atom| atom.atom_id).collect();
    let complete = VisibilityMask::new(&owners, &hidden);
    assert!(!complete.cartoon([0, 1, 2], &residue));
    let restored = VisibilityMask::new(&owners, &AtomSelection::new());
    assert!(restored.cartoon([0, 1, 2], &residue));
    assert!(!partial.bond([a.atom_id, scene.atoms[1].atom_id]));
    Ok(())
  }
}
