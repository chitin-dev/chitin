//! Same-runner release-profile checks; not an absolute FPS or startup benchmark.
//!
//! Usage: chitin-release-audit CRITERION_ROOT BASELINE CANDIDATE BEFORE_EXE
//! AFTER_EXE REPORT_JSON. Criterion runs must use distinct `--save-baseline`
//! names and a fresh shared `CRITERION_HOME` for each comparison.

use std::{collections::BTreeMap, env, fs, path::Path};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};

/// Maximum permitted increase in each benchmark's median execution time.
const MAX_SLOWDOWN: f64 = 0.05;

#[derive(Deserialize)]
struct Estimate {
  point_estimate: f64,
}

#[derive(Deserialize)]
struct Estimates {
  median: Estimate,
}

#[derive(Serialize)]
struct Timing {
  benchmark: String,
  baseline_ns: f64,
  candidate_ns: f64,
  ratio: f64,
  passed: bool,
}

#[derive(Serialize)]
struct Report {
  baseline_bytes: u64,
  candidate_bytes: u64,
  max_slowdown: f64,
  timings: Vec<Timing>,
  passed: bool,
}

/// Reads saved Criterion medians without including `new` or `change` results.
///
/// # Parameters
///
/// * `root` is a fresh Criterion output directory shared by both profiles.
/// * `label` is the corresponding `--save-baseline` name.
///
/// # Returns
///
/// Benchmark paths mapped to positive finite timings, or an error for malformed
/// input. Symlinks are not followed while walking the output directory.
fn read_timings(root: &Path, label: &str) -> Result<BTreeMap<String, f64>> {
  ensure!(
    !label.is_empty() && Path::new(label).components().count() == 1 && label != "." && label != "..",
    "baseline names must be single directory names"
  );
  let mut pending = vec![root.to_owned()];
  let mut timings = BTreeMap::new();
  while let Some(directory) = pending.pop() {
    for entry in fs::read_dir(&directory).with_context(|| format!("read {}", directory.display()))? {
      let entry = entry?;
      if !entry.file_type()?.is_dir() {
        continue;
      }
      let path = entry.path();
      if entry.file_name() == label {
        let estimates_path = path.join("estimates.json");
        if estimates_path.is_file() {
          let estimates: Estimates = serde_json::from_slice(&fs::read(&estimates_path)?)
            .with_context(|| format!("parse {}", estimates_path.display()))?;
          let median = estimates.median.point_estimate;
          ensure!(
            median.is_finite() && median > 0.0,
            "invalid timing in {}",
            estimates_path.display()
          );
          let name = directory.strip_prefix(root)?.to_string_lossy().replace('\\', "/");
          ensure!(!name.is_empty(), "benchmark name is missing");
          ensure!(
            timings.insert(name.clone(), median).is_none(),
            "duplicate benchmark {name}"
          );
        } else {
          // Benchmark groups can have the same name as a saved baseline.
          pending.push(path);
        }
      } else {
        pending.push(path);
      }
    }
  }
  ensure!(!timings.is_empty(), "no saved Criterion timings for {label}");
  Ok(timings)
}

/// Compares the same complete set of benchmarks, failing closed on missing data.
fn compare_timings(baseline: &BTreeMap<String, f64>, candidate: &BTreeMap<String, f64>) -> Result<Vec<Timing>> {
  ensure!(!baseline.is_empty(), "benchmark set must not be empty");
  ensure!(baseline.keys().eq(candidate.keys()), "profile benchmark sets differ");
  baseline
    .iter()
    .zip(candidate.values())
    .map(|((name, before), after)| {
      ensure!(
        before.is_finite() && *before > 0.0 && after.is_finite() && *after > 0.0,
        "invalid median for {name}"
      );
      let ratio = after / before;
      ensure!(ratio.is_finite(), "invalid ratio for {name}");
      Ok(Timing {
        benchmark: name.clone(),
        baseline_ns: *before,
        candidate_ns: *after,
        ratio,
        passed: ratio <= 1.0 + MAX_SLOWDOWN,
      })
    })
    .collect()
}

/// Requires a strictly smaller executable and bounded per-benchmark slowdown.
fn check_budget(baseline_bytes: u64, candidate_bytes: u64, timings: &[Timing]) -> bool {
  baseline_bytes > 0
    && candidate_bytes > 0
    && candidate_bytes < baseline_bytes
    && !timings.is_empty()
    && timings.iter().all(|timing| timing.passed)
}

fn main() -> Result<()> {
  let arguments: Vec<_> = env::args_os().skip(1).collect();
  let [root, baseline, candidate, before, after, output] = arguments.as_slice() else {
    bail!("usage: chitin-release-audit CRITERION_ROOT BASELINE CANDIDATE BEFORE_EXE AFTER_EXE REPORT_JSON");
  };
  ensure!(baseline != candidate, "profile labels must differ");
  let baseline = baseline.to_str().context("baseline name must be UTF-8")?;
  let candidate = candidate.to_str().context("candidate name must be UTF-8")?;
  let timings = compare_timings(
    &read_timings(Path::new(root), baseline)?,
    &read_timings(Path::new(root), candidate)?,
  )?;
  // A partially executed benchmark script must not produce a passing release.
  for required in ["native_structure_parse_512_atoms", "native_molecule_frame_512_atoms"] {
    ensure!(
      timings.iter().any(|timing| timing.benchmark == required),
      "missing benchmark {required}"
    );
  }
  for prefix in ["sequence_", "kit_tree_", "molecular_surface"] {
    ensure!(
      timings.iter().any(|timing| timing.benchmark.starts_with(prefix)),
      "missing benchmark family {prefix}"
    );
  }
  let baseline_bytes = fs::metadata(before)?.len();
  let candidate_bytes = fs::metadata(after)?.len();
  let report = Report {
    baseline_bytes,
    candidate_bytes,
    max_slowdown: MAX_SLOWDOWN,
    passed: check_budget(baseline_bytes, candidate_bytes, &timings),
    timings,
  };
  let output = Path::new(output);
  if let Some(parent) = output.parent().filter(|parent| !parent.as_os_str().is_empty()) {
    fs::create_dir_all(parent)?;
  }
  fs::write(output, serde_json::to_vec_pretty(&report)?)?;

  println!("## Native release comparison\n");
  println!("Executable: {baseline_bytes} → {candidate_bytes} bytes. Per-benchmark slowdown budget: 5%.\n");
  println!("| Benchmark | Baseline (ns) | Candidate (ns) | Change | Result |");
  println!("|---|---:|---:|---:|---|");
  for timing in &report.timings {
    println!(
      "| {} | {:.1} | {:.1} | {:+.2}% | {} |",
      timing.benchmark,
      timing.baseline_ns,
      timing.candidate_ns,
      (timing.ratio - 1.0) * 100.0,
      if timing.passed { "pass" } else { "FAIL" }
    );
  }
  ensure!(
    report.passed,
    "release size/performance budget failed; see {}",
    output.display()
  );
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn missing_benchmarks_are_not_silently_ignored() {
    let baseline = BTreeMap::from([("render".to_owned(), 100.0)]);
    assert!(compare_timings(&baseline, &BTreeMap::new()).is_err());
  }

  #[test]
  fn regression_above_five_percent_fails() -> Result<()> {
    let baseline = BTreeMap::from([("render".to_owned(), 100.0)]);
    let candidate = BTreeMap::from([("render".to_owned(), 106.0)]);
    assert!(!check_budget(100, 90, &compare_timings(&baseline, &candidate)?));
    Ok(())
  }

  #[test]
  fn smaller_binary_with_small_timing_variation_passes() -> Result<()> {
    let baseline = BTreeMap::from([("render".to_owned(), 100.0)]);
    let candidate = BTreeMap::from([("render".to_owned(), 102.0)]);
    assert!(check_budget(100, 90, &compare_timings(&baseline, &candidate)?));
    Ok(())
  }

  #[test]
  fn unchanged_artifact_size_fails() -> Result<()> {
    let timings = BTreeMap::from([("render".to_owned(), 100.0)]);
    assert!(!check_budget(100, 100, &compare_timings(&timings, &timings)?));
    Ok(())
  }

  #[test]
  fn invalid_measurements_fail_closed() {
    let baseline = BTreeMap::from([("render".to_owned(), 0.0)]);
    assert!(compare_timings(&baseline, &baseline).is_err());
  }

  #[test]
  fn saved_medians_are_read_without_current_run_results() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    for (label, median) in [("baseline", 100.0), ("new", 999.0)] {
      let path = temporary.path().join("render").join(label);
      fs::create_dir_all(&path)?;
      fs::write(
        path.join("estimates.json"),
        serde_json::to_vec(&serde_json::json!({ "median": { "point_estimate": median } }))?,
      )?;
    }
    assert_eq!(
      read_timings(temporary.path(), "baseline")?,
      BTreeMap::from([("render".to_owned(), 100.0)])
    );
    Ok(())
  }

  #[test]
  fn missing_saved_results_are_an_error() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    assert!(read_timings(temporary.path(), "baseline").is_err());
    Ok(())
  }

  #[test]
  fn parent_directory_cannot_be_used_as_a_label() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    assert!(read_timings(temporary.path(), "..").is_err());
    Ok(())
  }
}
