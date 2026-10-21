//! End-to-end corpus accounting and artifact safety with the real worker binary.

use anyhow::Result;
use serde_json::Value;
use std::{fs, process::Command};

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
  assert_eq!(summary["quality_version"], "diagram-quality-v2");
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
