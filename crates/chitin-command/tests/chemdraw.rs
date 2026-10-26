//! Shared execution contracts for drawing, persistence and source isolation.

use chitin_command::{
  ChemdrawCommand, ChemdrawInput, ChemdrawStyle, CommandEventSink, CommandExecutionContext, CommandExecutor,
  CommandOutcome, CommandReportStatus, DrawingLength, render_outcome,
};
use chitin_databases::ClientConfig;
use std::path::PathBuf;

/// Shared test error boundary covering command failures and fixture filesystem I/O.
type TestResult = Result<(), Box<dyn std::error::Error>>;

#[tokio::test]
async fn diagram_arc_mode_should_export_native_svg_arcs() -> TestResult {
  let directory = tempfile::tempdir()?;
  let source = br#"{"version":1,"molecules":[{"id":"a","smiles":"[Cl-]","at":[0,0]},{"id":"b","smiles":"[H+]","at":[200,0]}],"annotations":[{"type":"electron-arrow","curve":"arc","arc-angle":-120,"from":{"molecule":"a","target":{"type":"atom","index":0}},"to":{"molecule":"b","target":{"type":"atom","index":0}}}]}"#;
  execute(
    command(ChemdrawInput::Diagram("-".into()), false),
    CommandExecutionContext::new(directory.path()).with_standard_input(source.to_vec()),
  )
  .await?;
  assert!(std::fs::read_to_string(directory.path().join("out.svg"))?.contains(" A"));
  Ok(())
}

#[tokio::test]
async fn circled_charges_and_authored_controls_should_be_available_to_commands() -> TestResult {
  let directory = tempfile::tempdir()?;
  let source = br#"{"version":1,"molecules":[{"id":"a","smiles":"[OH-]","at":[0,0]},{"id":"b","smiles":"[H+]","at":[200,0]}],"annotations":[{"type":"electron-arrow","from":{"molecule":"a","target":{"type":"atom","index":0},"offset":[0,15]},"to":{"molecule":"b","target":{"type":"bond-formation","index":0,"other_molecule":"a","other_index":0}},"bend":0,"controls":[[30,40],[-10,40]]},{"type":"electron-dot","anchor":{"molecule":"a","target":{"type":"radical","index":0},"offset":[20,-10]}}]}"#;
  let mut request = command(ChemdrawInput::Diagram("-".into()), false);
  request.style.circled_charges = true;
  execute(
    request,
    CommandExecutionContext::new(directory.path()).with_standard_input(source.to_vec()),
  )
  .await?;
  let svg = std::fs::read_to_string(directory.path().join("out.svg"))?;
  assert!(svg.matches("<circle").count() >= 3);
  assert!(svg.contains(" C"));
  Ok(())
}

/// Creates the same contextual output target for each isolated temporary directory.
fn command(input: ChemdrawInput, force: bool) -> ChemdrawCommand {
  ChemdrawCommand {
    input,
    output: PathBuf::from("out.svg"),
    force,
    style: ChemdrawStyle::default(),
  }
}
/// Exercises the public portable executor rather than the internal drawing worker.
async fn execute(
  command: ChemdrawCommand,
  context: CommandExecutionContext,
) -> Result<CommandOutcome, chitin_command::CommandExecutionError> {
  CommandExecutor::new(ClientConfig::default())
    .execute(command.into(), context, CommandEventSink::silent())
    .await
}

#[tokio::test]
async fn smiles_should_export_a_portable_svg_and_shared_report() -> TestResult {
  let directory = tempfile::tempdir()?;
  let outcome = execute(
    command(ChemdrawInput::Smiles("CCO".into()), false),
    CommandExecutionContext::new(directory.path()),
  )
  .await?;
  let CommandOutcome::ChemdrawExport(export) = &outcome else {
    return Err("unexpected outcome".into());
  };
  assert_eq!(export.path, directory.path().join("out.svg"));
  let svg = std::fs::read_to_string(&export.path)?;
  assert!(svg.starts_with("<svg"));
  assert!(!svg.contains("<text"));
  assert_eq!(svg.len(), export.byte_count);
  assert_eq!(render_outcome(&outcome).status, CommandReportStatus::Succeeded);
  Ok(())
}

#[tokio::test]
async fn outputs_should_be_protected_without_force_and_replaced_with_force() -> TestResult {
  let directory = tempfile::tempdir()?;
  let path = directory.path().join("out.svg");
  std::fs::write(&path, "original")?;
  let context = CommandExecutionContext::new(directory.path());
  assert!(
    execute(command(ChemdrawInput::Smiles("CO".into()), false), context.clone())
      .await
      .is_err()
  );
  assert_eq!(std::fs::read_to_string(&path)?, "original");
  execute(command(ChemdrawInput::Smiles("CO".into()), true), context).await?;
  assert!(std::fs::read_to_string(path)?.starts_with("<svg"));
  Ok(())
}

#[tokio::test]
async fn invalid_input_with_force_should_leave_existing_output_intact() -> TestResult {
  let directory = tempfile::tempdir()?;
  let path = directory.path().join("out.svg");
  std::fs::write(&path, "original")?;
  assert!(
    execute(
      command(ChemdrawInput::Smiles("C1".into()), true),
      CommandExecutionContext::new(directory.path())
    )
    .await
    .is_err()
  );
  assert_eq!(std::fs::read_to_string(path)?, "original");
  assert_eq!(std::fs::read_dir(directory.path())?.count(), 1);
  Ok(())
}

#[tokio::test]
async fn diagram_example_should_render_from_frontend_stdin() -> TestResult {
  let directory = tempfile::tempdir()?;
  let context = CommandExecutionContext::new(directory.path())
    .with_standard_input(include_bytes!("../examples/reaction.yaml").to_vec());
  let outcome = execute(command(ChemdrawInput::Diagram(PathBuf::from("-")), false), context).await?;
  let CommandOutcome::ChemdrawExport(export) = outcome else {
    return Err("unexpected outcome".into());
  };
  assert_eq!(export.molecule_count, 3);
  let svg = std::fs::read_to_string(export.path)?;
  assert!(svg.contains("opacity=\"0.39215686274509803\""));
  assert!(svg.contains(" C"));
  Ok(())
}

#[tokio::test]
async fn invalid_versions_and_foreign_references_should_not_create_output() -> TestResult {
  // These inputs distinguish document identity errors from occurrence-local
  // index errors; none may leave behind a partially published SVG.
  for input in [
    r#"{"version":2,"molecules":[]}"#,
    r#"{"version":1,"molecules":[{"id":"a","smiles":"CO","at":[0,0]},{"id":"a","smiles":"CO","at":[100,0]}]}"#,
    r#"{"version":1,"molecules":[{"id":"a","smiles":"CO","at":[0,0]}],"annotations":[{"type":"highlight","molecule":"other","atoms":[0]}]}"#,
    r#"{"version":1,"molecules":[{"id":"a","smiles":"CO","at":[0,0]}],"annotations":[{"type":"highlight","molecule":"a","atoms":[99]}]}"#,
  ] {
    let directory = tempfile::tempdir()?;
    let context = CommandExecutionContext::new(directory.path()).with_standard_input(input.as_bytes().to_vec());
    assert!(
      execute(command(ChemdrawInput::Diagram("-".into()), false), context)
        .await
        .is_err()
    );
    assert!(!directory.path().join("out.svg").exists());
  }
  Ok(())
}

#[tokio::test]
async fn cancellation_should_prevent_persistence() -> TestResult {
  let directory = tempfile::tempdir()?;
  let context = CommandExecutionContext::new(directory.path());
  context.cancellation.cancel();
  assert!(
    execute(command(ChemdrawInput::Smiles("CO".into()), false), context)
      .await
      .is_err()
  );
  assert!(!directory.path().join("out.svg").exists());
  Ok(())
}

#[tokio::test]
async fn input_document_should_remain_protected_even_with_force() -> TestResult {
  let directory = tempfile::tempdir()?;
  let path = directory.path().join("out.svg");
  std::fs::write(&path, "original input")?;
  assert!(
    execute(
      command(ChemdrawInput::Diagram("out.svg".into()), true),
      CommandExecutionContext::new(directory.path())
    )
    .await
    .is_err()
  );
  assert_eq!(std::fs::read_to_string(path)?, "original input");
  Ok(())
}

#[test]
fn drawing_sizes_should_reject_nonfinite_or_nonpositive_values() {
  for value in ["NaN", "inf", "-1", "0", "garbage"] {
    assert!(value.parse::<DrawingLength>().is_err());
  }
  assert!("1.25".parse::<DrawingLength>().is_ok());
}

#[tokio::test]
async fn oversized_stdin_should_be_rejected_before_rendering() -> TestResult {
  let directory = tempfile::tempdir()?;
  let context = CommandExecutionContext::new(directory.path()).with_standard_input(vec![b' '; 4 * 1024 * 1024 + 1]);
  assert!(
    execute(command(ChemdrawInput::Diagram("-".into()), false), context)
      .await
      .is_err()
  );
  assert!(!directory.path().join("out.svg").exists());
  Ok(())
}

#[tokio::test]
async fn a_missing_stdin_resource_should_fail_without_creating_output() -> TestResult {
  let directory = tempfile::tempdir()?;
  let context = CommandExecutionContext::new(directory.path());
  assert!(
    execute(command(ChemdrawInput::Smiles("-".into()), false), context)
      .await
      .is_err()
  );
  assert!(!directory.path().join("out.svg").exists());
  Ok(())
}
