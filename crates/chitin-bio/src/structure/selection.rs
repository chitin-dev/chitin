//! Scientific object specifications and model-scoped selection resolution.

use super::{AtomId, Chain, Residue, ResidueId, Structure, StructureScene};
use std::{collections::BTreeSet, str::FromStr};

/// Atom identities selected within one structure revision.
pub type AtomSelection = BTreeSet<AtomId>;

/// Compact source metadata retained for scientific selection, not rendering.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SelectionIndex {
  chains: Vec<Chain>,
  residues: Vec<Residue>,
  atoms: Vec<(String, ResidueId)>,
}

impl SelectionIndex {
  pub(super) fn from_structure(structure: &Structure) -> Self {
    Self {
      chains: structure.chains.clone(),
      residues: structure.residues.clone(),
      atoms: structure
        .atoms
        .iter()
        .map(|atom| (atom.name.clone(), atom.residue_id))
        .collect(),
    }
  }
  /// Returns all coordinate-bearing atoms of one residue in this scene.
  pub fn residue_atoms(&self, scene: &StructureScene, residue: ResidueId) -> AtomSelection {
    scene
      .atoms
      .iter()
      .filter(|atom| atom.residue_id == residue)
      .map(|atom| atom.atom_id)
      .collect()
  }
  /// Resolves a specification using author chain IDs and author residue numbering.
  pub fn resolve(&self, spec: &AtomSpec, scene: &StructureScene) -> AtomSelection {
    let spec = &spec.0;
    scene
      .atoms
      .iter()
      .filter_map(|instance| {
        let (name, residue_id) = self.atoms.get(instance.atom_id.index())?;
        let residue = self.residues.get(residue_id.index())?;
        let chain = self.chains.get(residue.chain_id.index())?;
        let chain_matches = spec
          .chains
          .as_ref()
          .is_none_or(|ids| ids.iter().any(|id| chain.auth_id.as_deref().unwrap_or("") == id));
        let residue_matches = spec.residues.as_ref().is_none_or(|terms| {
          terms.iter().any(|term| match term {
            ResidueTerm::Name(name) => residue.name.eq_ignore_ascii_case(name),
            ResidueTerm::Number(number, insertion) => {
              residue.sequence_number == *number && insertion.is_none_or(|code| residue.insertion_code == Some(code))
            }
            ResidueTerm::Range(start, end) => (*start..=*end).contains(&residue.sequence_number),
          })
        });
        let atom_matches = spec
          .atoms
          .as_ref()
          .is_none_or(|names| names.iter().any(|candidate| name.eq_ignore_ascii_case(candidate)));
        (chain_matches && residue_matches && atom_matches).then_some(instance.atom_id)
      })
      .collect()
  }
  /// Summarizes selected coordinate-bearing atoms without a second selection authority.
  pub fn counts(&self, scene: &StructureScene, selected: &AtomSelection) -> (usize, usize, usize) {
    let atoms = scene.atoms.iter().filter(|atom| selected.contains(&atom.atom_id));
    let mut residues = BTreeSet::new();
    let mut chains = BTreeSet::new();
    let mut count = 0;
    for atom in atoms {
      count += 1;
      residues.insert(atom.residue_id);
      chains.insert(atom.chain_id);
    }
    (count, residues.len(), chains.len())
  }
}

/// Validated ChimeraX-style hierarchical subset: `/chain:residue@atom`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtomSpec(Box<SpecComponents>);

#[derive(Debug, Clone, PartialEq, Eq)]
struct SpecComponents {
  chains: Option<Vec<String>>,
  residues: Option<Vec<ResidueTerm>>,
  atoms: Option<Vec<String>>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
enum ResidueTerm {
  Name(String),
  Number(i32, Option<char>),
  Range(i32, i32),
}

impl FromStr for AtomSpec {
  type Err = String;
  fn from_str(input: &str) -> Result<Self, Self::Err> {
    if input.is_empty() || input.chars().any(|c| c.is_whitespace() || "#&|~()".contains(c)) {
      return Err("expected /chain:residue@atom; model IDs and boolean expressions are not supported yet".into());
    }
    let mut spec = SpecComponents {
      chains: None,
      residues: None,
      atoms: None,
    };
    let mut rest = input;
    for marker in ['/', ':', '@'] {
      if let Some(value) = rest.strip_prefix(marker) {
        let end = value.find(['/', ':', '@']).unwrap_or(value.len());
        let part = &value[..end];
        rest = &value[end..];
        let parts = part.split(',').collect::<Vec<_>>();
        if parts.iter().any(|part| part.is_empty()) {
          return Err("empty selection component".into());
        }
        match marker {
          '/' => spec.chains = Some(parts.iter().map(|part| (*part).to_string()).collect()),
          '@' => spec.atoms = Some(parts.iter().map(|part| (*part).to_string()).collect()),
          ':' => spec.residues = Some(parts.into_iter().map(parse_residue).collect::<Result<_, _>>()?),
          _ => {}
        }
      }
    }
    if !rest.is_empty() || (spec.chains.is_none() && spec.residues.is_none() && spec.atoms.is_none()) {
      return Err("selection components must be ordered /chain:residue@atom".into());
    }
    Ok(Self(Box::new(spec)))
  }
}

fn parse_residue(value: &str) -> Result<ResidueTerm, String> {
  if value.chars().all(|c| c.is_ascii_alphabetic()) {
    return Ok(ResidueTerm::Name(value.to_ascii_uppercase()));
  }
  // Skip a leading minus so negative author numbers are not mistaken for ranges.
  if let Some((offset, _)) = value.char_indices().skip(1).find(|(_, c)| *c == '-') {
    let start = value[..offset].parse::<i32>().map_err(|_| "invalid residue range")?;
    let end = value[offset + 1..]
      .parse::<i32>()
      .map_err(|_| "invalid residue range")?;
    if start > end {
      return Err("residue range must be ascending".into());
    }
    return Ok(ResidueTerm::Range(start, end));
  }
  let insertion = value.chars().last().filter(char::is_ascii_alphabetic);
  let number = if insertion.is_some() {
    &value[..value.len() - 1]
  } else {
    value
  };
  let number = number
    .parse::<i32>()
    .map_err(|_| "expected residue number, range, or name")?;
  Ok(ResidueTerm::Number(number, insertion))
}

#[cfg(test)]
mod tests {
  use super::super::PdbParser;
  use super::*;
  #[test]
  fn author_specs_should_resolve_residues_and_atoms() -> Result<(), Box<dyn std::error::Error>> {
    let parsed = PdbParser::new().parse_bytes(b"ATOM      1  CA  GLY A  42       1.000   2.000   3.000  1.00 10.00           C  \nATOM      2  N   GLY A  42       0.000   2.000   3.000  1.00 10.00           N  \nEND\n")?;
    let scene = StructureScene::from_first_model(&parsed.structure)?;
    assert_eq!(scene.selection.resolve(&"/A:42".parse()?, &scene).len(), 2);
    assert_eq!(scene.selection.resolve(&"/A:42@CA".parse()?, &scene).len(), 1);
    assert!(scene.selection.resolve(&"/B:42".parse()?, &scene).is_empty());
    Ok(())
  }
  #[test]
  fn specs_should_validate_negative_ranges_insertion_codes_and_component_order() {
    for value in ["/AA:-5--1", "/A:42A", ":LYS,ARG", "@CA,N"] {
      assert!(value.parse::<AtomSpec>().is_ok(), "{value}");
    }
    for value in ["#1/A:42", "/A:", ":60-42", "@CA/A", "/A & /B"] {
      assert!(value.parse::<AtomSpec>().is_err(), "{value}");
    }
  }

  #[test]
  fn resolution_should_use_author_ids_and_preserve_insertion_codes() -> Result<(), Box<dyn std::error::Error>> {
    let mut parsed = PdbParser::new()
      .parse_bytes(b"ATOM      1  CA  GLY A  42       1.000   2.000   3.000  1.00 10.00           C  \nEND\n")?;
    parsed.structure.chains[0].auth_id = Some("AA".into());
    parsed.structure.chains[0].label_id = Some("B".into());
    parsed.structure.residues[0].sequence_number = -5;
    parsed.structure.residues[0].insertion_code = Some('A');
    let scene = StructureScene::from_first_model(&parsed.structure)?;
    for spec in ["/AA:-5A", "/AA:-5", "/AA:-6--1", ":GLY@CA"] {
      assert_eq!(scene.selection.resolve(&spec.parse()?, &scene).len(), 1, "{spec}");
    }
    for spec in ["/B:-5", "/AA:-5B", "/AA:42"] {
      assert!(scene.selection.resolve(&spec.parse()?, &scene).is_empty(), "{spec}");
    }
    Ok(())
  }
}
