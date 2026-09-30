//! Structure-file parsing independent of GPU presentation.

use chitin_bio::structure::{MmcifParser, PdbParser, StructureScene};
use std::{path::Path, sync::Arc};

/// Loads a local PDB or mmCIF file and extracts its first renderable model.
pub(crate) fn load_structure_scene(path: &Path) -> Result<Arc<StructureScene>, String> {
  let bytes = std::fs::read(path).map_err(|error| format!("cannot read '{}': {error}", path.display()))?;
  let extension = path
    .extension()
    .and_then(|extension| extension.to_str())
    .map(str::to_ascii_lowercase);
  let structure = match extension.as_deref() {
    Some("pdb") | Some("ent") => PdbParser::new()
      .parse_bytes(&bytes)
      .map(|parsed| parsed.structure)
      .map_err(|error| error.to_string())?,
    Some("cif") | Some("mmcif") => MmcifParser::new()
      .parse_bytes(&bytes)
      .map(|parsed| parsed.structure)
      .map_err(|error| error.to_string())?,
    _ => return Err("expected a .pdb, .ent, .cif, or .mmcif file".to_string()),
  };
  StructureScene::from_first_model(&structure)
    .map(Arc::new)
    .map_err(|error| error.to_string())
}
