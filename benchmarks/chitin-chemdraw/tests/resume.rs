//! Resume is prefix recovery, not a retry policy or permission to mix algorithms.

use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::{
  fs::{self, File},
  io::Write,
  path::Path,
  process::{Command, Output},
};

/// Creates a real run containing successes and durable parse failures.
fn run_fixture(sample: usize) -> Result<tempfile::TempDir> {
  let directory = tempfile::tempdir()?;
  fs::write(
    directory.path().join("cases.csv"),
    "id,smiles\nvalid,CCO\nbad,C(\nempty,\nring,C1CC1\n",
  )?;
  let config = json!({"dataset":"test", "version":"1", "license":"test", "source_url":"test",
    "input":"cases.csv", "output":"results", "delimiter":",", "headers":true,
    "smiles_column":"smiles", "id_column":"id", "sample_size":sample, "seed":42,
    "timeout_seconds":5, "max_atoms":100, "save_svg_count":4});
  fs::write(directory.path().join("config.yaml"), serde_json::to_vec(&config)?)?;
  let output = invoke(directory.path(), &[])?;
  assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
  Ok(directory)
}

/// Runs the controller independently from the frozen per-case executable.
fn invoke(directory: &Path, flags: &[&str]) -> Result<Output> {
  Ok(
    Command::new(env!("CARGO_BIN_EXE_chitin-chemdraw-benchmark"))
      .arg(directory.join("config.yaml"))
      .args(flags)
      .output()?,
  )
}

/// Captures only user-visible checkpoint bytes; lock acquisition must not change them.
fn checkpoint_bytes(directory: &Path) -> Result<Vec<Vec<u8>>> {
  ["manifest.json", "cases.jsonl", "sample.jsonl", "summary.json"]
    .iter()
    .map(|name| Ok(fs::read(directory.join("results").join(name))?))
    .collect()
}

#[test]
fn resume_should_recover_a_pending_case_without_repeating_completed_failures() -> Result<()> {
  for sample in [0, 4] {
    let directory = run_fixture(sample)?;
    let results = directory.path().join("results");
    let original_manifest = fs::read(results.join("manifest.json"))?;
    let original_summary: Value = serde_json::from_slice(&fs::read(results.join("summary.json"))?)?;
    let original_cases = fs::read_to_string(results.join("cases.jsonl"))?;
    let prefix = original_cases
      .lines()
      .take(3)
      .map(|line| format!("{line}\n"))
      .collect::<String>();
    let saved_sample = fs::read(results.join("sample.jsonl"))?;
    let saved_svg = fs::read(results.join("svg/00000001.svg"))?;
    fs::write(results.join("cases.jsonl"), format!("{prefix}{{\"case\":"))?;
    fs::write(results.join("summary.json"), b"{interrupted summary")?;
    fs::write(results.join("svg/00000004.svg"), b"unfinished SVG")?;
    let output = invoke(directory.path(), &["--resume"])?;
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let cases = fs::read_to_string(results.join("cases.jsonl"))?;
    assert!(cases.starts_with(&prefix), "completed observations were modified");
    assert_eq!(cases.lines().count(), 4);
    let rows: Vec<_> = cases
      .lines()
      .map(serde_json::from_str::<Value>)
      .collect::<std::result::Result<_, _>>()?;
    assert_eq!(
      rows.iter().map(|r| r["case"]["row"].as_u64()).collect::<Vec<_>>(),
      vec![Some(1), Some(2), Some(3), Some(4)]
    );
    let summary: Value = serde_json::from_slice(&fs::read(results.join("summary.json"))?)?;
    assert_eq!(summary["cases"], 4);
    assert_eq!(summary["success"], 2);
    assert_eq!(summary["outcomes"]["error:parse"], 2);
    assert_eq!(summary["quality"], original_summary["quality"]);
    assert_eq!(fs::read(results.join("sample.jsonl"))?, saved_sample);
    assert_eq!(fs::read(results.join("manifest.json"))?, original_manifest);
    assert_eq!(fs::read(results.join("svg/00000001.svg"))?, saved_svg);
    let backup = fs::read_dir(results.join("recovery"))?
      .next()
      .context("no recovery directory")??
      .path();
    assert_eq!(fs::read(backup.join("cases.uncommitted-tail"))?, b"{\"case\":");
    assert_eq!(fs::read(backup.join("00000004.svg"))?, b"unfinished SVG");
    assert_eq!(fs::read(backup.join("sample.jsonl"))?, saved_sample);
    let rerun = invoke(directory.path(), &["--resume"])?;
    assert!(rerun.status.success());
    assert_eq!(fs::read_to_string(results.join("cases.jsonl"))?, cases);
    assert_eq!(fs::read_dir(results.join("recovery"))?.count(), 1);
  }
  Ok(())
}

#[test]
fn check_should_validate_without_reconciling_or_writing_checkpoint_files() -> Result<()> {
  let directory = run_fixture(4)?;
  let mut file = fs::OpenOptions::new()
    .append(true)
    .open(directory.path().join("results/cases.jsonl"))?;
  file.write_all(b"{unfinished")?;
  let before = checkpoint_bytes(directory.path())?;
  let output = invoke(directory.path(), &["--resume", "--check", "--output", "results"])?;
  assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
  assert_eq!(checkpoint_bytes(directory.path())?, before);
  assert!(!directory.path().join("results/recovery").exists());
  Ok(())
}

#[test]
fn immutable_config_or_input_drift_should_fail_without_touching_results() -> Result<()> {
  let directory = run_fixture(4)?;
  let path = directory.path().join("config.yaml");
  let original = fs::read(&path)?;
  let before = checkpoint_bytes(directory.path())?;
  for (field, value) in [
    ("seed", json!(7)),
    ("timeout_seconds", json!(6)),
    ("max_atoms", json!(99)),
    ("save_svg_count", json!(2)),
    ("label_feedback", json!(true)),
    ("sample_size", json!(3)),
  ] {
    let mut config: Value = serde_json::from_slice(&original)?;
    config[field] = value;
    fs::write(&path, serde_json::to_vec(&config)?)?;
    let output = invoke(directory.path(), &["--resume"])?;
    assert!(!output.status.success(), "drift accepted: {field}");
    assert_eq!(checkpoint_bytes(directory.path())?, before);
  }
  fs::write(&path, original)?;
  fs::write(directory.path().join("cases.csv"), "id,smiles\nchanged,CCN\n")?;
  let output = invoke(directory.path(), &["--resume"])?;
  assert!(!output.status.success());
  assert!(String::from_utf8_lossy(&output.stderr).contains("input checksum mismatch"));
  assert_eq!(checkpoint_bytes(directory.path())?, before);
  Ok(())
}

#[test]
fn corrupt_committed_lines_and_duplicate_source_rows_should_not_be_truncated() -> Result<()> {
  let directory = run_fixture(0)?;
  let path = directory.path().join("results/cases.jsonl");
  let original = fs::read_to_string(&path)?;
  for corrupt in [
    format!("{original}{{bad\n"),
    original.lines().next().context("no first result")?.to_owned() + "\n" + &original,
  ] {
    fs::write(&path, corrupt)?;
    let before = checkpoint_bytes(directory.path())?;
    assert!(!invoke(directory.path(), &["--resume"])?.status.success());
    assert_eq!(checkpoint_bytes(directory.path())?, before);
    assert!(!directory.path().join("results/recovery").exists());
  }
  Ok(())
}

#[test]
fn modified_frozen_renderer_should_fail_integrity_validation() -> Result<()> {
  let directory = run_fixture(4)?;
  let path = directory.path().join("results/renderer");
  fs::OpenOptions::new().append(true).open(path)?.write_all(b"changed")?;
  let before = checkpoint_bytes(directory.path())?;
  let output = invoke(directory.path(), &["--resume"])?;
  assert!(!output.status.success());
  assert!(String::from_utf8_lossy(&output.stderr).contains("frozen renderer checksum mismatch"));
  assert_eq!(checkpoint_bytes(directory.path())?, before);
  Ok(())
}

#[test]
fn a_concurrent_controller_should_not_acquire_the_run_lock() -> Result<()> {
  let directory = run_fixture(0)?;
  let lock = File::open(directory.path().join("results/manifest.json"))?;
  lock.try_lock()?;
  let before = checkpoint_bytes(directory.path())?;
  let output = invoke(directory.path(), &["--resume", "--check"])?;
  assert!(!output.status.success());
  assert!(String::from_utf8_lossy(&output.stderr).contains("another controller"));
  assert_eq!(checkpoint_bytes(directory.path())?, before);
  Ok(())
}
