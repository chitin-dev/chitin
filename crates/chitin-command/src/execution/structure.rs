//! Portable local-structure inspection and validation.

use std::{
  path::{Path, PathBuf},
  sync::Arc,
};

use chitin_bio::structure::{MmcifParser, PdbParser, Structure, StructureParseResult};
use chitin_databases::providers::rcsb::StructureFormat;

use super::{CommandExecutionError, CommandOutcome, StructureInspection, StructureValidation};
use crate::{CommandExecutionContext, StructureCommand, StructureInputArguments};

/// Executes a local structure command against frontend-provided resources.
///
/// # Parameters
///
/// * `command` contains the input, output, and validation preferences.
/// * `context` supplies the working directory and optional standard input.
///
/// # Returns
///
/// Parsed inspection data or a structured validation result.
pub(super) fn execute(
  command: StructureCommand,
  context: &CommandExecutionContext,
) -> Result<CommandOutcome, CommandExecutionError> {
  match command {
    StructureCommand::Inspect(arguments) => {
      let (format, bytes) = read_input(&arguments.input, context)?;
      let parsed = parse_structure(format, &arguments.input.input, &bytes)?;
      Ok(CommandOutcome::StructureInspection(Box::new(StructureInspection {
        path: arguments.input.input,
        format,
        byte_count: bytes.len(),
        parsed,
        output: arguments.output,
        verbose: arguments.verbose,
      })))
    }
    StructureCommand::Validate(arguments) => {
      let (format, bytes) = read_input(&arguments.input, context)?;
      let parsed = parse_structure(format, &arguments.input.input, &bytes)?;
      let error = parsed
        .structure
        .validate_invariants()
        .map_err(|error| error.to_string())
        .and_then(|()| validate_content(&parsed.structure))
        .err();
      Ok(CommandOutcome::StructureValidation(StructureValidation {
        path: arguments.input.input,
        format,
        output: arguments.output,
        error,
      }))
    }
  }
}

/// Reads a local file or frontend-provided standard input.
///
/// # Parameters
///
/// * `input` identifies the source path and optional explicit format.
/// * `context` resolves relative paths and supplies standard-input bytes.
///
/// # Returns
///
/// The resolved structure format and complete source bytes.
fn read_input(
  input: &StructureInputArguments,
  context: &CommandExecutionContext,
) -> Result<(StructureFormat, Arc<[u8]>), CommandExecutionError> {
  let path = &input.input;
  let format = input
    .format
    .or_else(|| detect_format(path))
    .ok_or_else(|| CommandExecutionError::UnknownStructureFormat(path.to_owned()))?;
  if path == Path::new("-") {
    let bytes = context
      .standard_input
      .as_ref()
      .ok_or(CommandExecutionError::MissingStandardInput)?
      .clone();
    return Ok((format, bytes));
  }
  let resolved = resolve_from_working_directory(path, &context.working_directory);
  let bytes = std::fs::read(resolved).map_err(|source| CommandExecutionError::StructureRead {
    path: path.to_owned(),
    source,
  })?;
  Ok((format, bytes.into()))
}

/// Parses bytes with the reader selected by the resolved format.
///
/// # Parameters
///
/// * `format` selects the PDB or mmCIF parser.
/// * `path` identifies the input in diagnostics.
/// * `bytes` contains the complete source document.
///
/// # Returns
///
/// The shared molecular structure and recoverable parser diagnostics.
fn parse_structure(
  format: StructureFormat,
  path: &Path,
  bytes: &[u8],
) -> Result<StructureParseResult, CommandExecutionError> {
  let result = match format {
    StructureFormat::Pdb => PdbParser::new().parse_bytes(bytes).map_err(|source| source.to_string()),
    StructureFormat::Mmcif => MmcifParser::new()
      .parse_bytes(bytes)
      .map_err(|source| source.to_string()),
  };
  result.map_err(|message| CommandExecutionError::StructureParse {
    path: path.to_owned(),
    message,
  })
}

/// Verifies that parsed data contains a coordinate-bearing structure.
fn validate_content(structure: &Structure) -> Result<(), String> {
  if structure.atoms().is_empty() {
    return Err("no atom records were found".to_owned());
  }
  if structure.models().is_empty() {
    return Err("no coordinate models were found".to_owned());
  }
  if !structure
    .coordinates()
    .iter()
    .flat_map(|coordinates| coordinates.positions.iter())
    .any(|position| position.iter().all(|coordinate| coordinate.is_finite()))
  {
    return Err("no finite atom coordinates were found".to_owned());
  }
  Ok(())
}

/// Infers a structure format from a supported filename extension.
fn detect_format(path: &Path) -> Option<StructureFormat> {
  let extension = path.extension()?.to_str()?.to_ascii_lowercase();
  match extension.as_str() {
    "pdb" | "ent" => Some(StructureFormat::Pdb),
    "cif" | "mmcif" => Some(StructureFormat::Mmcif),
    _ => None,
  }
}

/// Resolves a relative input path against the frontend working directory.
fn resolve_from_working_directory(path: &Path, working_directory: &Path) -> PathBuf {
  if path.is_absolute() {
    path.to_owned()
  } else {
    working_directory.join(path)
  }
}

#[cfg(test)]
mod tests {
  use crate::{CommandOutputFormat, StructureInspectArguments};

  use super::*;

  #[test]
  fn inspect_should_parse_frontend_standard_input() -> Result<(), CommandExecutionError> {
    let command = StructureCommand::Inspect(StructureInspectArguments {
      input: StructureInputArguments {
        input: PathBuf::from("-"),
        format: Some(StructureFormat::Pdb),
      },
      output: CommandOutputFormat::Text,
      verbose: false,
    });
    let context = CommandExecutionContext::new(".").with_standard_input(
      b"ATOM      1  CA  GLY A   1       1.000   2.000   3.000  1.00 20.00           C  \nEND\n".to_vec(),
    );

    let outcome = execute(command, &context)?;

    assert!(
      matches!(outcome, CommandOutcome::StructureInspection(inspection) if inspection.parsed.structure.atoms().len() == 1)
    );
    Ok(())
  }
}
