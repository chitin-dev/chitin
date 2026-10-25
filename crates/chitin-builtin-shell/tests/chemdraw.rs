//! Built-in shell discovery, portable routing and actual SVG execution.
//!
//! A session without any rendering panel must still use the same scientific
//! executor as the process CLI; shell tokenization must preserve stereo slashes.

use chitin_builtin_shell::{BuiltinCommandLine, BuiltinShell, complete_builtin_shell_line, parse_builtin_command_line};
use chitin_command::{CommandExecutionDomain, CommandExecutor, CommandId};
use chitin_databases::ClientConfig;

/// Propagates shell, command and temporary-file failures through the test harness.
type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn quoted_smiles_should_route_to_the_shared_portable_command() -> TestResult {
  let line = parse_builtin_command_line("chemdraw smiles 'F/C=C\\F' -o molecule.svg")?;
  let BuiltinCommandLine::Portable(command) = line else {
    return Err("expected portable command".into());
  };
  assert_eq!(command.id(), CommandId::ChemdrawSmiles);
  let chitin_command::PortableCommand::Chemdraw(command) = command else {
    return Err("wrong workflow".into());
  };
  assert_eq!(command.input, chitin_command::ChemdrawInput::Smiles(r"F/C=C\F".into()));
  assert!(
    complete_builtin_shell_line("chemdraw ")
      .iter()
      .any(|s| s.contains("diagram"))
  );
  Ok(())
}

#[tokio::test]
async fn shell_should_draw_without_a_rendering_panel() -> TestResult {
  let directory = tempfile::tempdir()?;
  let shell = BuiltinShell::new(directory.path());
  let submission = shell.submit("chemdraw smiles 'CCO' -o shell.svg")?.into_command()?;
  assert_eq!(submission.target(), CommandExecutionDomain::Portable);
  let executor = CommandExecutor::new(ClientConfig::default());
  shell.execute_portable(submission, &executor).await?;
  assert!(std::fs::read_to_string(directory.path().join("shell.svg"))?.starts_with("<svg"));
  Ok(())
}
