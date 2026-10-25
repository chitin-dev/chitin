//! Process CLI dispatch and file safety for shared chemical drawing commands.
//!
//! Tests invoke the built binary so argument parsing, stdin collection, output
//! reporting and exit status are checked together with portable execution.

/// Propagates process-launch and fixture I/O failures without test-only panics.
type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn cli_should_export_smiles_and_refuse_silent_overwrites() -> TestResult {
  let directory = tempfile::tempdir()?;
  let run = || {
    std::process::Command::new(env!("CARGO_BIN_EXE_chitin"))
      .current_dir(directory.path())
      .args(["chemdraw", "smiles", "CO", "-o", "molecule.svg"])
      .output()
  };
  let result = run()?;
  assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
  assert!(String::from_utf8_lossy(&result.stdout).contains("Wrote SVG"));
  let original = std::fs::read(directory.path().join("molecule.svg"))?;
  assert!(!run()?.status.success());
  assert_eq!(std::fs::read(directory.path().join("molecule.svg"))?, original);
  Ok(())
}

#[test]
fn cli_should_expose_diagram_help_and_reject_invalid_dimensions() -> TestResult {
  let binary = env!("CARGO_BIN_EXE_chitin");
  let output = std::process::Command::new(binary)
    .args(["chemdraw", "diagram", "--help"])
    .output()?;
  assert!(output.status.success());
  assert!(String::from_utf8_lossy(&output.stdout).contains("--force"));
  let output = std::process::Command::new(binary)
    .args(["chemdraw", "smiles", "CO", "-o", "unused.svg", "--stroke-width", "NaN"])
    .output()?;
  assert!(!output.status.success());
  Ok(())
}

#[test]
fn cli_should_render_a_diagram_from_standard_input() -> TestResult {
  use std::io::Write;
  let directory = tempfile::tempdir()?;
  let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_chitin"))
    .current_dir(directory.path())
    .args(["chemdraw", "diagram", "-", "-o", "reaction.svg"])
    .stdin(std::process::Stdio::piped())
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::piped())
    .spawn()?;
  let mut stdin = child.stdin.take().ok_or("missing child stdin")?;
  stdin.write_all(include_bytes!("../../chitin-command/examples/reaction.yaml"))?;
  // The bounded reader still waits for EOF on a short document; close the pipe
  // before waiting for the child's final result.
  drop(stdin);
  let result = child.wait_with_output()?;
  assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
  assert!(std::fs::read_to_string(directory.path().join("reaction.svg"))?.contains(" C"));
  Ok(())
}
