//! Builds every benchmark before taking paired, order-balanced measurements.

use std::{
  env, fs,
  io::Write,
  path::{Path, PathBuf},
  process::Command,
};

use anyhow::{Context, Result, ensure};
use serde::Serialize;
use serde_json::Value;

const BENCHMARKS: &[(&str, &str)] = &[
  ("chitin-desktop", "sequence_virtualization"),
  ("chitin-ui", "tree_virtualization"),
  ("chitin-bio", "molecular_surface"),
  ("chitin-molecule-renderer", "native_frame"),
];

// Symmetric order balances gradual clock/thermal drift without discarding runs.
const ORDER: [(&str, usize); 4] = [("baseline", 0), ("optimized", 0), ("optimized", 1), ("baseline", 1)];

#[derive(Serialize)]
struct Executables {
  package: &'static str,
  benchmark: &'static str,
  baseline: PathBuf,
  optimized: PathBuf,
}

/// Finds the executable for the requested bench, not an automatically built bin.
fn executable(messages: &[u8], benchmark: &str) -> Result<Option<PathBuf>> {
  let mut executable = None;
  for line in messages.split(|byte| *byte == b'\n').filter(|line| !line.is_empty()) {
    let message: Value = serde_json::from_slice(line).context("parse Cargo JSON output")?;
    if message["reason"] == "compiler-message"
      && let Some(rendered) = message["message"]["rendered"].as_str()
    {
      eprint!("{rendered}");
    }
    if message["reason"] != "compiler-artifact" || message["target"]["name"] != benchmark {
      continue;
    }
    let is_bench = message["target"]["kind"]
      .as_array()
      .is_some_and(|kinds| kinds.iter().any(|kind| kind == "bench"));
    if !is_bench {
      continue;
    }
    if let Some(path) = message["executable"].as_str() {
      let path = PathBuf::from(path);
      ensure!(
        executable.as_ref().is_none_or(|previous| previous == &path),
        "ambiguous benchmark executable"
      );
      executable = Some(path);
    }
  }
  Ok(executable)
}

/// Builds without measuring and resolves the path using Cargo's artifact stream.
fn build(package: &str, benchmark: &str, profile: &str) -> Result<PathBuf> {
  println!("Prebuilding {package}/{benchmark} with profile {profile}");
  let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
  let output = Command::new(cargo)
    .args([
      "bench",
      "--no-run",
      "--locked",
      "--message-format=json",
      "-p",
      package,
      "--bench",
      benchmark,
      "--profile",
      profile,
    ])
    .output()
    .context("start Cargo benchmark build")?;
  std::io::stderr().write_all(&output.stderr)?;
  let executable = executable(&output.stdout, benchmark)?;
  ensure!(
    output.status.success(),
    "benchmark build failed: {package}/{benchmark} ({profile})"
  );
  executable.context("Cargo did not report a benchmark executable")
}

/// Prebuilds both profiles, then runs every benchmark in baseline/optimized/
/// optimized/baseline order. A missing adapter or failed phase is an error.
///
/// # Parameters
///
/// * `root` is a new, empty Criterion output directory for this comparison.
///
/// # Returns
///
/// Success only after all four phases of all workloads finish. Their raw samples
/// and executable paths remain available for independent inspection.
pub(super) fn run(root: &Path) -> Result<()> {
  fs::create_dir_all(root)?;
  ensure!(
    fs::read_dir(root)?.next().is_none(),
    "measurement directory must be empty: {}",
    root.display()
  );
  let root = root.canonicalize()?;
  let mut executables = Vec::new();
  for &(package, benchmark) in BENCHMARKS {
    let baseline = build(package, benchmark, "release")?;
    let optimized = build(package, benchmark, "release-size")?;
    ensure!(baseline != optimized, "profiles resolved to the same executable");
    executables.push(Executables {
      package,
      benchmark,
      baseline,
      optimized,
    });
  }
  fs::write(
    root.join("execution.json"),
    serde_json::to_vec_pretty(&serde_json::json!({ "order": ORDER, "benchmarks": executables }))?,
  )?;
  for benchmark in executables {
    for (label, round) in ORDER {
      let executable = if label == "baseline" {
        &benchmark.baseline
      } else {
        &benchmark.optimized
      };
      let label = format!("{label}-{round}");
      println!("Measuring {}: {label}", benchmark.benchmark);
      let status = Command::new(executable)
        .env("CRITERION_HOME", &root)
        .args([
          "--bench",
          "--save-baseline",
          &label,
          "--sample-size",
          "60",
          "--warm-up-time",
          "1",
          "--measurement-time",
          "3",
          "--noplot",
        ])
        .status()
        .with_context(|| format!("start {}", executable.display()))?;
      ensure!(status.success(), "benchmark failed: {} ({label})", benchmark.benchmark);
    }
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn artifacts_select_the_bench_instead_of_the_application() -> Result<()> {
    let messages =
      br#"{"reason":"compiler-artifact","target":{"name":"native_frame","kind":["bin"]},"executable":"application"}
{"reason":"compiler-artifact","target":{"name":"native_frame","kind":["bench"]},"executable":"benchmark"}
"#;
    assert_eq!(executable(messages, "native_frame")?, Some(PathBuf::from("benchmark")));
    Ok(())
  }

  #[test]
  fn missing_executable_is_not_guessed() -> Result<()> {
    assert!(executable(br#"{"reason":"build-finished","success":true}"#, "native_frame")?.is_none());
    Ok(())
  }

  #[test]
  fn measurement_order_is_symmetric() {
    assert_eq!(
      ORDER,
      [("baseline", 0), ("optimized", 0), ("optimized", 1), ("baseline", 1)]
    );
  }
}
