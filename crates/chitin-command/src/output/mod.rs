//! Frontend-neutral presentation of completed command outcomes.
//!
//! This module owns wording, field order, line breaks, output format, and
//! success semantics. Process terminals and GPUI terminals only translate the
//! semantic tones into their native styling systems.

use chitin_bio::structure::Structure;
use serde_json::{Value, json};

use crate::{CommandOutcome, CommandOutputFormat, StructureInspection, StructureValidation};

/// Completion state attached to a rendered command report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandReportStatus {
  /// The command completed successfully.
  Succeeded,
  /// The command completed with a domain-level failure already described by the report.
  Failed,
}

/// Serialization form selected by the command arguments.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandOutputKind {
  /// Human-readable terminal output.
  Text,
  /// Stable machine-readable JSON output.
  Json,
}

/// Semantic visual role for one output span.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandOutputTone {
  /// Normal result content.
  Primary,
  /// Labels and supporting text.
  Secondary,
  /// Section or document headings.
  Heading,
  /// Successful result content.
  Success,
  /// Recoverable diagnostic content.
  Warning,
  /// Failed result content.
  Error,
}

/// Styled text fragment independent of ANSI and GPUI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandOutputSpan {
  /// Human-readable content.
  pub text: String,
  /// Semantic role interpreted by the frontend adapter.
  pub tone: CommandOutputTone,
}

impl CommandOutputSpan {
  /// Creates a semantic output span.
  pub fn new(text: impl Into<String>, tone: CommandOutputTone) -> Self {
    Self {
      text: text.into(),
      tone,
    }
  }
}

/// One physical row in a command report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandOutputLine {
  /// Ordered fragments forming the row.
  pub spans: Vec<CommandOutputSpan>,
}

impl CommandOutputLine {
  /// Creates a row from ordered semantic spans.
  pub fn new(spans: impl IntoIterator<Item = CommandOutputSpan>) -> Self {
    Self {
      spans: spans.into_iter().collect(),
    }
  }

  /// Creates an unstyled output row.
  pub fn plain(text: impl Into<String>) -> Self {
    Self::new([CommandOutputSpan::new(text, CommandOutputTone::Primary)])
  }

  /// Returns the row without frontend styling.
  pub fn plain_text(&self) -> String {
    self.spans.iter().map(|span| span.text.as_str()).collect()
  }
}

/// Complete portable-command output shared by textual frontends.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandReport {
  /// Domain-level completion state.
  pub status: CommandReportStatus,
  /// Text or JSON representation selected by the command.
  pub kind: CommandOutputKind,
  /// Ordered physical rows without trailing newline characters.
  pub lines: Vec<CommandOutputLine>,
}

impl CommandReport {
  /// Returns the complete report without frontend styling.
  pub fn plain_text(&self) -> String {
    self
      .lines
      .iter()
      .map(CommandOutputLine::plain_text)
      .collect::<Vec<_>>()
      .join("\n")
  }
}

/// Converts a completed command outcome into its canonical report.
///
/// # Parameters
///
/// * `outcome` contains frontend-independent command results and presentation preferences.
///
/// # Returns
///
/// A semantic report whose wording and line structure are identical for every frontend.
pub fn render_outcome(outcome: &CommandOutcome) -> CommandReport {
  match outcome {
    CommandOutcome::DatabaseDownload { artifact_count } => CommandReport {
      status: CommandReportStatus::Succeeded,
      kind: CommandOutputKind::Text,
      lines: vec![semantic_line(
        CommandOutputTone::Success,
        format!("Downloaded {artifact_count} structure artifact(s)."),
      )],
    },
    CommandOutcome::StructureInspection(inspection) => render_inspection(inspection),
    CommandOutcome::StructureValidation(validation) => render_validation(validation),
  }
}

/// Renders a structure inspection according to its requested output format.
///
/// # Parameters
///
/// * `inspection` contains parsed structure data and text/JSON preferences.
///
/// # Returns
///
/// The canonical text or JSON report for the inspection.
fn render_inspection(inspection: &StructureInspection) -> CommandReport {
  match inspection.output {
    CommandOutputFormat::Text => render_text_inspection(inspection),
    CommandOutputFormat::Json => render_json_inspection(inspection),
  }
}

/// Renders the complete human-readable structure summary.
///
/// # Parameters
///
/// * `inspection` contains the parsed structure and verbosity preference.
///
/// # Returns
///
/// A successful report with stable labels, ordering, and optional details.
fn render_text_inspection(inspection: &StructureInspection) -> CommandReport {
  let structure = &inspection.parsed.structure;
  let mut lines = vec![
    paired_line(
      "Structure ",
      inspection.path.display().to_string(),
      CommandOutputTone::Heading,
    ),
    paired_line("Format: ", inspection.format.label(), CommandOutputTone::Success),
    paired_line("Bytes: ", inspection.byte_count.to_string(), CommandOutputTone::Primary),
    CommandOutputLine::plain(""),
  ];
  lines.extend(
    summary_counts(structure)
      .into_iter()
      .map(|(label, count)| paired_line(format!("{label}: "), count.to_string(), CommandOutputTone::Success)),
  );
  if !inspection.parsed.diagnostics.is_empty() {
    lines.push(paired_line(
      "Diagnostics: ",
      inspection.parsed.diagnostics.len().to_string(),
      CommandOutputTone::Warning,
    ));
  }
  if inspection.verbose {
    lines.push(CommandOutputLine::plain(""));
    lines.push(semantic_line(CommandOutputTone::Heading, "Details"));
    extend_multiline(
      &mut lines,
      &format!("Metadata: {:#?}", structure.metadata()),
      CommandOutputTone::Secondary,
    );
    lines.push(paired_line(
      "Chains: ",
      format!(
        "{:?}",
        structure
          .chains()
          .iter()
          .map(|chain| chain.auth_id.as_deref().or(chain.label_id.as_deref()))
          .collect::<Vec<_>>()
      ),
      CommandOutputTone::Primary,
    ));
    if !inspection.parsed.diagnostics.is_empty() {
      extend_multiline(
        &mut lines,
        &format!("Diagnostics: {:#?}", inspection.parsed.diagnostics),
        CommandOutputTone::Warning,
      );
    }
  }
  CommandReport {
    status: CommandReportStatus::Succeeded,
    kind: CommandOutputKind::Text,
    lines,
  }
}

/// Renders the stable JSON structure summary.
///
/// # Parameters
///
/// * `inspection` contains the parsed structure and verbosity preference.
///
/// # Returns
///
/// A successful report whose rows contain canonical pretty-printed JSON.
fn render_json_inspection(inspection: &StructureInspection) -> CommandReport {
  let mut value = summary_value(inspection);
  if inspection.verbose {
    value["metadata"] = metadata_value(&inspection.parsed.structure);
    value["diagnostics_detail"] = json!(
      inspection
        .parsed
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
  json_report(CommandReportStatus::Succeeded, value)
}

/// Renders a structure validation result and preserves its failure status.
///
/// # Parameters
///
/// * `validation` contains the requested format and optional invariant failure.
///
/// # Returns
///
/// A report whose status matches the validation result.
fn render_validation(validation: &StructureValidation) -> CommandReport {
  match validation.output {
    CommandOutputFormat::Text => {
      let (status, tone, text) = match validation.error.as_deref() {
        Some(error) => (
          CommandReportStatus::Failed,
          CommandOutputTone::Error,
          format!("✗ Invalid structure: {error}"),
        ),
        None => (
          CommandReportStatus::Succeeded,
          CommandOutputTone::Success,
          format!(
            "✓ Valid structure: {} ({})",
            validation.path.display(),
            validation.format.label()
          ),
        ),
      };
      CommandReport {
        status,
        kind: CommandOutputKind::Text,
        lines: vec![semantic_line(tone, text)],
      }
    }
    CommandOutputFormat::Json => json_report(
      if validation.is_valid() {
        CommandReportStatus::Succeeded
      } else {
        CommandReportStatus::Failed
      },
      json!({
        "path": validation.path.display().to_string(),
        "format": validation.format.id(),
        "valid": validation.is_valid(),
        "error": validation.error,
      }),
    ),
  }
}

/// Builds one label-value row with independent semantic tones.
fn paired_line(label: impl Into<String>, value: impl Into<String>, value_tone: CommandOutputTone) -> CommandOutputLine {
  CommandOutputLine::new([
    CommandOutputSpan::new(label, CommandOutputTone::Secondary),
    CommandOutputSpan::new(value, value_tone),
  ])
}

/// Builds one row with a uniform semantic tone.
fn semantic_line(tone: CommandOutputTone, text: impl Into<String>) -> CommandOutputLine {
  CommandOutputLine::new([CommandOutputSpan::new(text, tone)])
}

/// Appends every physical row from a formatted multi-line value.
fn extend_multiline(lines: &mut Vec<CommandOutputLine>, text: &str, tone: CommandOutputTone) {
  lines.extend(text.lines().map(|line| semantic_line(tone, line)));
}

/// Returns stable count labels shared by text and JSON reports.
fn summary_counts(structure: &Structure) -> [(&'static str, usize); 10] {
  [
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

/// Builds the stable JSON inspection summary.
///
/// # Parameters
///
/// * `inspection` contains source metadata, diagnostics, and parsed structure data.
///
/// # Returns
///
/// A JSON object containing stable scalar metadata and structure counts.
fn summary_value(inspection: &StructureInspection) -> Value {
  let mut value = json!({
    "path": inspection.path.display().to_string(),
    "format": inspection.format.id(),
    "bytes": inspection.byte_count,
    "diagnostics": inspection.parsed.diagnostics.len(),
  });
  for (name, count) in summary_counts(&inspection.parsed.structure) {
    value[name.to_ascii_lowercase().replace(' ', "_")] = json!(count);
  }
  value
}

/// Builds stable JSON metadata for verbose inspection output.
///
/// # Parameters
///
/// * `structure` supplies crystallographic and biological assembly metadata.
///
/// # Returns
///
/// A JSON object suitable for the verbose inspection report.
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

/// Converts a JSON value into canonical pretty-printed physical rows.
///
/// # Parameters
///
/// * `status` is the domain-level completion state.
/// * `value` is the complete machine-readable result.
///
/// # Returns
///
/// A JSON report split into physical rows for consistent frontend rendering.
fn json_report(status: CommandReportStatus, value: Value) -> CommandReport {
  let text = format!("{value:#}");
  CommandReport {
    status,
    kind: CommandOutputKind::Json,
    lines: text.lines().map(CommandOutputLine::plain).collect(),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn report_plain_text_should_preserve_semantic_span_order() {
    let report = CommandReport {
      status: CommandReportStatus::Succeeded,
      kind: CommandOutputKind::Text,
      lines: vec![paired_line("Format: ", "mmCIF", CommandOutputTone::Success)],
    };

    assert_eq!(report.plain_text(), "Format: mmCIF");
  }

  #[test]
  fn database_report_should_use_the_shared_success_message() {
    let report = render_outcome(&CommandOutcome::DatabaseDownload { artifact_count: 2 });

    assert_eq!(report.plain_text(), "Downloaded 2 structure artifact(s).");
  }

  #[test]
  fn invalid_structure_report_should_carry_failure_without_a_second_error() {
    let outcome = CommandOutcome::StructureValidation(StructureValidation {
      path: "invalid.pdb".into(),
      format: chitin_databases::providers::rcsb::StructureFormat::Pdb,
      output: CommandOutputFormat::Text,
      error: Some("no atom records were found".to_owned()),
    });

    let report = render_outcome(&outcome);

    assert_eq!(
      (report.status, report.plain_text()),
      (
        CommandReportStatus::Failed,
        "✗ Invalid structure: no atom records were found".to_owned()
      )
    );
  }
}
