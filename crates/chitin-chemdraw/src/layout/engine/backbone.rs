//! Native backbone drawing preferences, separate from supplied stereochemistry.
//! Source attribution and redistribution terms are in licenses/coordinate-generation.txt.

use super::{Error, Work, interactions::Interaction, topology::Model};

/// Detects coordinate-only backbone torsions independently in each component.
///
/// # Returns
/// Trans drawing preferences in the three source sequence orders. These terms
/// participate in discrete search, not chemical perception or final force setup.
///
/// # Errors
/// Classification, path enumeration and emitted torsions share the work budget.
pub(super) fn torsions(model: &Model, work: &mut Work) -> Result<Vec<Interaction>, Error> {
  work.charge(model.atoms.len() + 2 * model.edges.len())?;
  let carbonyl: Vec<_> = model
    .atoms
    .iter()
    .enumerate()
    .map(|(a, atom)| {
      atom.number == 6
        && model.neighbors[a]
          .iter()
          .any(|&(n, e)| model.atoms[n].number == 8 && model.edges[e].order == 2)
    })
    .collect();
  let nitrogen: Vec<_> = model.atoms.iter().map(|atom| atom.number == 7).collect();
  let alpha: Vec<_> = model
    .atoms
    .iter()
    .enumerate()
    .map(|(a, atom)| {
      atom.number == 6
        && !carbonyl[a]
        && model.neighbors[a].iter().any(|&(n, _)| carbonyl[n])
        && model.neighbors[a].iter().any(|&(n, _)| nitrogen[n])
    })
    .collect();
  let mut eligible = vec![false; model.atoms.len()];
  for nodes in &model.components {
    if [&carbonyl, &nitrogen, &alpha]
      .iter()
      .all(|set| nodes.iter().filter(|&&a| set[a]).count() >= 2)
    {
      for &a in nodes {
        eligible[a] = true;
      }
    }
  }
  let mut result = Vec::new();
  for sets in [
    [&carbonyl, &nitrogen, &alpha, &carbonyl],
    [&nitrogen, &alpha, &carbonyl, &nitrogen],
    [&alpha, &carbonyl, &nitrogen, &alpha],
  ] {
    let mut first: Vec<_> = (0..model.atoms.len()).filter(|&a| eligible[a] && sets[0][a]).collect();
    first.sort_by_key(|&a| model.atoms[a].id);
    for a in first {
      for &(b, _) in &model.neighbors[a] {
        work.charge(1)?;
        if !sets[1][b] {
          continue;
        }
        for &(c, _) in &model.neighbors[b] {
          work.charge(1)?;
          if !sets[2][c] {
            continue;
          }
          for &(d, _) in &model.neighbors[c] {
            work.charge(1)?;
            if sets[3][d] {
              result.push(Interaction::Alkene {
                atoms: [a, b, c, d],
                same_side: false,
              });
            }
          }
        }
      }
    }
  }
  Ok(result)
}
