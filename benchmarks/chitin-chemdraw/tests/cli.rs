//! End-to-end corpus accounting and artifact safety with the real worker binary.

use anyhow::Result;
use serde_json::Value;
use std::{fs, process::Command};

#[test]
fn automatic_worker_should_measure_quality_triggered_reembedding() -> Result<()> {
  use std::io::Write;
  use std::process::Stdio;

  let source = include_str!("../../../crates/chitin-chemdraw/tests/fixtures/assembly.tsv")
    .lines()
    .nth(2)
    .and_then(|line| line.split_once('\t'))
    .ok_or_else(|| anyhow::anyhow!("missing constrained-cycle fixture"))?
    .1;
  let mut child = Command::new(env!("CARGO_BIN_EXE_chitin-chemdraw-benchmark"))
    .arg("--worker")
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .spawn()?;
  let request = serde_json::json!({
    "case": { "row": 1, "id": "constrained-cycle", "smiles": source },
    "max_atoms": 128, "svg_path": null, "label_feedback": true
  });
  {
    let mut input = child
      .stdin
      .take()
      .ok_or_else(|| anyhow::anyhow!("missing worker input"))?;
    input.write_all(&serde_json::to_vec(&request)?)?;
  }
  let output = child.wait_with_output()?;
  assert!(output.status.success());
  let line = std::str::from_utf8(&output.stdout)?
    .lines()
    .last()
    .ok_or_else(|| anyhow::anyhow!("missing worker output"))?;
  let event: Value = serde_json::from_str(line)?;
  assert_eq!(event["event"], "finished");
  assert_eq!(event["record"]["metrics"]["readability_clear"], true);
  assert_eq!(event["record"]["constraint_closure"], false);
  Ok(())
}

#[test]
fn reports_failures_without_discarding_source_records() -> Result<()> {
  let directory = tempfile::tempdir()?;
  fs::write(
    directory.path().join("cases.csv"),
    "id,smiles\nvalid,CCO\ninvalid,C(\nempty,\n",
  )?;
  let config = serde_json::json!({
    "dataset": "test", "version": "1", "license": "test", "source_url": "test",
    "input": "cases.csv", "output": "results", "delimiter": ",", "headers": true,
    "smiles_column": "smiles", "id_column": "id", "sample_size": 0, "seed": 1,
    "timeout_seconds": 5, "max_atoms": 100, "save_svg_count": 1
  });
  let path = directory.path().join("config.yaml");
  fs::write(&path, serde_json::to_vec(&config)?)?;
  let output = Command::new(env!("CARGO_BIN_EXE_chitin-chemdraw-benchmark"))
    .arg(&path)
    .output()?;
  assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
  let summary: Value = serde_json::from_slice(&fs::read(directory.path().join("results/summary.json"))?)?;
  assert_eq!(summary["cases"], 3);
  assert_eq!(summary["success"], 1);
  assert_eq!(summary["outcomes"]["error:parse"], 2);
  assert_eq!(summary["quality_version"], "diagram-quality-v3");
  assert_eq!(summary["quality"]["selected_cases"], 3);
  assert_eq!(summary["quality"]["generated_cases"], 1);
  assert_eq!(
    summary["quality"]["metrics"]["tetrahedral_depiction_parity"]["not_applicable_cases"],
    1
  );
  assert_eq!(
    summary["quality"]["metrics"]["chemical_label_semantics"]["not_implemented_cases"],
    1
  );
  assert_eq!(
    summary["quality"]["metrics"]["chemical_label_semantics"]["distribution"]["p50"],
    Value::Null
  );
  assert!(directory.path().join("results/svg/00000001.svg").is_file());
  // Lazy previews must execute the measured renderer, not a subsequently rebuilt target.
  let manifest: Value = serde_json::from_slice(&fs::read(directory.path().join("results/manifest.json"))?)?;
  assert_eq!(manifest["label_feedback"], false);
  assert_eq!(manifest["renderer_executable"], "renderer");
  assert_eq!(
    fs::read(directory.path().join("results/renderer"))?,
    fs::read(env!("CARGO_BIN_EXE_chitin-chemdraw-benchmark"))?
  );
  // A repeat must not overwrite the existing report or the first run's artifacts.
  assert!(
    !Command::new(env!("CARGO_BIN_EXE_chitin-chemdraw-benchmark"))
      .arg(&path)
      .output()?
      .status
      .success()
  );
  Ok(())
}

#[test]
fn label_feedback_should_be_explicit_and_reproducible_in_the_frozen_worker() -> Result<()> {
  let directory = tempfile::tempdir()?;
  fs::write(
    directory.path().join("cases.csv"),
    "id,smiles\ncompact,COC(=O)[C@H](OC(C)=O)C1=CC=C(N)C=C1C(=O)O\n",
  )?;
  let config = serde_json::json!({
    "dataset": "test", "version": "1", "license": "test", "source_url": "test",
    "input": "cases.csv", "output": "results", "delimiter": ",", "headers": true,
    "smiles_column": "smiles", "id_column": "id", "sample_size": 0, "seed": 1,
    "timeout_seconds": 5, "max_atoms": 100, "save_svg_count": 1, "label_feedback": true
  });
  let path = directory.path().join("config.yaml");
  fs::write(&path, serde_json::to_vec(&config)?)?;
  let output = Command::new(env!("CARGO_BIN_EXE_chitin-chemdraw-benchmark"))
    .arg(&path)
    .output()?;
  assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
  let manifest: Value = serde_json::from_slice(&fs::read(directory.path().join("results/manifest.json"))?)?;
  assert_eq!(manifest["label_feedback"], true);
  assert_eq!(manifest["config"]["label_feedback"], true);
  assert!(
    manifest["pipeline"]
      .as_str()
      .is_some_and(|text| text.contains("pipeline::generate"))
  );
  let line = fs::read_to_string(directory.path().join("results/cases.jsonl"))?;
  let record: Value = serde_json::from_str(line.trim())?;
  assert!(record["timings_ms"]["automatic_drawing"].is_number());
  assert!(record["timings_ms"]["layout"].is_null());
  assert!(record["timings_ms"]["depict"].is_null());

  let replay = directory.path().join("replay.svg");
  let mut child = Command::new(directory.path().join("results/renderer"))
    .arg("--worker")
    .stdin(std::process::Stdio::piped())
    .stdout(std::process::Stdio::piped())
    .spawn()?;
  {
    use std::io::Write;
    let mut input = child
      .stdin
      .take()
      .ok_or_else(|| anyhow::anyhow!("missing worker input"))?;
    input.write_all(&serde_json::to_vec(&serde_json::json!({
      "case": record["case"], "max_atoms": 100, "svg_path": replay, "label_feedback": true
    }))?)?;
  }
  assert!(child.wait_with_output()?.status.success());
  assert_eq!(
    fs::read(replay)?,
    fs::read(directory.path().join("results/svg/00000001.svg"))?
  );
  Ok(())
}
