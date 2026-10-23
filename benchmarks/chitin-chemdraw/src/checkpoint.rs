//! Read-only checkpoint validation followed by recoverable journal reconciliation.

use crate::{
  dataset::{self, Case, Config, LayoutEngine},
  quality,
  report::Summary,
  worker::Record,
};
use anyhow::{Context, Result, ensure};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use sha2::Digest;
use std::{
  fs::{self, File, OpenOptions},
  io::{BufRead, BufReader, Read, Seek, SeekFrom, Write},
  path::{Path, PathBuf},
  time::Duration,
};

/// Holds the exclusive advisory lock until the controller exits or is interrupted.
///
/// A persistent manifest inode avoids stale lock files and automatic lock stealing.
/// Older controllers without this lock must be stopped before resuming their runs.
pub fn lock(directory: &Path, resume: bool) -> Result<File> {
  let path = directory.join("manifest.json");
  let file = if resume {
    File::open(path)?
  } else {
    File::create_new(path)?
  };
  file
    .try_lock()
    .context("another controller is using this benchmark directory")?;
  Ok(file)
}

/// A journal's complete prefix; a final unterminated line is never committed.
struct Journal<T> {
  entries: Vec<T>,
  committed_bytes: u64,
  bytes: u64,
}

/// Recovery metadata contains no open append handles until validation has passed.
pub struct Recovery {
  pub summary: Summary,
  pub manifest: Value,
  pub renderer: PathBuf,
  pub elapsed: Duration,
  pub total: usize,
  pub clean: bool,
  completed: Vec<Case>,
  results_bytes: u64,
  results_committed: u64,
  samples_bytes: u64,
  backup: Option<PathBuf>,
}

/// Parses complete records strictly; a malformed newline-terminated record is corruption.
fn journal<T: DeserializeOwned>(path: &Path) -> Result<Journal<T>> {
  ensure!(
    fs::symlink_metadata(path)?.file_type().is_file(),
    "journal must be a regular file: {}",
    path.display()
  );
  let mut reader = BufReader::new(File::open(path)?);
  let mut buffer = Vec::new();
  let mut entries = Vec::new();
  let mut committed_bytes = 0;
  loop {
    buffer.clear();
    if reader.read_until(b'\n', &mut buffer)? == 0 {
      break;
    }
    if !buffer.ends_with(b"\n") {
      break;
    }
    entries.push(
      serde_json::from_slice(&buffer)
        .with_context(|| format!("corrupt committed record {} in {}", entries.len() + 1, path.display()))?,
    );
    committed_bytes += buffer.len() as u64;
  }
  Ok(Journal {
    entries,
    committed_bytes,
    bytes: reader.get_ref().metadata()?.len(),
  })
}

/// Checks immutable run identity before touching journals, artifacts or statistics.
///
/// # Parameters
/// - `config`, `input_hash`: the resolved requested configuration and source bytes.
/// - `sample`: recomputed seeded sample, or `None` for a streamed complete corpus.
///
/// # Returns
/// Rebuilt statistics and a validated source-order prefix. Old failures and timeouts
/// are completed observations. Missing/stale/corrupt summary JSON does not lose cases.
///
/// # Errors
/// Configuration drift, changed source/provider/renderer, duplicate or out-of-order
/// cases, and corruption inside either committed journal are rejected without edits.
pub fn inspect(config: &Config, input_hash: &str, sample: Option<&[Case]>) -> Result<Recovery> {
  let manifest: Value = serde_json::from_reader(File::open(config.output.join("manifest.json"))?)?;
  let stored: Config = serde_json::from_value(manifest["config"].clone())?;
  let comparable = |config: &Config| -> Result<Value> {
    let mut value = serde_json::to_value(config)?;
    // Relocation is safe only after validating the actual input/provider hashes.
    if let Some(object) = value.as_object_mut() {
      for key in ["input", "output", "external_python"] {
        object.remove(key);
      }
    }
    Ok(value)
  };
  ensure!(
    comparable(&stored)? == comparable(config)?,
    "resume configuration differs from the original run"
  );
  ensure!(manifest["input_sha256"] == input_hash, "resume input checksum mismatch");
  ensure!(
    manifest["sample_algorithm"] == "lowest_sha256(seed_le_u64,row_le_u64)",
    "unsupported historical sampling algorithm"
  );
  ensure!(
    manifest["quality_spec_sha256"] == format!("{:x}", sha2::Sha256::digest(quality::SPECIFICATION.as_bytes())),
    "resume quality specification mismatch"
  );
  ensure!(
    manifest["architecture"] == std::env::consts::ARCH && manifest["os"] == std::env::consts::OS,
    "resume platform mismatch"
  );
  ensure!(
    manifest["renderer_executable"] == "renderer",
    "unsupported frozen renderer location"
  );
  let renderer = config.output.join("renderer").canonicalize()?;
  ensure!(
    fs::symlink_metadata(config.output.join("renderer"))?
      .file_type()
      .is_file(),
    "frozen renderer must be a regular file"
  );
  ensure!(
    manifest["executable_sha256"] == dataset::checksum(&renderer)?,
    "frozen renderer checksum mismatch"
  );
  if config.layout_engine == LayoutEngine::RdkitCoordgen {
    if let Some(hash) = manifest["external_adapter_sha256"].as_str() {
      ensure!(
        hash == crate::external::adapter_checksum(),
        "external adapter changed; restore the original adapter before resuming"
      );
    } else {
      ensure!(
        crate::external::matches_legacy_adapter(&renderer)?,
        "legacy external adapter does not match the frozen renderer"
      );
    }
    let python = config
      .external_python
      .as_deref()
      .context("missing external interpreter")?;
    ensure!(
      manifest["external_provider"] == crate::external::probe(python)?,
      "external provider changed since the original run"
    );
  }
  let results: Journal<Record> = journal(&config.output.join("cases.jsonl"))?;
  let samples: Journal<Case> = journal(&config.output.join("sample.jsonl"))?;
  ensure!(
    samples.entries.len() <= results.entries.len() + 1,
    "sample journal is more than one observation ahead"
  );
  let mut index = 0;
  let mut check = |case: &Case| -> Result<()> {
    if let Some(record) = results.entries.get(index) {
      ensure!(
        record.case == *case,
        "result source-prefix mismatch at observation {}",
        index + 1
      );
    }
    if let Some(saved) = samples.entries.get(index) {
      ensure!(
        saved == case,
        "sample source-prefix mismatch at observation {}",
        index + 1
      );
    }
    index += 1;
    Ok(())
  };
  if let Some(sample) = sample {
    for case in sample {
      check(case)?;
    }
  } else {
    dataset::visit(config, |case| check(&case))?;
  }
  ensure!(
    results.entries.len() <= index && samples.entries.len() <= index,
    "journals extend beyond the selected source"
  );
  let previous: Option<Value> = File::open(config.output.join("summary.json"))
    .ok()
    .and_then(|file| serde_json::from_reader(file).ok());
  let checkpoint = previous
    .as_ref()
    .and_then(|v| {
      Some((
        usize::try_from(v["cases"].as_u64()?).ok()?,
        v["elapsed_seconds_including_sampling_and_spawn"].as_f64()?,
      ))
    })
    .filter(|&(count, seconds)| count <= results.entries.len() && Duration::try_from_secs_f64(seconds).is_ok());
  let mut summary = Summary::default();
  let mut wall_seconds = checkpoint.map_or(0., |(_, seconds)| seconds);
  for (i, record) in results.entries.iter().enumerate() {
    ensure!(
      ["success", "error", "timeout", "crash"].contains(&record.status.as_str()),
      "record is not a terminal observation"
    );
    if let Some(metrics) = &record.metrics {
      ensure!(
        metrics.version == quality::VERSION,
        "mixed quality protocols in checkpoint"
      );
    }
    ensure!(
      record.wall_ms.is_finite() && record.wall_ms >= 0.,
      "invalid recorded wall time"
    );
    if i >= checkpoint.map_or(0, |(count, _)| count) {
      wall_seconds += record.wall_ms / 1000.;
    }
    summary.add(record);
  }
  let clean = results.bytes == results.committed_bytes
    && samples.bytes == samples.committed_bytes
    && results.entries.len() == samples.entries.len();
  Ok(Recovery {
    summary,
    manifest,
    renderer,
    elapsed: Duration::try_from_secs_f64(wall_seconds)?,
    total: index,
    clean,
    completed: results.entries.into_iter().map(|r| r.case).collect(),
    results_bytes: results.bytes,
    results_committed: results.committed_bytes,
    samples_bytes: samples.bytes,
    backup: None,
  })
}

impl Recovery {
  /// Returns the exact number of durable completed observations, not a summary count.
  pub fn completed(&self) -> usize {
    self.completed.len()
  }

  /// Allocates a persistent recovery directory only when an old file needs preserving.
  fn backup(&mut self, directory: &Path) -> Result<&Path> {
    if self.backup.is_none() {
      let parent = directory.join("recovery");
      fs::create_dir_all(&parent)?;
      self.backup = Some(tempfile::Builder::new().prefix("resume-").tempdir_in(parent)?.keep());
    }
    self.backup.as_deref().context("missing recovery backup directory")
  }

  /// Reconciles uncommitted tails only after every immutable check has passed.
  ///
  /// Original journal bytes and partial artifacts are preserved in `recovery/`.
  /// Completed result lines are never rewritten. Rebuilding the sample journal
  /// removes a pending entry so the interrupted molecule is appended exactly once.
  pub fn apply(&mut self, directory: &Path) -> Result<(File, File)> {
    let results = directory.join("cases.jsonl");
    let samples = directory.join("sample.jsonl");
    ensure!(
      fs::metadata(&results)?.len() == self.results_bytes && fs::metadata(&samples)?.len() == self.samples_bytes,
      "checkpoint changed during validation; stop other writers"
    );
    let backup = self.backup(directory)?.to_path_buf();
    fs::copy(&samples, backup.join("sample.jsonl"))?;
    if self.results_committed < self.results_bytes {
      let mut source = File::open(&results)?;
      source.seek(SeekFrom::Start(self.results_committed))?;
      let mut tail = Vec::new();
      source.read_to_end(&mut tail)?;
      fs::write(backup.join("cases.uncommitted-tail"), tail)?;
      OpenOptions::new()
        .write(true)
        .open(&results)?
        .set_len(self.results_committed)?;
    }
    let mut file = tempfile::NamedTempFile::new_in(directory)?;
    for case in &self.completed {
      crate::write_json_line(file.as_file_mut(), case)?;
    }
    file.persist(&samples)?;
    eprintln!("Checkpoint recovery backup: {}", backup.display());
    Ok((
      OpenOptions::new().append(true).open(results)?,
      OpenOptions::new().append(true).open(samples)?,
    ))
  }

  /// Preserves only artifacts whose source observation has not been committed.
  pub fn preserve_artifact(&mut self, directory: &Path, path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
      Ok(metadata) => {
        ensure!(
          metadata.file_type().is_file(),
          "uncommitted artifact is not a regular file"
        );
        let target = self
          .backup(directory)?
          .join(path.file_name().context("missing artifact name")?);
        fs::rename(path, target)?;
      }
      Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
      Err(error) => return Err(error.into()),
    }
    Ok(())
  }
}

/// Atomic replacement prevents an interrupted summary write from exposing partial JSON.
pub fn save_json(directory: &Path, name: &str, value: &impl Serialize) -> Result<()> {
  let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
  serde_json::to_writer_pretty(&mut temporary, value)?;
  temporary.flush()?;
  temporary.persist(directory.join(name))?;
  Ok(())
}
