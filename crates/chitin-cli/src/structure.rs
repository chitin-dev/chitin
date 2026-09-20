//! Terminal rendering for local structure command outcomes.

use std::path::Path;

use chitin_bio::structure::{Structure, StructureParseResult};
use chitin_command::CommandOutputFormat;
use chitin_command_runtime::{CommandOutcome, StructureInspection, StructureValidation};
use chitin_databases::providers::rcsb::StructureFormat;
use console::Style;
use serde_json::{Value, json};

use crate::error::CliError;

/// Renders a portable command outcome for the CLI frontend.
pub(crate) fn render_outcome(outcome: CommandOutcome) -> Result<(), CliError> {
  match outcome {
    CommandOutcome::DatabaseDownload { .. } => Ok(()),
    CommandOutcome::StructureInspection(inspection) => print_inspection(&inspection),
    CommandOutcome::StructureValidation(validation) => print_validation(&validation),
  }
}

/// Prints a structure summary in text or JSON form.
fn print_inspection(inspection: &StructureInspection) -> Result<(), CliError> {
  match inspection.output {
    CommandOutputFormat::Text => print_text_summary(
      &inspection.path,
      inspection.format,
      inspection.byte_count,
      &inspection.parsed,
      inspection.verbose,
    ),
    CommandOutputFormat::Json => print_json_summary(
      &inspection.path,
      inspection.format,
      inspection.byte_count,
      &inspection.parsed,
      inspection.verbose,
    ),
  }
}

/// Verifies structure invariants and reports the result.
fn print_validation(validation: &StructureValidation) -> Result<(), CliError> {
  match validation.output {
    CommandOutputFormat::Text => {
      if let Some(message) = validation.error.as_ref() {
        let error = Style::new().red().bold();
        eprintln!("{} {}", error.apply_to("✗ Invalid structure:"), message);
        return Err(CliError::StructureValidation {
          path: validation.path.clone(),
          message: message.clone(),
        });
      }
      let success = Style::new().green().bold();
      println!(
        "{} {} ({})",
        success.apply_to("✓ Valid structure:"),
        validation.path.display(),
        validation.format.label()
      );
    }
    CommandOutputFormat::Json => {
      let value = json!({
        "path": validation.path.display().to_string(),
        "format": validation.format.id(),
        "valid": validation.is_valid(),
        "error": validation.error,
      });
      println!("{}", serde_json::to_string_pretty(&value)?);
      if let Some(message) = value.get("error").and_then(Value::as_str) {
        return Err(CliError::StructureValidation {
          path: validation.path.clone(),
          message: message.to_owned(),
        });
      }
    }
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
