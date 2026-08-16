//! Atomic coordinate projection from `_atom_site`.

use crate::structure::mmcif::category::TypedRow;
use crate::structure::mmcif::cif::CifDocument;
use crate::structure::mmcif::schema::AtomSite;
use crate::structure::pdb::{AtomRecord, StructureBuilder};
use crate::structure::{Element, MmcifParseError, ResidueKind};

use super::{map_builder_error, optional_text_i32, required};

/// Projects the required atom-site loop into shared topology and coordinates.
///
/// # Parameters
///
/// * `document` contains the generic CIF document.
/// * `builder` receives atom records grouped into coordinate models.
///
/// # Returns
///
/// `Ok(())` after every atom-site row is accepted. A missing/empty category or
/// malformed required field returns [`MmcifParseError`].
pub(crate) fn parse(document: &CifDocument, builder: &mut StructureBuilder) -> Result<(), MmcifParseError> {
  let Some(category) = AtomSite::from_document(document) else {
    return Err(MmcifParseError::InvalidToken {
      line: 0,
      message: "missing atom_site loop".to_owned(),
    });
  };
  let mut rows = category.rows().peekable();
  if rows.peek().is_none() {
    return Err(MmcifParseError::InvalidToken {
      line: 0,
      message: "atom_site loop has no rows".to_owned(),
    });
  }

  let mut current_model = None;
  for row in rows {
    let model_number = row.pdbx_pdb_model_num()?.unwrap_or(1);
    if current_model != Some(model_number) {
      builder.start_model(model_number);
      current_model = Some(model_number);
    }
    builder.add_atom(atom_record(row)?).map_err(map_builder_error)?;
  }
  Ok(())
}

/// Converts one typed atom-site row into a format-independent atom record.
///
/// # Parameters
///
/// * `row` exposes validated typed fields from one `_atom_site` row.
///
/// # Returns
///
/// A shared [`AtomRecord`] using author identifiers when present and label
/// identifiers as fallback aliases.
fn atom_record(row: TypedRow<'_, AtomSite>) -> Result<AtomRecord, MmcifParseError> {
  let row_number = row.row_number();
  let atom_name = required(
    row_number,
    "atom name",
    row.auth_atom_id().or_else(|| row.label_atom_id()),
  )?;
  let residue_name = required(
    row_number,
    "residue name",
    row.auth_comp_id().or_else(|| row.label_comp_id()),
  )?;
  let sequence_number = if let Some(author_sequence) = row.auth_seq_id() {
    required(
      row_number,
      "residue sequence",
      optional_text_i32(row_number, "residue sequence", Some(author_sequence))?,
    )?
  } else {
    required(row_number, "residue sequence", row.label_seq_id()?)?
  };

  Ok(AtomRecord {
    line: row_number,
    serial: optional_text_i32(row_number, "atom-site id", row.id())?,
    name: atom_name.to_owned(),
    element: row.type_symbol().map(|value| Element(value.to_owned())),
    chain_id: row.auth_asym_id().or_else(|| row.label_asym_id()).map(str::to_owned),
    residue_name: residue_name.to_owned(),
    sequence_number,
    insertion_code: row.pdbx_pdb_ins_code().and_then(|value| value.chars().next()),
    altloc: row.label_alt_id().and_then(|value| value.chars().next()),
    occupancy: row.occupancy()?,
    b_factor: row.b_iso_or_equiv()?,
    formal_charge: row.pdbx_formal_charge()?.and_then(|value| i8::try_from(value).ok()),
    position: [
      required(row_number, "x coordinate", row.cartn_x()?)?,
      required(row_number, "y coordinate", row.cartn_y()?)?,
      required(row_number, "z coordinate", row.cartn_z()?)?,
    ],
    kind: if row.group_pdb() == Some("HETATM") {
      ResidueKind::Hetero
    } else {
      ResidueKind::Polymer
    },
    label_chain_id: row.label_asym_id().map(str::to_owned),
    label_sequence_number: row.label_seq_id()?,
    label_atom_name: row.label_atom_id().map(str::to_owned),
    label_entity_id: row.label_entity_id().map(str::to_owned),
  })
}
