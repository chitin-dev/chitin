//! Crystallographic unit-cell and space-group metadata projection.

use crate::structure::mmcif::cif::CifDocument;
use crate::structure::mmcif::schema::{Cell, Symmetry as SymmetryCategory};
use crate::structure::pdb::StructureBuilder;
use crate::structure::{MmcifParseError, Symmetry, UnitCell};

use super::required;

/// Projects crystallographic metadata from the generic CIF document into the
/// shared structure builder.
///
/// The two categories are optional in an mmCIF structure. When present,
/// `_cell` is validated as a complete six-parameter unit cell before it is
/// stored, while `_symmetry` preserves whichever space-group identifiers are
/// available. This projection intentionally records metadata only; it does
/// not expand symmetry mates or transform fractional coordinates.
///
/// # Parameters
///
/// * `document` contains the syntax-level data blocks and category values.
/// * `builder` receives validated unit-cell and space-group metadata.
///
/// # Returns
///
/// `Ok(())` when absent metadata is skipped or all present values are valid.
/// Returns [`MmcifParseError::InvalidField`] for malformed numeric values or
/// geometrically invalid unit-cell parameters.
pub(crate) fn parse(document: &CifDocument, builder: &mut StructureBuilder) -> Result<(), MmcifParseError> {
  parse_unit_cell(document, builder)?;
  parse_symmetry(document, builder)
}

/// Reads and validates the six scalar values that define a unit cell.
///
/// A cell category with no rows is treated as absent. If a row exists, all
/// three edge lengths and all three angles are required: accepting a partial
/// cell would make later fractional-to-Cartesian conversion ambiguous.
///
/// # Parameters
///
/// * `document` contains the generic `_cell` category.
/// * `builder` receives the validated [`UnitCell`].
///
/// # Returns
///
/// `Ok(())` when no cell is declared or when a complete valid cell is stored.
/// Returns a field error when a required value is missing, cannot be parsed as
/// a finite number, or falls outside the valid geometric domain.
fn parse_unit_cell(document: &CifDocument, builder: &mut StructureBuilder) -> Result<(), MmcifParseError> {
  let Some(category) = Cell::from_document(document) else {
    return Ok(());
  };
  let Some(row) = category.rows().next() else {
    return Ok(());
  };
  let lengths = [
    required(row.row_number(), "_cell.length_a", row.length_a()?)?,
    required(row.row_number(), "_cell.length_b", row.length_b()?)?,
    required(row.row_number(), "_cell.length_c", row.length_c()?)?,
  ];
  let angles = [
    required(row.row_number(), "_cell.angle_alpha", row.angle_alpha()?)?,
    required(row.row_number(), "_cell.angle_beta", row.angle_beta()?)?,
    required(row.row_number(), "_cell.angle_gamma", row.angle_gamma()?)?,
  ];
  validate_cell_parameters(row.row_number(), lengths, angles)?;
  builder.set_unit_cell(UnitCell { lengths, angles });
  Ok(())
}

/// Validates the geometric domain of unit-cell lengths and angles.
///
/// Edge lengths must be strictly positive. Each angle must be greater than
/// zero and less than 180 degrees; these bounds exclude degenerate cells while
/// still allowing every conventional crystallographic cell system.
///
/// # Parameters
///
/// * `row` identifies the source category row for error reporting.
/// * `lengths` contains (a), (b), and (c) in ångströms.
/// * `angles` contains α, β, and γ in degrees.
///
/// # Returns
///
/// `Ok(())` when all six values are within their geometric domains. Otherwise
/// returns [`MmcifParseError::InvalidField`] naming the first invalid value.
fn validate_cell_parameters(row: usize, lengths: [f32; 3], angles: [f32; 3]) -> Result<(), MmcifParseError> {
  for (field, value) in [
    ("_cell.length_a", lengths[0]),
    ("_cell.length_b", lengths[1]),
    ("_cell.length_c", lengths[2]),
  ] {
    if value <= 0.0 {
      return Err(MmcifParseError::InvalidField {
        row,
        field,
        value: value.to_string(),
      });
    }
  }
  for (field, value) in [
    ("_cell.angle_alpha", angles[0]),
    ("_cell.angle_beta", angles[1]),
    ("_cell.angle_gamma", angles[2]),
  ] {
    if !(0.0..180.0).contains(&value) {
      return Err(MmcifParseError::InvalidField {
        row,
        field,
        value: value.to_string(),
      });
    }
  }
  Ok(())
}

/// Reads optional space-group identifiers from the `_symmetry` category.
///
/// The Hermann–Mauguin name and International Tables number are independent
/// dictionary items, so either one may be present without the other. Missing
/// values are preserved as `None`; the builder is updated only when at least
/// one identifier is available.
///
/// # Parameters
///
/// * `document` contains the generic `_symmetry` category.
/// * `builder` receives the identifiers that were declared by the source.
///
/// # Returns
///
/// `Ok(())` when the category is absent or its values are valid. Returns a
/// field conversion error if the International Tables number is malformed.
fn parse_symmetry(document: &CifDocument, builder: &mut StructureBuilder) -> Result<(), MmcifParseError> {
  let Some(category) = SymmetryCategory::from_document(document) else {
    return Ok(());
  };
  let Some(row) = category.rows().next() else {
    return Ok(());
  };
  let space_group_name = row.space_group_name_h_m();
  let international_tables_number = row.int_tables_number()?;
  if space_group_name.is_some() || international_tables_number.is_some() {
    builder.set_symmetry(Symmetry {
      space_group_name: space_group_name.map(str::to_owned),
      international_tables_number,
    });
  }
  Ok(())
}
