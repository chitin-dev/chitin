//! Budgeted exact core isomorphism, independent of atom types and input ordering.

use super::{Error, Work};

/// Enumerates template-to-input bijections using degree and adjacency pruning.
///
/// # Parameters
/// - `template`, `input`: equal-size, simple, undirected local adjacency tables.
/// - `work`: the shared ring budget, charged before each expansion/comparison.
///
/// # Returns
/// Complete exact mappings only. Already assigned nonedges are checked as well
/// as edges, so size, rank and degree agreement cannot produce a false match.
/// Template/input order affects tie-breaking, never connectivity acceptance.
///
/// # Errors
/// Budget exhaustion aborts enumeration rather than publishing partial matches.
pub(super) fn enumerate(
  template: &[Vec<usize>],
  input: &[Vec<usize>],
  work: &mut Work,
) -> Result<Vec<Vec<usize>>, Error> {
  if template.len() != input.len() {
    return Ok(Vec::new());
  }
  work.charge(template.len() + input.len())?;
  let mut search = Search {
    template,
    input,
    map: vec![None; template.len()],
    used: vec![false; input.len()],
    matches: Vec::new(),
  };
  search.visit(0, work)?;
  Ok(search.matches)
}

/// Private bounded backtracking scratch; mapping depth never exceeds ten here.
struct Search<'a> {
  /// Authored skeleton adjacency, never modified by matching.
  template: &'a [Vec<usize>],
  /// Source block adjacency, excluding external substituents.
  input: &'a [Vec<usize>],
  /// Tentative template-to-source rows; partial states never escape.
  map: Vec<Option<usize>>,
  /// Source vertices already consumed by the bijection.
  used: Vec<bool>,
  /// Only fully verified complete mappings.
  matches: Vec<Vec<usize>>,
}

impl Search<'_> {
  /// Expands a most-constrained unassigned vertex and restores scratch on return.
  fn visit(&mut self, depth: usize, work: &mut Work) -> Result<(), Error> {
    work.charge(1 + self.template.len() * self.template.len())?;
    if depth == self.map.len() {
      let complete = self
        .map
        .iter()
        .copied()
        .collect::<Option<Vec<_>>>()
        .ok_or(Error::CoordinateRange)?;
      self.matches.push(complete);
      return Ok(());
    }
    let Some(vertex) = (0..self.map.len()).filter(|&i| self.map[i].is_none()).max_by_key(|&i| {
      (
        self.template[i]
          .iter()
          .filter(|&&other| self.map[other].is_some())
          .count(),
        self.template[i].len(),
        std::cmp::Reverse(i),
      )
    }) else {
      return Err(Error::CoordinateRange);
    };
    for candidate in 0..self.input.len() {
      work.charge(1 + self.map.len() * self.template.len())?;
      if self.used[candidate] || self.template[vertex].len() != self.input[candidate].len() {
        continue;
      }
      if self.map.iter().enumerate().any(|(other, &mapped)| {
        mapped.is_some_and(|mapped| self.template[vertex].contains(&other) != self.input[candidate].contains(&mapped))
      }) {
        continue;
      }
      self.map[vertex] = Some(candidate);
      self.used[candidate] = true;
      self.visit(depth + 1, work)?;
      self.map[vertex] = None;
      self.used[candidate] = false;
    }
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::super::templates::Template;
  use super::*;

  #[test]
  fn exact_matching_should_enumerate_all_template_symmetries_within_default_budget() -> Result<(), Error> {
    for (template, expected) in [(Template::Adamantane, 24), (Template::Cubane, 48)] {
      let rows = template.rows();
      let mappings = enumerate(&rows, &rows, &mut Work::new(2_000_000))?;
      assert_eq!(mappings.len(), expected);
      for mapping in mappings {
        let unique: std::collections::BTreeSet<_> = mapping.iter().copied().collect();
        assert_eq!(unique.len(), rows.len());
        for a in 0..rows.len() {
          for b in 0..rows.len() {
            assert_eq!(rows[a].contains(&b), rows[mapping[a]].contains(&mapping[b]));
          }
        }
      }
    }
    Ok(())
  }

  #[test]
  fn equal_size_rank_and_degrees_should_not_accept_a_different_core() -> Result<(), Error> {
    let cube = Template::Cubane.rows();
    let mut other = vec![Vec::new(); 8];
    for [a, b] in (0..8).map(|i| [i, (i + 1) % 8]).chain((0..4).map(|i| [i, i + 4])) {
      other[a].push(b);
      other[b].push(a);
    }
    assert!(other.iter().all(|row| row.len() == 3));
    assert!(enumerate(&cube, &other, &mut Work::new(2_000_000))?.is_empty());
    Ok(())
  }

  #[test]
  fn unfinished_matching_should_fail_on_budget_without_publishing_partial_results() {
    let rows = Template::Cubane.rows();
    assert!(matches!(
      enumerate(&rows, &rows, &mut Work::new(100)),
      Err(Error::RingSearchBudgetExceeded)
    ));
  }
}
