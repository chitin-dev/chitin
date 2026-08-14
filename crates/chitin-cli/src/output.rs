//! Output path resolution for downloaded structures.

use std::path::PathBuf;

use chitin_databases::providers::rcsb::{PdbId, StructureFormat};

use crate::error::CliError;

/// Resolves an optional CLI output into the final file path.
///
/// # Parameters
///
/// * `output` is an optional explicit file or directory.
/// * `id` supplies the generated filename when a directory is selected.
/// * `format` supplies the generated file extension and default directory.
///
/// # Returns
///
/// The path where the downloaded bytes should be written.
pub(crate) fn resolve_output_path(
  output: Option<PathBuf>,
  id: &PdbId,
  format: StructureFormat,
  multiple: bool,
) -> Result<PathBuf, CliError> {
  let Some(output) = output else {
    return Ok(
      home_directory()?
        .join(".chitin")
        .join("download")
        .join(format.id())
        .join(format.filename(id)),
    );
  };
  if output.is_dir() {
    return Ok(output.join(format.filename(id)));
  }
  if multiple && (output.extension().is_some() || output.exists()) {
    return Err(CliError::MultipleOutputFile(output));
  }
  if multiple {
    return Ok(output.join(format.filename(id)));
  }
  Ok(output)
}

/// Finds the platform home directory used by the default output path.
fn home_directory() -> Result<PathBuf, CliError> {
  std::env::var_os("HOME")
    .or_else(|| std::env::var_os("USERPROFILE"))
    .map(PathBuf::from)
    .ok_or(CliError::HomeDirectory)
}

#[cfg(test)]
mod tests {
  use super::*;

  fn test_id() -> PdbId {
    match PdbId::new("4hhb") {
      Ok(id) => id,
      Err(_) => panic!("test identifier must be valid"),
    }
  }

  #[test]
  fn existing_directory_should_receive_generated_filename() {
    let directory = std::env::temp_dir();
    let resolved = resolve_output_path(Some(directory.clone()), &test_id(), StructureFormat::Pdb, false);
    assert!(matches!(resolved, Ok(path) if path == directory.join("4HHB.pdb")));
  }

  #[test]
  fn nonexistent_extensionless_output_should_be_used_as_file() {
    let path = PathBuf::from("custom/structure");
    let resolved = resolve_output_path(Some(path.clone()), &test_id(), StructureFormat::Pdb, false);
    assert!(matches!(resolved, Ok(resolved) if resolved == path));
  }

  #[test]
  fn path_with_extension_should_be_used_as_file() {
    let path = PathBuf::from("custom/output.cif");
    let resolved = resolve_output_path(Some(path.clone()), &test_id(), StructureFormat::Mmcif, false);
    assert!(matches!(resolved, Ok(resolved) if resolved == path));
  }
}
