//! Exercises the real executable, including path resolution and safe replacement.

use std::{
  fs,
  path::Path,
  process::{Command, Output},
};

use anyhow::Result;

/// Invokes the binary outside the configuration directory to expose CWD coupling.
fn invoke(config: &Path, overwrite: bool) -> Result<Output> {
  let mut command = Command::new(env!("CARGO_BIN_EXE_chitin-chemdraw-batch"));
  command.current_dir(std::env::temp_dir()).arg(config);
  if overwrite {
    command.arg("--overwrite");
  }
  Ok(command.output()?)
}

#[test]
fn batch_should_generate_numbered_svgs_relative_to_yaml() -> Result<()> {
  let root = tempfile::tempdir()?;
  let config = root.path().join("job.yaml");
  fs::write(&config, "smiles: ['CCO', 'CCO']")?;
  let output = invoke(&config, false)?;
  assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
  let first = fs::read_to_string(root.path().join("generated/0001.svg"))?;
  let second = fs::read_to_string(root.path().join("generated/0002.svg"))?;
  assert!(first.starts_with("<svg "));
  assert_eq!(first, second);
  Ok(())
}

#[test]
fn bad_entry_should_not_stop_later_entries_or_renumber_them() -> Result<()> {
  let root = tempfile::tempdir()?;
  let config = root.path().join("job.yaml");
  fs::write(&config, "smiles: ['C', 'not-smiles', '', 'O']")?;
  let output = invoke(&config, false)?;
  assert!(!output.status.success());
  assert!(root.path().join("generated/0001.svg").exists());
  assert!(!root.path().join("generated/0002.svg").exists());
  assert!(!root.path().join("generated/0003.svg").exists());
  assert!(root.path().join("generated/0004.svg").exists());
  assert!(String::from_utf8_lossy(&output.stdout).contains("2 generated, 2 failed"));
  assert!(String::from_utf8_lossy(&output.stderr).contains("[2]"));
  Ok(())
}

#[test]
fn existing_output_should_require_explicit_replacement() -> Result<()> {
  let root = tempfile::tempdir()?;
  let config = root.path().join("job.yaml");
  fs::write(&config, "smiles: ['CC']")?;
  let directory = root.path().join("generated");
  fs::create_dir(&directory)?;
  let target = directory.join("0001.svg");
  fs::write(&target, "previous output")?;
  assert!(!invoke(&config, false)?.status.success());
  assert_eq!(fs::read_to_string(&target)?, "previous output");
  assert!(invoke(&config, true)?.status.success());
  assert!(fs::read_to_string(&target)?.starts_with("<svg "));
  Ok(())
}

#[test]
fn failed_render_should_preserve_previous_output_even_with_overwrite() -> Result<()> {
  let root = tempfile::tempdir()?;
  let config = root.path().join("job.yaml");
  fs::write(&config, "smiles: ['C1']")?;
  fs::create_dir(root.path().join("generated"))?;
  let target = root.path().join("generated/0001.svg");
  fs::write(&target, "previous output")?;
  assert!(!invoke(&config, true)?.status.success());
  assert_eq!(fs::read_to_string(&target)?, "previous output");
  Ok(())
}

#[test]
fn drawing_options_should_control_strokes_background_and_chiral_hydrogens() -> Result<()> {
  let root = tempfile::tempdir()?;
  let config = root.path().join("job.yaml");
  fs::write(
    &config,
    "output_dir: images\ndepict:\n  stroke_width: 2.5\n  explicit_chiral_hydrogens: true\nsvg:\n  transparent: true\nsmiles: ['N[C@@H](C)C(=O)O']",
  )?;
  let output = invoke(&config, false)?;
  assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
  let svg = fs::read_to_string(root.path().join("images/0001.svg"))?;
  assert!(svg.contains("stroke-width=\"2.5\""));
  assert!(svg.contains("aria-label=\"H\""));
  assert!(!svg.contains("<rect "));
  Ok(())
}

#[test]
fn malformed_configuration_should_not_create_output_directory() -> Result<()> {
  let root = tempfile::tempdir()?;
  let config = root.path().join("job.yaml");
  fs::write(&config, "smiles: ['C']\ndepict: {stroke_with: 2}")?;
  assert!(!invoke(&config, false)?.status.success());
  assert!(!root.path().join("generated").exists());
  Ok(())
}

#[test]
fn invalid_size_should_fail_without_publishing_svg() -> Result<()> {
  let root = tempfile::tempdir()?;
  let config = root.path().join("job.yaml");
  fs::write(&config, "smiles: ['C']\ndepict: {stroke_width: -1}")?;
  assert!(!invoke(&config, false)?.status.success());
  assert!(!root.path().join("generated/0001.svg").exists());
  Ok(())
}

#[test]
fn element_mode_should_export_builtin_and_overridden_label_colors_with_black_bonds() -> Result<()> {
  let root = tempfile::tempdir()?;
  let config = root.path().join("job.yaml");
  fs::write(
    &config,
    "depict:\n  color_scheme: element\n  element_colors: {O: '#12345680'}\nsmiles: ['NCCO']",
  )?;
  let output = invoke(&config, false)?;
  assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
  let svg = fs::read_to_string(root.path().join("generated/0001.svg"))?;
  assert!(svg.contains("fill=\"#123456\""));
  assert!(svg.contains("fill-opacity=\"0.5019607843137255\""));
  assert!(svg.contains("fill=\"#3058d8\""));
  assert!(svg.contains("stroke=\"#000000\""));
  Ok(())
}

#[test]
fn invalid_palette_should_fail_before_creating_output_directory() -> Result<()> {
  let root = tempfile::tempdir()?;
  let config = root.path().join("job.yaml");
  fs::write(&config, "smiles: ['CO']\ndepict: {element_colors: {O: '#12zz00'}}")?;
  let output = invoke(&config, false)?;
  assert!(!output.status.success());
  assert!(String::from_utf8_lossy(&output.stderr).contains("element_colors.O"));
  assert!(!root.path().join("generated").exists());
  Ok(())
}

#[test]
fn circle_option_should_export_inner_circles_without_inner_double_strokes() -> Result<()> {
  let root = tempfile::tempdir()?;
  let config = root.path().join("job.yaml");
  fs::write(
    &config,
    "depict:\n  aromatic_style: circle\n  color_scheme: element\nsmiles: ['c1ccncc1', 'c1ccc2ccccc2c1']",
  )?;
  let output = invoke(&config, false)?;
  assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
  let single = fs::read_to_string(root.path().join("generated/0001.svg"))?;
  assert_eq!(single.matches("<circle ").count(), 1);
  assert_eq!(single.matches("<line ").count(), 6);
  assert!(single.contains("fill=\"#3058d8\""));
  let fused = fs::read_to_string(root.path().join("generated/0002.svg"))?;
  assert!(!fused.contains("<circle "));
  Ok(())
}
