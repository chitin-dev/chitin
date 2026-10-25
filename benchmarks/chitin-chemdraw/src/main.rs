//! Reproducible corpus sweeps with bounded workers and append-only case reports.
//!
//! Run: cargo run --release -p chitin-chemdraw-benchmark -- CONFIG.yaml
//! Input/output paths are resolved relative to CONFIG.yaml. Raw data and generated
//! reports belong under benchmarks/data and benchmarks/results, both ignored by Git.

mod checkpoint;
mod dataset;
mod external;
mod macrocycle_angles;
mod metrics;
mod quality;
mod report;
mod ring_cycles;
mod ring_geometry;
mod scene_quality;
mod worker;

use anyhow::{Context, Result, ensure};
use dataset::{Case, Config, LayoutEngine};
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
      "Usage: chitin-chemdraw-benchmark CONFIG.yaml [--output DIRECTORY] [--resume [--check]]\n\
      Paths are relative to CONFIG.yaml. Existing output requires --resume.\n\
      --resume verifies and continues a run with its frozen renderer. --check validates without writing.\n\
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
    !arguments.is_empty(),
    "usage: chitin-chemdraw-benchmark CONFIG.yaml [--output DIRECTORY] [--resume [--check]]"
  );
  let (mut resume, mut check) = (false, false);
  let mut output_override = None;
  let mut flags = arguments[1..].iter();
  while let Some(flag) = flags.next() {
    if flag == "--resume" {
      ensure!(!resume, "duplicate --resume");
      resume = true;
    } else if flag == "--check" {
      ensure!(!check, "duplicate --check");
      check = true;
    } else if flag == "--output" {
      ensure!(output_override.is_none(), "duplicate --output");
      output_override = Some(flags.next().context("--output requires a directory")?);
    } else {
      anyhow::bail!("unknown argument: {}", flag.to_string_lossy());
    }
  }
  ensure!(!check || resume, "--check requires --resume");
  let config_path = Path::new(&arguments[0]).canonicalize()?;
  let mut config: Config = serde_yaml::from_reader(File::open(&config_path)?)?;
  if let Some(output) = output_override {
    config.output = output.into();
  }
  let base = config_path.parent().context("config needs a parent directory")?;
  config.input = base.join(&config.input).canonicalize()?;
  config.output = base.join(&config.output);
  ensure!(
    ["coarse", "quick", "standard", "best"].contains(&config.coordgen_precision.as_str()),
    "unknown external precision"
  );
  let mut provider = if config.layout_engine == LayoutEngine::RdkitCoordgen {
    ensure!(
      !config.label_feedback,
      "external layout evaluation requires label_feedback: false"
    );
    let python = config
      .external_python
      .as_mut()
      .context("external_python is required for external layouts")?;
    // Keep the virtual-environment executable path: canonicalizing its symlink
    // would accidentally execute the system interpreter without that environment.
    *python = base.join(&python);
    if resume { None } else { Some(external::probe(python)?) }
  } else {
    ensure!(
      config.external_python.is_none(),
      "external_python requires an external layout engine"
    );
    None
  };
  ensure!(
    config.timeout_seconds > 0 && config.max_atoms > 0,
    "resource limits must be positive"
  );
  if !resume {
    fs::create_dir_all(config.output.parent().context("output needs a parent directory")?)?;
    fs::create_dir(&config.output).context("output must be a new directory; choose another path or use --resume")?;
    fs::create_dir(config.output.join("svg"))?;
  }
  let _run_lock = checkpoint::lock(&config.output, resume)?;
  // Keep the exact renderer with the run so lazy web previews cannot substitute
  // a newer algorithm for historical measurements after cargo replaces its target.
  if !resume {
    fs::copy(std::env::current_exe()?, config.output.join("renderer")).context("snapshot the benchmark renderer")?;
  }
  let start = Instant::now();
  let input_sha256 = dataset::checksum(&config.input)?;
  let (source_records, sample) = if config.sample_size > 0 {
    let (total, cases) = dataset::sample(&config)?;
    (Some(total), Some(cases))
  } else {
    (None, None)
  };
  let mut recovery = if resume {
    Some(checkpoint::inspect(&config, &input_sha256, sample.as_deref())?)
  } else {
    None
  };
  // Frozen migration-era executables still require their original transport
  // flag. The current renderer treats it only as a historical compatibility field.
  let historical_fragmented = recovery
    .as_ref()
    .is_some_and(|state| state.manifest["layout_engine"] == "chitin_fragmented");
  if let Some(state) = &recovery {
    eprintln!(
      "Resume{}: {} / {} observations committed; {} remaining",
      if check { " check" } else { "" },
      state.completed(),
      state.total,
      state.total - state.completed()
    );
    if check {
      return Ok(());
    }
    if state.clean && state.completed() == state.total {
      save_summary(&config.output, &state.summary, state.elapsed + start.elapsed())?;
      eprintln!("Run already complete; statistics rebuilt without rerunning cases.");
      return Ok(());
    }
    if config.layout_engine == LayoutEngine::RdkitCoordgen {
      provider = Some(state.manifest["external_provider"].clone());
    }
  }
  let renderer = recovery
    .as_ref()
    .map_or_else(|| config.output.join("renderer"), |state| state.renderer.clone());
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
    "layout_engine": config.layout_engine, "external_provider": provider,
    "external_adapter_sha256": (config.layout_engine == LayoutEngine::RdkitCoordgen).then(external::adapter_checksum),
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
    "pipeline": if config.layout_engine == LayoutEngine::RdkitCoordgen {
      "external parse -> CoordGen coordinates -> source mapping validation -> unit normalization -> depict::depict (no repair) -> svg::export -> identical quality"
    } else if config.label_feedback {
      "parse -> characterize -> pipeline::generate (native layout and guarded label feedback) -> svg::export -> quality"
    } else {
      "parse -> characterize -> layout::generate -> depict::depict -> svg::export -> quality"
    },
    "quality_spec_sha256": format!("{:x}", sha2::Sha256::digest(quality::SPECIFICATION.as_bytes())),
    "label_feedback": config.label_feedback, "independent_stereo_validation": "double-bond relative-side only; tetrahedral not implemented",
    "layout_options": if config.layout_engine != LayoutEngine::RdkitCoordgen {
      Some(format!("{:?}", chitin_chemdraw::layout::LayoutOptions {
        max_atoms: config.max_atoms, ..Default::default()
      }))
    } else { None },
    "external_options": if config.layout_engine == LayoutEngine::RdkitCoordgen {
      Some(serde_json::json!({"precision": config.coordgen_precision, "remove_hydrogen_vertices": false,
        "normalization": "positive global scaling by median bond length", "internal_geometry_repair": false}))
    } else { None },
    "depict_options": format!("{:?}", chitin_chemdraw::depict::DepictOptions::default()),
    "svg_options": format!("{:?}", chitin_chemdraw::svg::SvgOptions::default()),
    "timing_policy": "cold per-molecule processes; external import/parse/layout separated; wall includes provider startup and transfer; QA separate; one shared per-case deadline",
    "quality_policy": {"version": quality::VERSION, "no_weighted_score": true,
      "atom_distance": quality::ATOM_DISTANCE, "atom_edge_distance": quality::ATOM_EDGE_DISTANCE,
      "length_relative_deviation": quality::LENGTH_DEVIATION, "angle_deviation_degrees": quality::ANGLE_DEVIATION_DEG}
  });
  if !resume {
    fs::write(
      config.output.join("manifest.json"),
      serde_json::to_vec_pretty(&manifest)?,
    )?;
  }
  let (mut results, mut selected) = if let Some(state) = &mut recovery {
    state.apply(&config.output)?
  } else {
    (
      File::create_new(config.output.join("cases.jsonl"))?,
      File::create_new(config.output.join("sample.jsonl"))?,
    )
  };
  let resumed = recovery.as_ref().map_or(0, |state| state.completed());
  let previous_elapsed = recovery.as_ref().map_or(Duration::ZERO, |state| state.elapsed);
  let mut summary = recovery
    .as_mut()
    .map_or_else(Summary::default, |state| std::mem::take(&mut state.summary));
  let mut source_index = 0;
  let mut consume = |case: Case| -> Result<()> {
    source_index += 1;
    if source_index <= resumed {
      return Ok(());
    }
    write_json_line(&mut selected, &case)?;
    let artifact =
      (summary.cases < config.save_svg_count).then(|| config.output.join("svg").join(format!("{:08}.svg", case.row)));
    if let (Some(state), Some(path)) = (&mut recovery, &artifact) {
      state.preserve_artifact(&config.output, path)?;
    }
    let record = isolated(
      Request {
        case,
        max_atoms: config.max_atoms,
        svg_path: artifact,
        label_feedback: config.label_feedback,
        fragmented_layout: historical_fragmented,
        imported_positions: None,
        external_layout: None,
        external_timings_ms: Default::default(),
      },
      config.timeout_seconds,
      &config,
      provider.as_ref(),
      &renderer,
    )?;
    write_json_line(&mut results, &record)?;
    summary.add(&record);
    if summary.cases.is_multiple_of(100) {
      eprintln!(
        "{} cases: {} success, {} timeouts",
        summary.cases, summary.success, summary.timeouts
      );
      save_summary(&config.output, &summary, previous_elapsed + start.elapsed())?;
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
  save_summary(&config.output, &summary, previous_elapsed + start.elapsed())?;
  eprintln!("{} cases finished; report: {}", summary.cases, config.output.display());
  Ok(())
}

/// Starts one disposable child and enforces a wall-clock bound, including QA.
///
/// File-backed output avoids pipe deadlocks. Killing a timed-out worker is
/// followed by wait, so no background chemistry computation survives its case.
fn isolated(
  mut request: Request,
  timeout_seconds: u64,
  config: &Config,
  provider: Option<&serde_json::Value>,
  renderer: &Path,
) -> Result<Record> {
  let output = tempfile::tempfile()?;
  let errors = tempfile::tempfile()?;
  let start = Instant::now();
  if config.layout_engine == LayoutEngine::RdkitCoordgen {
    let mut failed = Record::pending(request.case.clone());
    failed.stage = "external_worker".into();
    let result = external::invoke(
      config
        .external_python
        .as_deref()
        .context("missing external interpreter")?,
      Some(
        &serde_json::json!({"case": request.case, "max_atoms": request.max_atoms, "precision": config.coordgen_precision}),
      ),
      Duration::from_secs(timeout_seconds),
    );
    match result.and_then(|response| {
      response
        .map(serde_json::from_value::<external::Response>)
        .transpose()
        .map_err(Into::into)
    }) {
      Ok(Some(response)) => {
        failed.stage = response.stage;
        failed.timings_ms = response.timings_ms;
        if let Some(message) = response.error {
          failed.error = Some(message);
        } else if let Some(layout) = response.layout {
          if Some(&layout.metadata) != provider || layout.precision != config.coordgen_precision {
            failed.error = Some("external provider changed after the run probe".into());
          } else {
            request.external_layout = Some(layout);
            request.external_timings_ms.clone_from(&failed.timings_ms);
          }
        } else {
          failed.error = Some("external provider omitted coordinates".into());
        }
      }
      Ok(None) => {
        failed.status = "timeout".into();
        failed.error = Some(format!("exceeded {timeout_seconds}s hard wall limit"));
      }
      Err(error) => {
        failed.status = "crash".into();
        failed.error = Some(format!("{error:#}"));
      }
    }
    if request.external_layout.is_none() {
      failed.wall_ms = start.elapsed().as_secs_f64() * 1000.;
      return Ok(failed);
    }
  }
  let mut child = Command::new(renderer)
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
  checkpoint::save_json(path, "summary.json", &report)
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
