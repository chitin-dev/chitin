//! Bounded chemical drawing and atomic SVG persistence, independent of frontends.
//!
//! Frontends provide paths, bytes and cancellation, never rendering state. The
//! worker validates and renders the complete document before creating a sibling
//! temporary file. Only the final publication step may replace a destination.

mod document;

use crate::{ChemdrawCommand, ChemdrawInput, CommandExecutionContext, CommandOutcome};
use chitin_chemdraw::{depict, font::FontFace, pipeline, svg};
use std::{
  io::{Read, Write},
  path::{Path, PathBuf},
};

/// Successful SVG export with shared, frontend-neutral diagnostics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChemdrawExport {
  /// Resolved output path.
  pub path: PathBuf,
  /// Number of molecular occurrences in the drawing.
  pub molecule_count: usize,
  /// Number of persisted SVG bytes.
  pub byte_count: usize,
  /// Recoverable layout or final-scene quality observations.
  pub diagnostics: Vec<String>,
}

/// A drawing failure never leaves a partially written output SVG.
#[derive(Debug, thiserror::Error)]
pub enum ChemdrawExecutionError {
  /// A command, input document, or source reference violates its contract.
  #[error("{0}")]
  Input(String),
  /// A path could not be read or persisted.
  #[error("chemical drawing file '{path}': {source}")]
  File {
    /// Exact affected path.
    path: PathBuf,
    /// Underlying filesystem error.
    source: std::io::Error,
  },
  /// A notation could not be parsed faithfully.
  #[error(transparent)]
  Smiles(#[from] chitin_chem::io::smiles::SmilesError),
  /// The shared automatic molecular pipeline failed.
  #[error(transparent)]
  Render(#[from] pipeline::RenderError),
  /// Authored mechanism geometry or references are invalid.
  #[error(transparent)]
  Diagram(#[from] chitin_chemdraw::diagram::DiagramError),
  /// A selected font is invalid or lacks a glyph.
  #[error(transparent)]
  Font(#[from] chitin_chemdraw::font::FontError),
  /// SVG document framing failed.
  #[error(transparent)]
  Svg(#[from] svg::SvgError),
  /// The versioned YAML/JSON document is invalid.
  #[error("invalid reaction diagram document: {0}")]
  Document(#[from] serde_yaml::Error),
  /// Background execution could not start or its worker failed.
  #[error("chemical drawing worker: {0}")]
  Worker(String),
  /// Cancellation was observed before persistence.
  #[error("chemical drawing cancelled")]
  Cancelled,
}

/// Local shorthand preserving typed failures at the portable executor boundary.
type Error = ChemdrawExecutionError;
/// Maximum source document size; font files use their own larger read allowance.
pub(super) const INPUT_LIMIT: usize = 4 * 1024 * 1024;

/// Resolves a path against frontend context without reading the process cwd.
pub(super) fn resolve(path: &Path, context: &CommandExecutionContext) -> PathBuf {
  if path.is_absolute() {
    path.to_owned()
  } else {
    context.working_directory.join(path)
  }
}

/// Rejects work at cooperative checkpoints when the frontend requested cancellation.
pub(super) fn cancelled(context: &CommandExecutionContext) -> Result<(), Error> {
  if context.cancellation.is_cancelled() {
    Err(Error::Cancelled)
  } else {
    Ok(())
  }
}

/// Retains the affected path beside the original filesystem error.
fn file_error(path: &Path, source: std::io::Error) -> Error {
  Error::File {
    path: path.to_owned(),
    source,
  }
}

/// Reads a file with a strict allocation bound, independent of reported metadata.
///
/// # Parameters
/// - `path`: already resolved source path, retained in error diagnostics.
/// - `limit`: maximum accepted byte count, excluding the overflow probe byte.
///
/// # Returns
/// Complete input bytes when the source fits the allowance.
///
/// # Errors
/// Propagates open/read failures and rejects larger inputs. Reading one extra
/// byte distinguishes an exact-limit file from a truncated oversized document.
fn read_file(path: &Path, limit: usize) -> Result<Vec<u8>, Error> {
  let file = std::fs::File::open(path).map_err(|e| file_error(path, e))?;
  let mut bytes = Vec::new();
  file
    .take(limit as u64 + 1)
    .read_to_end(&mut bytes)
    .map_err(|e| file_error(path, e))?;
  if bytes.len() > limit {
    return Err(Error::Input(format!(
      "input '{}' exceeds {} bytes",
      path.display(),
      limit
    )));
  }
  Ok(bytes)
}

/// Reads a bounded document from a contextual path or frontend-supplied stdin.
///
/// # Parameters
/// - `path`: source path, or the literal `-` sentinel.
/// - `context`: relative-path base and optional immutable stdin buffer.
///
/// # Returns
/// Owned bytes bounded by [`INPUT_LIMIT`]. No interactive terminal read occurs.
///
/// # Errors
/// Rejects missing stdin, oversized input, or a file read failure.
fn input(path: &Path, context: &CommandExecutionContext) -> Result<Vec<u8>, Error> {
  if path.as_os_str() == "-" {
    let bytes = context
      .standard_input
      .as_ref()
      .ok_or_else(|| Error::Input("input '-' requires standard-input bytes from the frontend".into()))?;
    if bytes.len() > INPUT_LIMIT {
      return Err(Error::Input("drawing input exceeds 4 MiB".into()));
    }
    return Ok(bytes.to_vec());
  }
  read_file(&resolve(path, context), INPUT_LIMIT)
}

/// Applies command-level appearance overrides without changing chemical facts.
///
/// # Parameters
/// - `command`: optional bond/stroke sizes, display policies and font path.
/// - `context`: working directory used for a relative font path.
///
/// # Returns
/// Default molecular proportions scaled to the selected bond length, followed
/// by explicit absolute overrides. Fonts are loaded as immutable outline data.
///
/// # Errors
/// Rejects unreadable, oversized or invalid font files. Derived depiction sizes
/// and glyph availability are validated later by the drawing engine.
fn options(command: &ChemdrawCommand, context: &CommandExecutionContext) -> Result<depict::DepictOptions, Error> {
  let mut options = depict::DepictOptions::default();
  if command.style.circled_charges {
    options.charge_style = depict::ChargeStyle::Circled;
  }
  if let Some(length) = command.style.bond_length {
    // Preserve default proportions before applying an explicit stroke width;
    // command overrides are absolute SVG units rather than scale multipliers.
    let ratio = length.get() / options.bond_length;
    options.bond_length = length.get();
    options.font_size *= ratio;
    options.stroke_width *= ratio;
    options.wedge_width *= ratio;
    options.bond_spacing *= ratio;
    options.label_padding *= ratio;
  }
  if let Some(width) = command.style.stroke_width {
    options.stroke_width = width.get();
  }
  if command.style.monochrome {
    options.color_scheme = depict::ColorScheme::Monochrome;
    options.bond_color_style = depict::BondColorStyle::Uniform;
  }
  if command.style.aromatic_circles {
    options.aromatic_style = depict::AromaticStyle::Circle;
  }
  if command.style.explicit_chiral_hydrogens {
    options.chiral_hydrogens = depict::ChiralHydrogenMode::Explicit;
  }
  if let Some(path) = &command.style.font_file {
    options.font = FontFace::from_bytes(read_file(&resolve(path, context), 16 * 1024 * 1024)?, 0)?;
  }
  Ok(options)
}

/// Runs drawing and file I/O on the active runtime's blocking worker pool.
///
/// # Parameters
/// - `command`: owned request, moved into the background worker.
/// - `context`: path, input and cancellation resources shared with its frontend.
///
/// # Returns
/// The persisted SVG outcome; no scientific work runs on the awaiting UI task.
///
/// # Errors
/// Rejects a missing runtime, worker failure, or any rendering/persistence error.
/// Dropping the awaiting future signals cancellation to the detached worker;
/// work already in progress stops cooperatively at a subsequent checkpoint.
pub(super) async fn execute(
  command: ChemdrawCommand,
  context: CommandExecutionContext,
) -> Result<CommandOutcome, Error> {
  let runtime = tokio::runtime::Handle::try_current().map_err(|e| Error::Worker(e.to_string()))?;
  let mut guard = WorkerCancellationGuard(Some(context.cancellation.clone()));
  let result = runtime
    .spawn_blocking(move || run(command, &context))
    .await
    .map_err(|e| Error::Worker(e.to_string()));
  // An ordinary result must not mark the reusable frontend context cancelled.
  guard.0 = None;
  result?
}

/// Blocking work cannot be aborted by dropping its join future. Signal its
/// pre-persistence cancellation check if the awaiting frontend disappears.
struct WorkerCancellationGuard(Option<chitin_databases::CancellationToken>);

impl Drop for WorkerCancellationGuard {
  /// Requests cancellation only while an awaiting invocation is still armed.
  fn drop(&mut self) {
    if let Some(token) = &self.0 {
      token.cancel();
    }
  }
}

/// Validates, renders and atomically publishes one complete SVG export.
///
/// # Parameters
/// - `command`: source, protected output path, overwrite permission and styling.
/// - `context`: working directory, bounded stdin and cooperative cancellation.
///
/// # Returns
/// Resolved output path, occurrence/byte counts and recoverable quality warnings.
/// The parent directory must exist; input and font files cannot be overwritten.
///
/// # Errors
/// Rejects invalid paths, protected destinations, unsupported input/chemistry,
/// cancellation, and read/render/write failures. Temporary files clean themselves
/// up on failure. Publication uses a final no-clobber operation unless forced;
/// cancellation observed before that operation prevents publication.
fn run(command: ChemdrawCommand, context: &CommandExecutionContext) -> Result<CommandOutcome, Error> {
  cancelled(context)?;
  let output = resolve(&command.output, context);
  if !output.extension().is_some_and(|s| s.eq_ignore_ascii_case("svg")) {
    return Err(Error::Input("output must name an .svg file".into()));
  }
  if !command.force && output.symlink_metadata().is_ok() {
    return Err(Error::Input(format!(
      "output '{}' already exists; pass --force to replace it",
      output.display()
    )));
  }
  let source_path = match &command.input {
    ChemdrawInput::Diagram(path) if path.as_os_str() != "-" => Some(path),
    _ => None,
  };
  for path in source_path.into_iter().chain(command.style.font_file.as_ref()) {
    // Force authorizes replacement of an output, not destruction of its inputs.
    // Canonical paths also catch aliases introduced by directory symlinks.
    if matches!((resolve(path, context).canonicalize(), output.canonicalize()), (Ok(source), Ok(destination)) if source == destination)
    {
      return Err(Error::Input(
        "output must not replace the diagram input or font file".into(),
      ));
    }
  }
  let options = options(&command, context)?;
  let (drawing, molecule_count, diagnostics) = match &command.input {
    ChemdrawInput::Smiles(value) => {
      let source = if value == "-" {
        String::from_utf8(input(Path::new("-"), context)?)
          .map_err(|_| Error::Input("SMILES input must be UTF-8".into()))?
      } else {
        value.clone()
      };
      if source.len() > 64 * 1024 {
        return Err(Error::Input("SMILES exceeds 64 KiB".into()));
      }
      let graph = chitin_chem::io::smiles::read_smiles(source.trim())?.into_graph();
      let rendered = pipeline::generate(&graph, &Default::default(), &options)?;
      let mut diagnostics: Vec<_> = rendered.layout.diagnostics().iter().map(|d| format!("{d:?}")).collect();
      diagnostics.extend(rendered.contacts.iter().map(|d| format!("{d:?}")));
      (rendered.drawing, 1, diagnostics)
    }
    ChemdrawInput::Diagram(path) => document::render(&input(path, context)?, &options, context)?,
  };
  let svg = svg::export(&drawing, &svg::SvgOptions::default())?;
  cancelled(context)?;
  let parent = output
    .parent()
    .ok_or_else(|| Error::Input("output has no parent directory".into()))?;
  // A sibling temporary file keeps final publication on the same filesystem.
  let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|e| file_error(parent, e))?;
  temporary
    .write_all(svg.as_bytes())
    .map_err(|e| file_error(&output, e))?;
  temporary.as_file().sync_all().map_err(|e| file_error(&output, e))?;
  cancelled(context)?;
  // The final no-clobber operation closes the race after the initial existence check.
  let persisted = if command.force {
    temporary.persist(&output)
  } else {
    temporary.persist_noclobber(&output)
  };
  persisted.map_err(|e| file_error(&output, e.error))?;
  Ok(CommandOutcome::ChemdrawExport(ChemdrawExport {
    path: output,
    molecule_count,
    byte_count: svg.len(),
    diagnostics,
  }))
}

#[cfg(test)]
mod tests {
  use super::WorkerCancellationGuard;
  use chitin_databases::CancellationToken;

  #[test]
  fn dropping_an_active_worker_guard_should_cancel_pending_persistence() {
    let token = CancellationToken::new();
    drop(WorkerCancellationGuard(Some(token.clone())));
    assert!(token.is_cancelled());
  }

  #[test]
  fn completing_a_worker_should_not_cancel_its_frontend_context() {
    let token = CancellationToken::new();
    let mut guard = WorkerCancellationGuard(Some(token.clone()));
    guard.0 = None;
    drop(guard);
    assert!(!token.is_cancelled());
  }
}
