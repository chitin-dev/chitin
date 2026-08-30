//! Polymer entity, complete sequence, and chain-assignment categories.

use crate::structure::mmcif::cif::CifDocument;
use crate::structure::mmcif::schema::{EntityPoly, EntityPolySeq, StructAsym};
use crate::structure::projection::StructureInput;
use crate::structure::{MmcifParseError, PolymerEntity, PolymerSequenceResidue, PolymerType};

use super::required;

/// Projects entity metadata, sequences, and chain assignments into shared input.
///
/// # Parameters
///
/// * `document` contains generic CIF categories.
/// * `input` receives normalized entity declarations.
///
/// # Returns
///
/// `Ok(())` when absent categories are skipped and present rows are valid.
pub(crate) fn parse(document: &CifDocument, input: &mut StructureInput) -> Result<(), MmcifParseError> {
  parse_struct_asym(document, input)?;
  parse_entity_poly(document, input)?;
  parse_entity_poly_seq(document, input)
}

/// Adds label-chain to entity mappings from `_struct_asym`.
fn parse_struct_asym(document: &CifDocument, input: &mut StructureInput) -> Result<(), MmcifParseError> {
  let Some(category) = StructAsym::from_document(document) else {
    return Ok(());
  };
  for row in category.rows() {
    input.add_label_chain_entity(
      required(row.row_number(), "_struct_asym.id", row.id())?.to_owned(),
      required(row.row_number(), "_struct_asym.entity_id", row.entity_id())?.to_owned(),
    );
  }
  Ok(())
}

/// Adds polymer types declared by `_entity_poly`.
fn parse_entity_poly(document: &CifDocument, input: &mut StructureInput) -> Result<(), MmcifParseError> {
  let Some(category) = EntityPoly::from_document(document) else {
    return Ok(());
  };
  for row in category.rows() {
    input.add_polymer_entity(PolymerEntity {
      id: required(row.row_number(), "_entity_poly.entity_id", row.entity_id())?.to_owned(),
      polymer_type: polymer_type_from_cif(required(row.row_number(), "_entity_poly.type", row.type_())?),
      sequence: Vec::new(),
      chain_ids: Vec::new(),
    });
  }
  Ok(())
}

/// Adds complete polymer positions declared by `_entity_poly_seq`.
fn parse_entity_poly_seq(document: &CifDocument, input: &mut StructureInput) -> Result<(), MmcifParseError> {
  let Some(category) = EntityPolySeq::from_document(document) else {
    return Ok(());
  };
  for row in category.rows() {
    let number = required(row.row_number(), "_entity_poly_seq.num", row.num()?)?;
    if number <= 0 {
      return Err(MmcifParseError::InvalidField {
        row: row.row_number(),
        field: "_entity_poly_seq.num",
        value: number.to_string(),
      });
    }
    input.add_polymer_sequence(
      required(row.row_number(), "_entity_poly_seq.entity_id", row.entity_id())?.to_owned(),
      PolymerSequenceResidue {
        number,
        monomer: required(row.row_number(), "_entity_poly_seq.mon_id", row.mon_id())?.to_owned(),
        hetero: row
          .hetero()
          .is_some_and(|value| value.eq_ignore_ascii_case("y") || value.eq_ignore_ascii_case("yes")),
      },
    );
  }
  Ok(())
}

/// Converts a dictionary polymer type into the shared model enum.
fn polymer_type_from_cif(value: &str) -> PolymerType {
  let normalized = value.to_ascii_lowercase();
  if normalized.starts_with("polypeptide") {
    PolymerType::Polypeptide
  } else if normalized.starts_with("polydeoxyribonucleotide") {
    PolymerType::Polydeoxyribonucleotide
  } else if normalized.starts_with("polyribonucleotide") {
    PolymerType::Polyribonucleotide
  } else {
    PolymerType::Other(value.to_owned())
  }
}
