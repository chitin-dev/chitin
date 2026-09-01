//! Explicit atom connectivity projected from `_struct_conn`.

use crate::structure::mmcif::cif::CifDocument;
use crate::structure::mmcif::schema::StructConn;
use crate::structure::projection::{AtomLookupKey, StructureInput};
use crate::structure::{BondSource, MmcifParseError};

use super::optional_text_i32;

/// Projects explicit atom-pair relations into format-neutral structure input.
///
/// # Parameters
///
/// * `document` contains the optional `_struct_conn` category.
/// * `input` receives endpoint pairs for resolution after atom indexing.
///
/// # Returns
///
/// `Ok(())` when the category is absent or every endpoint is valid.
pub(crate) fn parse(document: &CifDocument, input: &mut StructureInput) -> Result<(), MmcifParseError> {
  let Some(category) = StructConn::from_document(document) else {
    return Ok(());
  };
  for row in category.rows() {
    let bond_source = if row
      .conn_type_id()
      .is_some_and(|connection_type| connection_type.eq_ignore_ascii_case("metalc"))
    {
      BondSource::StructConnMetalCoordination
    } else {
      BondSource::StructConn
    };
    // Auth and label identifiers may be mixed within one endpoint in RCSB
    // files, so fallback is performed independently for every identifier.
    let first = endpoint(
      row.row_number(),
      "partner 1",
      row.ptnr1_auth_asym_id().or_else(|| row.ptnr1_label_asym_id()),
      selected_sequence(row.row_number(), "partner 1 sequence", row.ptnr1_auth_seq_id(), || {
        row.ptnr1_label_seq_id()
      })?,
      row.ptnr1_auth_atom_id().or_else(|| row.ptnr1_label_atom_id()),
      row.pdbx_ptnr1_pdb_ins_code().and_then(|value| value.chars().next()),
      row.pdbx_ptnr1_label_alt_id().and_then(|value| value.chars().next()),
    )?;
    let second = endpoint(
      row.row_number(),
      "partner 2",
      row.ptnr2_auth_asym_id().or_else(|| row.ptnr2_label_asym_id()),
      selected_sequence(row.row_number(), "partner 2 sequence", row.ptnr2_auth_seq_id(), || {
        row.ptnr2_label_seq_id()
      })?,
      row.ptnr2_auth_atom_id().or_else(|| row.ptnr2_label_atom_id()),
      row.pdbx_ptnr2_pdb_ins_code().and_then(|value| value.chars().next()),
      row.pdbx_ptnr2_label_alt_id().and_then(|value| value.chars().next()),
    )?;
    input.add_named_bond(
      row.row_number(),
      first,
      second,
      bond_source,
      "MMCIF_STRUCT_CONN_UNKNOWN_ATOM",
    );
  }
  Ok(())
}

/// Validates and normalizes one `_struct_conn` endpoint.
///
/// # Parameters
///
/// * `row` is the one-based category row number used in diagnostics.
/// * `name` identifies the partner in diagnostics.
/// * `chain_id`, `sequence_number`, and `atom_name` are required endpoint identifiers.
/// * `insertion_code` and `altloc` distinguish optional residue and atom variants.
///
/// # Returns
///
/// An atom lookup key, or a field error naming the missing endpoint component.
fn endpoint(
  row: usize,
  name: &'static str,
  chain_id: Option<&str>,
  sequence_number: Option<i32>,
  atom_name: Option<&str>,
  insertion_code: Option<char>,
  altloc: Option<char>,
) -> Result<AtomLookupKey, MmcifParseError> {
  let chain_id = required_endpoint_value(row, name, "chain", chain_id)?;
  let sequence_number = sequence_number.ok_or_else(|| MmcifParseError::InvalidField {
    row,
    field: "_struct_conn endpoint sequence",
    value: format!("{name} has no author or label sequence identifier"),
  })?;
  let atom_name = required_endpoint_value(row, name, "atom", atom_name)?;
  Ok(AtomLookupKey {
    chain_id: Some(chain_id.to_owned()),
    sequence_number,
    atom_name: atom_name.to_owned(),
    insertion_code,
    altloc,
  })
}

/// Selects and parses an author sequence before its label fallback.
fn selected_sequence(
  row: usize,
  field: &'static str,
  author: Option<&str>,
  label: impl FnOnce() -> Result<Option<i32>, MmcifParseError>,
) -> Result<Option<i32>, MmcifParseError> {
  if author.is_some() {
    optional_text_i32(row, field, author)
  } else {
    label()
  }
}

/// Returns a required textual endpoint component.
fn required_endpoint_value<'a>(
  row: usize,
  endpoint: &'static str,
  component: &'static str,
  value: Option<&'a str>,
) -> Result<&'a str, MmcifParseError> {
  value.ok_or_else(|| MmcifParseError::InvalidField {
    row,
    field: match component {
      "chain" => "_struct_conn endpoint chain",
      _ => "_struct_conn endpoint atom",
    },
    value: format!("{endpoint} has no author or label {component} identifier"),
  })
}
