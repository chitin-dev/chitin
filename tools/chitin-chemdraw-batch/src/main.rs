//! File-based batch orchestration; parsing and drawing remain frontend-independent.

use std::{env, ffi::OsString, fs, io::Write, path::Path, process::ExitCode};

use anyhow::{Context, Result, bail, ensure};

use chitin_chem::io::smiles::read_smiles;
use chitin_chemdraw::{depict, layout, svg};

mod config;

/// Explicit replacement is required so rerunning a batch cannot erase old output.
const USAGE: &str = "Usage: chitin-chemdraw-batch <config.yaml> [--overwrite]";

/// Reports contextual error chains without a Rust debug wrapper.
fn main() -> ExitCode {
  match run(env::args_os().skip(1).collect()) {
    Ok(()) => ExitCode::SUCCESS,
    Err(error) => {
      eprintln!("error: {error:#}");
      ExitCode::FAILURE
    }
  }
}

/// Loads a batch and attempts every entry, preserving input indices in filenames.
///
/// # Parameters
/// - `arguments`: configuration path followed by optional `--overwrite`.
///
/// # Errors
/// Configuration/directory failures stop before rendering. Entry failures are
/// printed with their index and source, then summarized in a nonzero exit status.
/// Layout diagnostics are warnings and do not suppress a complete SVG.
fn run(arguments: Vec<OsString>) -> Result<()> {
  if matches!(arguments.as_slice(), [help] if help == "--help" || help == "-h") {
    println!("{USAGE}");
    return Ok(());
  }
  let (path, overwrite) = match arguments.as_slice() {
    [path] if !path.to_string_lossy().starts_with('-') => (Path::new(path), false),
    [path, flag] if !path.to_string_lossy().starts_with('-') && flag == "--overwrite" => (Path::new(path), true),
    _ => bail!("{USAGE}"),
  };
  let input = fs::read_to_string(path).with_context(|| format!("read configuration {}", path.display()))?;
  let config: config::Config =
    serde_yaml::from_str(&input).with_context(|| format!("parse configuration {}", path.display()))?;
  let destination = config.destination(path)?;
  let depiction = config.depict.options(path)?;
  fs::create_dir_all(&destination).with_context(|| format!("create output directory {}", destination.display()))?;
  let framing = config.svg.options();
  let mut failures = 0;
  for (index, source) in config.smiles.iter().enumerate() {
    let number = index + 1;
    let output = destination.join(format!("{number:04}.svg"));
    let result = render(source, number, &depiction, &framing).and_then(|svg| publish(&output, &svg, overwrite));
    match result {
      Ok(()) => println!("[{number}] {}", output.display()),
      Err(error) => {
        failures += 1;
        eprintln!("[{number}] {source:?}: {error:#}");
      }
    }
  }
  println!("{} generated, {failures} failed", config.smiles.len() - failures);
  ensure!(failures == 0, "batch finished with {failures} failed entries");
  Ok(())
}

/// Renders one source with the existing pipeline and reports nonfatal diagnostics.
///
/// # Errors
/// Propagates source, layout, depiction, and SVG errors without creating a file.
fn render(source: &str, number: usize, depiction: &depict::DepictOptions, framing: &svg::SvgOptions) -> Result<String> {
  ensure!(!source.trim().is_empty(), "empty SMILES");
  let graph = read_smiles(source).context("parse SMILES")?.into_graph();
  let coordinates = layout::generate(&graph, &layout::LayoutOptions::default()).context("generate layout")?;
  if !coordinates.diagnostics().is_empty() {
    eprintln!("[{number}] layout diagnostics: {:?}", coordinates.diagnostics());
  }
  let drawing = depict::depict(&graph, &coordinates, depiction).context("depict molecule")?;
  svg::export(&drawing, framing).context("export SVG")
}

/// Publishes a completed SVG from a temporary file in the destination directory.
///
/// # Parameters
/// - `path`: numbered output inside the batch directory.
/// - `svg`: fully serialized document, never a partially rendered scene.
/// - `overwrite`: whether replacing an existing destination is authorized.
///
/// # Errors
/// Returns write/rename errors, including an existing destination in default mode.
/// Atomic publication avoids truncated output; old files survive failed rendering.
fn publish(path: &Path, svg: &str, overwrite: bool) -> Result<()> {
  let parent = path.parent().context("output has no parent directory")?;
  let mut temporary = tempfile::NamedTempFile::new_in(parent).context("create temporary SVG")?;
  temporary.write_all(svg.as_bytes()).context("write temporary SVG")?;
  if overwrite {
    temporary.persist(path)
  } else {
    temporary.persist_noclobber(path)
  }
  .with_context(|| format!("publish {} (replacement requires --overwrite)", path.display()))?;
  Ok(())
}
