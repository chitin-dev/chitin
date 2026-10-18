//! Shared, locally authored regression inputs for tests and SVG inspection.

use std::collections::BTreeSet;

/// One named molecular skeleton and its graph-theoretic cycle count.
pub struct Case {
  /// Safe, unique file stem for generated visual artifacts.
  pub name: &'static str,
  /// Molecular notation; aromatic input uses the shared domain representation model.
  pub source: &'static str,
  /// E - V + component count, not an aromatic ring count.
  pub rank: usize,
}

/// Loads the fixed regression corpus without reading external files at runtime.
///
/// # Returns
/// Cases in source order, shared by geometry assertions and the visual example.
/// File stems contain only lowercase ASCII letters, digits, and underscores.
///
/// # Errors
/// Rejects malformed rows, duplicate/unsafe names, invalid ranks, or empty input.
pub fn load() -> Result<Vec<Case>, Box<dyn std::error::Error>> {
  let mut names = BTreeSet::new();
  let mut cases = Vec::new();
  for (index, line) in include_str!("../fixtures/layout.tsv").lines().enumerate() {
    if line.is_empty() || line.starts_with('#') {
      continue;
    }
    let fields: Vec<_> = line.split('\t').collect();
    let [name, source, rank] = fields.as_slice() else {
      return Err(format!("malformed layout case on line {}", index + 1).into());
    };
    if name.is_empty()
      || !name
        .bytes()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
      || !names.insert(*name)
    {
      return Err(format!("invalid layout case name on line {}", index + 1).into());
    }
    cases.push(Case {
      name,
      source,
      rank: rank.parse()?,
    });
  }
  if cases.is_empty() {
    return Err("empty layout regression corpus".into());
  }
  Ok(cases)
}
