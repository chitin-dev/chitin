//! Streaming delimited input and deterministic, bounded-memory corpus sampling.

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BinaryHeap, fs::File, io::Read, path::PathBuf};

/// A pinned source and explicit column mapping; paths are relative to the config.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
  pub dataset: String,
  pub version: String,
  pub license: String,
  pub source_url: String,
  pub input: PathBuf,
  pub output: PathBuf,
  pub delimiter: char,
  pub headers: bool,
  /// Names with headers enabled, otherwise zero-based decimal column indices.
  pub smiles_column: String,
  pub id_column: String,
  /// Zero selects the complete corpus; otherwise select the lowest seeded hashes.
  pub sample_size: usize,
  pub seed: u64,
  pub timeout_seconds: u64,
  pub max_atoms: usize,
  pub save_svg_count: usize,
}

/// Original data-record identity; row numbers exclude comments and the header.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
pub struct Case {
  pub row: usize,
  pub id: String,
  pub smiles: String,
}

/// SHA-256 of the original input bytes, independent of CSV normalization.
pub fn checksum(path: &std::path::Path) -> Result<String> {
  let mut file = File::open(path)?;
  let mut digest = Sha256::new();
  let mut buffer = [0; 65536];
  loop {
    let count = file.read(&mut buffer)?;
    if count == 0 {
      break;
    }
    digest.update(&buffer[..count]);
  }
  Ok(format!("{:x}", digest.finalize()))
}

/// Visits every source record without chemistry-based filtering.
///
/// Malformed delimited data is a fatal input error, not a molecule rejection.
/// Missing/empty SMILES remain cases and are reported by the worker. Quoted CSV
/// fields and embedded newlines are decoded by the CSV reader, never split by hand.
pub fn visit(config: &Config, mut consume: impl FnMut(Case) -> Result<()>) -> Result<usize> {
  ensure!(config.delimiter.is_ascii(), "delimiter must be ASCII");
  let mut reader = csv::ReaderBuilder::new()
    .delimiter(config.delimiter as u8)
    .has_headers(config.headers)
    .comment(Some(b'#'))
    .from_path(&config.input)?;
  let headers = reader.headers()?.clone();
  let column = |name: &str| -> Result<usize> {
    if config.headers {
      headers
        .iter()
        .position(|h| h == name)
        .with_context(|| format!("missing column {name:?}"))
    } else {
      name.parse().context("headerless input requires numeric column indices")
    }
  };
  let smiles = column(&config.smiles_column)?;
  let id = column(&config.id_column)?;
  let mut count = 0;
  for record in reader.records() {
    let record = record?;
    count += 1;
    consume(Case {
      row: count,
      id: record.get(id).context("missing ID field")?.to_owned(),
      smiles: record.get(smiles).context("missing SMILES field")?.to_owned(),
    })?;
  }
  Ok(count)
}

/// Selects a reproducible sample in O(N log K) time and O(K) storage.
///
/// Selection hashes the seed and original record number, not chemical features.
/// Duplicates remain independent source records. Sorting selected rows restores
/// source order; the emitted manifest identifies the exact sample for comparisons.
pub fn sample(config: &Config) -> Result<(usize, Vec<Case>)> {
  ensure!(config.sample_size > 0, "sampling requires a positive sample_size");
  let mut heap = BinaryHeap::new();
  let total = visit(config, |case| {
    let mut digest = Sha256::new();
    digest.update(config.seed.to_le_bytes());
    digest.update((case.row as u64).to_le_bytes());
    let key: [u8; 32] = digest.finalize().into();
    heap.push((key, case));
    if heap.len() > config.sample_size {
      heap.pop();
    }
    Ok(())
  })?;
  let mut cases: Vec<_> = heap.into_iter().map(|(_, case)| case).collect();
  cases.sort_by_key(|case| case.row);
  Ok((total, cases))
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::io::Write;

  #[test]
  fn sampling_is_reproducible_and_retains_quoted_csv_fields() -> Result<()> {
    let mut file = tempfile::NamedTempFile::new()?;
    write!(file, "id,smiles\n\"a,b\",CC\nc,CCC\nd,CCCC\n")?;
    let config = Config {
      dataset: "test".into(),
      version: "1".into(),
      license: "test".into(),
      source_url: "test".into(),
      input: file.path().into(),
      output: PathBuf::new(),
      delimiter: ',',
      headers: true,
      smiles_column: "smiles".into(),
      id_column: "id".into(),
      sample_size: 3,
      seed: 42,
      timeout_seconds: 1,
      max_atoms: 100,
      save_svg_count: 0,
    };
    let first = sample(&config)?;
    assert_eq!(first, sample(&config)?);
    assert_eq!(first.1[0].id, "a,b");
    let smaller = Config {
      sample_size: 1,
      ..config
    };
    let selected = sample(&smaller)?;
    assert_eq!(selected.1.len(), 1);
    assert_eq!(selected, sample(&smaller)?);
    assert_eq!(selected.0, 3);
    Ok(())
  }
}
