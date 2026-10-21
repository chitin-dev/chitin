//! Reproducible corpus sweeps with bounded workers and append-only case reports.
//!
//! Run: cargo run --release -p chitin-chemdraw-benchmark -- CONFIG.yaml
//! Input/output paths are resolved relative to CONFIG.yaml. Raw data and generated
//! reports belong under benchmarks/data and benchmarks/results, both ignored by Git.

mod dataset;
mod metrics;
mod quality;
mod report;
mod ring_geometry;
mod scene_quality;
mod worker;

use anyhow::{Context, Result, ensure};
use dataset::{Case, Config};
use report::Summary;
use serde::Serialize;
use sha2::Digest;
use std::{
  fs::{self, File},
  io::{BufRead, BufReader, Write},
  path::Path,
  process::{Command, Stdio},
  thread,
  time::{Duration, Instant},
};
use worker::{Event, Record, Request};

/// Dispatches internal workers or one explicitly configured benchmark sweep.
fn main() -> Result<()> {
  let arguments: Vec<_> = std::env::args_os().skip(1).collect();
  if arguments
    .first()
    .is_some_and(|value| value == "--help" || value == "-h")
  {
    println!(
      "Usage: chitin-chemdraw-benchmark CONFIG.yaml [--output DIRECTORY]\n\
      Paths are relative to CONFIG.yaml. Output must not already exist.\n\
      sample_size: 0 streams the entire corpus; a positive value selects a fixed seeded sample.\n\
      Reports: manifest.json, sample.jsonl, cases.jsonl, summary.json and bounded SVG artifacts.\n\
      Quality screens do not certify full chemical, label or tetrahedral correctness."
    );
    return Ok(());
  }
  if arguments.first().is_some_and(|value| value == "--worker") {
    return worker::run();
  }
  ensure!(
    arguments.len() == 1 || arguments.len() == 3 && arguments[1] == "--output",
    "usage: chitin-chemdraw-benchmark CONFIG.yaml [--output DIRECTORY]"
  );
  let config_path = Path::new(&arguments[0]).canonicalize()?;
  let mut config: Config = serde_yaml::from_reader(File::open(&config_path)?)?;
  if let Some(output) = arguments.get(2) {
    config.output = output.into();
  }
  let base = config_path.parent().context("config needs a parent directory")?;
  config.input = base.join(&config.input).canonicalize()?;
  config.output = base.join(&config.output);
  ensure!(
    config.timeout_seconds > 0 && config.max_atoms > 0,
    "resource limits must be positive"
  );
  fs::create_dir_all(config.output.parent().context("output needs a parent directory")?)?;
  fs::create_dir(&config.output).context("output must be a new directory; choose another path for a repeat")?;
  fs::create_dir(config.output.join("svg"))?;
  // Keep the exact renderer with the run so lazy web previews cannot substitute
  // a newer algorithm for historical measurements after cargo replaces its target.
  fs::copy(std::env::current_exe()?, config.output.join("renderer")).context("snapshot the benchmark renderer")?;
  let start = Instant::now();
  let input_sha256 = dataset::checksum(&config.input)?;
  let (source_records, sample) = if config.sample_size > 0 {
    let (total, cases) = dataset::sample(&config)?;
    (Some(total), Some(cases))
  } else {
    (None, None)
  };
  let command_output = |arguments: &[&str]| {
    Command::new(arguments[0])
      .args(&arguments[1..])
      .output()
      .ok()
      .filter(|output| output.status.success())
      .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
  };
  let manifest = serde_json::json!({
    "schema_version": 2, "config": config, "input_sha256": input_sha256,
    "source_records": source_records, "sample_algorithm": "lowest_sha256(seed_le_u64,row_le_u64)",
    "git_revision": command_output(&["git", "rev-parse", "HEAD"]),
    "git_dirty": command_output(&["git", "status", "--porcelain"]).map(|value| !value.is_empty()),
    "rustc": command_output(&["rustc", "--version"]), "architecture": std::env::consts::ARCH,
    "executable_sha256": dataset::checksum(&config.output.join("renderer"))?,
    "renderer_executable": "renderer",
    "cpu_model": fs::read_to_string("/proc/cpuinfo").ok().and_then(|text| text.lines()
      .find_map(|line| line.strip_prefix("model name").and_then(|value| value.split_once(':'))
        .map(|(_, model)| model.trim().to_owned()))),
    "os": std::env::consts::OS, "debug_assertions": cfg!(debug_assertions), "jobs": 1,
    "pipeline": if config.label_feedback {
      "parse -> characterize -> layout::generate -> pipeline::finish -> svg::export -> quality"
    } else {
      "parse -> characterize -> layout::generate -> depict::depict -> svg::export -> quality"
    },
    "quality_spec_sha256": format!("{:x}", sha2::Sha256::digest(quality::SPECIFICATION.as_bytes())),
    "label_feedback": config.label_feedback, "independent_stereo_validation": "double-bond relative-side only; tetrahedral not implemented",
    "layout_options": format!("{:?}", chitin_chemdraw::layout::LayoutOptions {
      max_atoms: config.max_atoms, ..Default::default()
    }),
    "depict_options": format!("{:?}", chitin_chemdraw::depict::DepictOptions::default()),
    "svg_options": format!("{:?}", chitin_chemdraw::svg::SvgOptions::default()),
    "timing_policy": "cold per-molecule process; worker stage times exclude spawn; one observation per case; QA separate",
    "quality_policy": {"version": quality::VERSION, "no_weighted_score": true,
      "atom_distance": quality::ATOM_DISTANCE, "atom_edge_distance": quality::ATOM_EDGE_DISTANCE,
      "length_relative_deviation": quality::LENGTH_DEVIATION, "angle_deviation_degrees": quality::ANGLE_DEVIATION_DEG}
  });
  fs::write(
    config.output.join("manifest.json"),
    serde_json::to_vec_pretty(&manifest)?,
  )?;
  let mut results = File::create_new(config.output.join("cases.jsonl"))?;
  let mut selected = File::create_new(config.output.join("sample.jsonl"))?;
  let mut summary = Summary::default();
  let mut consume = |case: Case| -> Result<()> {
    write_json_line(&mut selected, &case)?;
    let artifact =
      (summary.cases < config.save_svg_count).then(|| config.output.join("svg").join(format!("{:08}.svg", case.row)));
    let record = isolated(
      Request {
        case,
        max_atoms: config.max_atoms,
        svg_path: artifact,
        label_feedback: config.label_feedback,
      },
      config.timeout_seconds,
    )?;
    write_json_line(&mut results, &record)?;
    summary.add(&record);
    if summary.cases.is_multiple_of(100) {
      eprintln!(
        "{} cases: {} success, {} timeouts",
        summary.cases, summary.success, summary.timeouts
      );
      save_summary(&config.output, &summary, start.elapsed())?;
    }
    Ok(())
  };
  if let Some(cases) = sample {
    for case in cases {
      consume(case)?;
    }
  } else {
    dataset::visit(&config, &mut consume)?;
  }
  save_summary(&config.output, &summary, start.elapsed())?;
  eprintln!("{} cases finished; report: {}", summary.cases, config.output.display());
  Ok(())
}

/// Starts one disposable child and enforces a wall-clock bound, including QA.
///
/// File-backed output avoids pipe deadlocks. Killing a timed-out worker is
/// followed by wait, so no background chemistry computation survives its case.
fn isolated(request: Request, timeout_seconds: u64) -> Result<Record> {
  let output = tempfile::tempfile()?;
  let errors = tempfile::tempfile()?;
  let start = Instant::now();
  let mut child = Command::new(std::env::current_exe()?)
    .arg("--worker")
    .stdin(Stdio::piped())
    .stdout(output.try_clone()?)
    .stderr(errors.try_clone()?)
    .spawn()?;
  let input_result = (|| -> Result<()> {
    let mut input = child.stdin.take().context("worker stdin unavailable")?;
    serde_json::to_writer(&mut input, &request)?;
    Ok(())
  })();
  if let Err(error) = input_result {
    let _ = child.kill();
    let _ = child.wait();
    return Err(error);
  }
  let remaining = Duration::from_secs(timeout_seconds).saturating_sub(start.elapsed());
  let (status, timed_out) = wait_with_deadline(&mut child, remaining)?;
  use std::io::{Read, Seek, SeekFrom};
  let mut output = output;
  output.seek(SeekFrom::Start(0))?;
  let mut record = Record::pending(request.case);
  for line in BufReader::new(output).lines() {
    // A killed worker may leave only its last line incomplete; previous markers survive.
    if let Ok(event) = serde_json::from_str::<Event>(&line?) {
      match event {
        Event::Checkpoint { record: checkpoint } | Event::Finished { record: checkpoint } => record = *checkpoint,
      }
    }
  }
  if timed_out {
    record.status = "timeout".into();
    record.error = Some(format!("exceeded {timeout_seconds}s hard wall limit"));
  } else if !status.success() || record.error.is_none() && record.status != "success" {
    let mut errors = errors;
    errors.seek(SeekFrom::Start(0))?;
    let mut message = String::new();
    errors.take(16384).read_to_string(&mut message)?;
    record.status = "crash".into();
    record.error = Some(format!("worker exited {status}: {message}"));
  }
  record.wall_ms = start.elapsed().as_secs_f64() * 1000.;
  Ok(record)
}

/// Reaps a completed worker, or kills and reaps it once its deadline expires.
fn wait_with_deadline(child: &mut std::process::Child, limit: Duration) -> Result<(std::process::ExitStatus, bool)> {
  let start = Instant::now();
  loop {
    if let Some(status) = child.try_wait()? {
      return Ok((status, false));
    }
    if start.elapsed() >= limit {
      // The process can exit between try_wait and kill; wait still reaps it.
      let _ = child.kill();
      return Ok((child.wait()?, true));
    }
    thread::sleep(Duration::from_millis(2));
  }
}

/// Flushes complete records so interrupted long runs retain completed cases.
fn write_json_line(file: &mut File, value: &impl Serialize) -> Result<()> {
  serde_json::to_writer(&mut *file, value)?;
  writeln!(file)?;
  file.flush()?;
  Ok(())
}

/// Writes an explicitly descriptive timing summary, not statistical confidence.
fn save_summary(path: &Path, summary: &Summary, elapsed: Duration) -> Result<()> {
  let report = summary.json(elapsed);
  // Replace only this tool's own checkpoint; case JSONL remains append-only.
  fs::write(path.join("summary.json"), serde_json::to_vec_pretty(&report)?)?;
  Ok(())
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
  use super::*;

  #[test]
  fn deadline_terminates_and_reaps_the_worker() -> Result<()> {
    let mut child = Command::new("/usr/bin/sleep").arg("10").spawn()?;
    let (_, timed_out) = wait_with_deadline(&mut child, Duration::from_millis(10))?;
    assert!(timed_out);
    assert!(child.try_wait()?.is_some());
    Ok(())
  }
}
