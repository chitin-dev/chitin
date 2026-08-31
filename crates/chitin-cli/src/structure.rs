//! Local PDB and mmCIF inspection workflows.

use std::{
  fs::File,
  io::{self, Read},
  path::Path,
};

use chitin_bio::structure::{MmcifParser, PdbParser, Structure, StructureParseResult};
use chitin_command::{ChitinCommand, StructureCommand};
use chitin_databases::providers::rcsb::StructureFormat;
use console::Style;
use serde_json::{Value, json};

use crate::{
  cli::{FormatArg, OutputArg, StructureInputArgs},
  error::CliError,
};

/// Executes a structure inspection or validation command.
pub(crate) async fn dispatch(
  command: ChitinCommand,
  input: StructureInputArgs,
  output: Option<OutputArg>,
  verbose: bool,
) -> Result<(), CliError> {
  let (format, bytes) = read_input(&input.input, input.format)?;
  let parsed = parse_structure(format, &input.input, &bytes)?;
  match command {
    ChitinCommand::Structure(StructureCommand::Inspect) => print_inspection(
      &input.input,
      format,
      bytes.len(),
      &parsed,
      output.unwrap_or(OutputArg::Text),
      verbose,
    ),
    ChitinCommand::Structure(StructureCommand::Validate) => {
      validate_structure(&input.input, format, &parsed, output.unwrap_or(OutputArg::Text))
    }
    other => Err(CliError::UnsupportedCommand(other.id())),
  }
}

/// Reads a local file or stdin and resolves its structure format.
fn read_input(path: &Path, requested: Option<FormatArg>) -> Result<(StructureFormat, Vec<u8>), CliError> {
  let format = requested
    .map(FormatArg::structure_format)
    .or_else(|| detect_format(path))
    .ok_or_else(|| CliError::StructureFormat(path.to_owned()))?;
  let mut bytes = Vec::new();
  if path == Path::new("-") {
    io::stdin()
      .read_to_end(&mut bytes)
      .map_err(|source| CliError::StructureRead {
        path: path.to_owned(),
        source,
      })?;
  } else {
    File::open(path)
      .and_then(|mut file| file.read_to_end(&mut bytes))
      .map_err(|source| CliError::StructureRead {
        path: path.to_owned(),
        source,
      })?;
  }
  Ok((format, bytes))
}

/// Parses bytes with the reader selected by the resolved format.
fn parse_structure(format: StructureFormat, path: &Path, bytes: &[u8]) -> Result<StructureParseResult, CliError> {
  let result = match format {
    StructureFormat::Pdb => PdbParser::new().parse_bytes(bytes).map_err(|source| source.to_string()),
    StructureFormat::Mmcif => MmcifParser::new()
      .parse_bytes(bytes)
      .map_err(|source| source.to_string()),
  };
  result.map_err(|message| CliError::StructureParse {
    path: path.to_owned(),
    message,
  })
}

/// Prints a structure summary in text or JSON form.
fn print_inspection(
  path: &Path,
  format: StructureFormat,
  byte_count: usize,
  parsed: &StructureParseResult,
  output: OutputArg,
  verbose: bool,
) -> Result<(), CliError> {
  match output {
    OutputArg::Text => print_text_summary(path, format, byte_count, parsed, verbose),
    OutputArg::Json => print_json_summary(path, format, byte_count, parsed, verbose),
  }
}

/// Verifies structure invariants and reports the result.
fn validate_structure(
  path: &Path,
  format: StructureFormat,
  parsed: &StructureParseResult,
  output: OutputArg,
) -> Result<(), CliError> {
  let validation = parsed
    .structure
    .validate_invariants()
    .map_err(|error| error.to_string())
    .and_then(|()| validate_content(&parsed.structure));
  match output {
    OutputArg::Text => {
      if let Err(message) = validation {
        let error = Style::new().red().bold();
        eprintln!("{} {}", error.apply_to("✗ Invalid structure:"), message);
        return Err(CliError::StructureValidation {
          path: path.to_owned(),
          message,
        });
      }
      let success = Style::new().green().bold();
      println!(
        "{} {} ({})",
        success.apply_to("✓ Valid structure:"),
        path.display(),
        format.label()
      );
    }
    OutputArg::Json => {
      let valid = validation.is_ok();
      let value = json!({
        "path": path.display().to_string(),
        "format": format.id(),
        "valid": valid,
        "error": validation.err(),
      });
      println!("{}", serde_json::to_string_pretty(&value)?);
      if let Some(message) = value.get("error").and_then(Value::as_str) {
        return Err(CliError::StructureValidation {
          path: path.to_owned(),
          message: message.to_owned(),
        });
      }
    }
  }
  Ok(())
}

/// Verifies that the parsed value contains an actual coordinate-bearing
/// structure rather than only metadata or ignored records.
///
/// # Parameters
///
/// * `structure` is the parsed, index-validated structure snapshot.
///
/// # Returns
///
/// `Ok(())` when at least one atom and one finite coordinate are present;
/// otherwise returns the reason the input cannot be considered a valid
/// molecular structure.
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

/// Prints the human-readable inspection summary with terminal color accents.
fn print_text_summary(
  path: &Path,
  format: StructureFormat,
  byte_count: usize,
  parsed: &StructureParseResult,
  verbose: bool,
) -> Result<(), CliError> {
  let heading = Style::new().cyan().bold();
  let label = Style::new().dim();
  let value = Style::new().green().bold();
  let warning = Style::new().yellow();
  println!("{} {}", heading.apply_to("Structure"), path.display());
  println!("{} {}", label.apply_to("Format:"), value.apply_to(format.label()));
  println!("{} {}", label.apply_to("Bytes:"), byte_count);
  println!();
  for (name, count) in summary_counts(&parsed.structure) {
    println!("{} {}", label.apply_to(format!("{name}:")), value.apply_to(count));
  }
  if !parsed.diagnostics.is_empty() {
    println!("{} {}", warning.apply_to("Diagnostics:"), parsed.diagnostics.len());
  }
  if verbose {
    println!();
    println!("{}", heading.apply_to("Details"));
    println!("{} {:#?}", label.apply_to("Metadata:"), parsed.structure.metadata());
    println!(
      "{} {:?}",
      label.apply_to("Chains:"),
      parsed
        .structure
        .chains()
        .iter()
        .map(|chain| chain.auth_id.as_deref().or(chain.label_id.as_deref()))
        .collect::<Vec<_>>()
    );
    if !parsed.diagnostics.is_empty() {
      println!("{} {:#?}", warning.apply_to("Diagnostics:"), parsed.diagnostics);
    }
  }
  Ok(())
}

/// Prints the stable machine-readable inspection summary.
fn print_json_summary(
  path: &Path,
  format: StructureFormat,
  byte_count: usize,
  parsed: &StructureParseResult,
  verbose: bool,
) -> Result<(), CliError> {
  let mut value = summary_value(path, format, byte_count, &parsed.structure, parsed.diagnostics.len());
  if verbose {
    value["metadata"] = metadata_value(&parsed.structure);
    value["diagnostics_detail"] = json!(
      parsed
        .diagnostics
        .iter()
        .map(|diagnostic| json!({
          "code": diagnostic.code,
          "line": diagnostic.line,
          "severity": format!("{:?}", diagnostic.severity),
          "message": diagnostic.message,
        }))
        .collect::<Vec<_>>()
    );
  }
  println!("{}", serde_json::to_string_pretty(&value)?);
  Ok(())
}

/// Builds a stable JSON representation of the currently parsed metadata.
fn metadata_value(structure: &Structure) -> Value {
  json!({
    "classification": structure.metadata().classification,
    "identifier": structure.metadata().identifier,
    "unit_cell": structure.metadata().unit_cell.as_ref().map(|cell| json!({
      "lengths": cell.lengths,
      "angles": cell.angles,
    })),
    "symmetry": structure.metadata().symmetry.as_ref().map(|symmetry| json!({
      "space_group_name": symmetry.space_group_name,
      "international_tables_number": symmetry.international_tables_number,
    })),
    "assembly": {
      "operations": structure.metadata().assembly.operations.iter().map(|operation| json!({
        "id": operation.id,
        "rotation": operation.rotation,
        "translation": operation.translation,
      })).collect::<Vec<_>>(),
      "assemblies": structure.metadata().assembly.assemblies.iter().map(|assembly| json!({
        "id": assembly.id,
        "details": assembly.details,
        "generations": assembly.generations.iter().map(|generation| json!({
          "asym_ids": generation.asym_ids,
          "auth_asym_ids": generation.auth_asym_ids,
          "entity_instance_ids": generation.entity_instance_ids,
          "operator_expression": generation.operator_expression,
        })).collect::<Vec<_>>(),
      })).collect::<Vec<_>>(),
    },
  })
}

/// Returns the stable count fields shared by text and JSON summaries.
fn summary_counts(structure: &Structure) -> Vec<(&'static str, usize)> {
  vec![
    ("Models", structure.models().len()),
    ("Chains", structure.chains().len()),
    ("Residues", structure.residues().len()),
    ("Atoms", structure.atoms().len()),
    ("Bonds", structure.bonds().len()),
    ("Polymer entities", structure.polymer_entities().len()),
    ("Missing polymer residues", structure.missing_polymer_residues().len()),
    ("Secondary ranges", structure.secondary_ranges().len()),
    ("Assembly operations", structure.metadata().assembly.operations.len()),
    ("Biological assemblies", structure.metadata().assembly.assemblies.len()),
  ]
}

/// Builds the JSON object used by `inspect --output json`.
fn summary_value(
  path: &Path,
  format: StructureFormat,
  byte_count: usize,
  structure: &Structure,
  diagnostics: usize,
) -> Value {
  let counts = summary_counts(structure);
  let mut value = json!({
    "path": path.display().to_string(),
    "format": format.id(),
    "bytes": byte_count,
    "diagnostics": diagnostics,
  });
  for (name, count) in counts {
    value[name.to_ascii_lowercase().replace(' ', "_")] = json!(count);
  }
  value
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
