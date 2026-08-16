//! Typed mmCIF categories and their projection into the structure model.

pub(super) mod atom_site;
pub(super) mod connectivity;
pub(super) mod entity;
pub(super) mod secondary;

use crate::structure::{MmcifParseError, PdbParseError};

/// Converts a shared builder failure into the mmCIF error namespace.
pub(super) fn map_builder_error(error: PdbParseError) -> MmcifParseError {
  MmcifParseError::InvalidStructure(error.to_string())
}

/// Returns a required generated schema value.
pub(super) fn required<T>(row: usize, field: &'static str, value: Option<T>) -> Result<T, MmcifParseError> {
  value.ok_or_else(|| MmcifParseError::InvalidField {
    row,
    field,
    value: String::new(),
  })
}

/// Parses an optional integer stored under a dictionary text type.
pub(super) fn optional_text_i32(
  row: usize,
  field: &'static str,
  value: Option<&str>,
) -> Result<Option<i32>, MmcifParseError> {
  value
    .map(|value| {
      value.parse().map_err(|_| MmcifParseError::InvalidField {
        row,
        field,
        value: value.to_owned(),
      })
    })
    .transpose()
}
