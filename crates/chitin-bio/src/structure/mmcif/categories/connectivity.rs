//! Explicit atom connectivity projected from `_struct_conn`.

use crate::structure::mmcif::cif::CifDocument;
use crate::structure::mmcif::schema::StructConn;
use crate::structure::projection::{AtomLookupKey, AtomLookupNamespace, StructureInput};
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
    let bond_source = match row.conn_type_id().map(str::to_ascii_lowercase).as_deref() {
      Some("metalc") => BondSource::StructConnMetalCoordination,
      Some("hydrog") => BondSource::StructConnHydrogenBond,
      Some("saltbr") => BondSource::StructConnSaltBridge,
      Some("disulf") => BondSource::StructConnDisulfide,
      Some("mismat") => BondSource::StructConnBaseMismatch,
      Some("covale_base") => BondSource::StructConnCovalentBase,
      Some("covale_phosphate") => BondSource::StructConnCovalentPhosphate,
      Some("covale_sugar") => BondSource::StructConnCovalentSugar,
      Some("modres") => BondSource::StructConnResidueModification,
      _ => BondSource::StructConn,
    };
    // Auth and label identifiers may be mixed within one endpoint in RCSB
    // files, so fallback is performed independently for every identifier.
    let first_author_chain = row.ptnr1_auth_asym_id();
    let first_author_sequence = row.ptnr1_auth_seq_id();
    let first_author_atom = row.ptnr1_auth_atom_id();
    let first_uses_author = first_author_chain.is_some() && first_author_sequence.is_some();
    let first_label_sequence = if first_uses_author {
      None
    } else {
      row.ptnr1_label_seq_id()?
    };
    let first = endpoint(
      row.row_number(),
      "partner 1",
      EndpointIdentifiers {
        author_chain_id: first_author_chain,
        author_sequence_number: optional_text_i32(row.row_number(), "partner 1 sequence", first_author_sequence)?,
        author_atom_name: first_author_atom,
        label_chain_id: row.ptnr1_label_asym_id(),
        label_sequence_number: first_label_sequence,
        label_atom_name: row.ptnr1_label_atom_id(),
      },
      row.pdbx_ptnr1_pdb_ins_code().and_then(|value| value.chars().next()),
      row.pdbx_ptnr1_label_alt_id().and_then(|value| value.chars().next()),
    )?;
    let second_author_chain = row.ptnr2_auth_asym_id();
    let second_author_sequence = row.ptnr2_auth_seq_id();
    let second_author_atom = row.ptnr2_auth_atom_id();
    let second_uses_author = second_author_chain.is_some() && second_author_sequence.is_some();
    let second_label_sequence = if second_uses_author {
      None
    } else {
      row.ptnr2_label_seq_id()?
    };
    let second = endpoint(
      row.row_number(),
      "partner 2",
      EndpointIdentifiers {
        author_chain_id: second_author_chain,
        author_sequence_number: optional_text_i32(row.row_number(), "partner 2 sequence", second_author_sequence)?,
        author_atom_name: second_author_atom,
        label_chain_id: row.ptnr2_label_asym_id(),
        label_sequence_number: second_label_sequence,
        label_atom_name: row.ptnr2_label_atom_id(),
      },
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
struct EndpointIdentifiers<'a> {
  author_chain_id: Option<&'a str>,
  author_sequence_number: Option<i32>,
  author_atom_name: Option<&'a str>,
  label_chain_id: Option<&'a str>,
  label_sequence_number: Option<i32>,
  label_atom_name: Option<&'a str>,
}

fn endpoint(
  row: usize,
  name: &'static str,
  identifiers: EndpointIdentifiers<'_>,
  insertion_code: Option<char>,
  altloc: Option<char>,
) -> Result<AtomLookupKey, MmcifParseError> {
  let EndpointIdentifiers {
    author_chain_id,
    author_sequence_number,
    author_atom_name,
    label_chain_id,
    label_sequence_number,
    label_atom_name,
  } = identifiers;
  let (namespace, chain_id, sequence_number, atom_name) =
    if author_chain_id.is_some() && author_sequence_number.is_some() {
      (
        AtomLookupNamespace::Author,
        author_chain_id,
        author_sequence_number,
        author_atom_name.or(label_atom_name),
      )
    } else {
      (
        AtomLookupNamespace::Label,
        label_chain_id,
        label_sequence_number,
        label_atom_name,
      )
    };
  let chain_id = required_endpoint_value(row, name, "chain", chain_id)?;
  let sequence_number = sequence_number.ok_or_else(|| MmcifParseError::InvalidField {
    row,
    field: "_struct_conn endpoint sequence",
    value: format!("{name} has no author or label sequence identifier"),
  })?;
  let atom_name = required_endpoint_value(row, name, "atom", atom_name)?;
  Ok(AtomLookupKey {
    namespace,
    chain_id: Some(chain_id.to_owned()),
    sequence_number,
    atom_name: atom_name.to_owned(),
    insertion_code,
    altloc,
  })
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
