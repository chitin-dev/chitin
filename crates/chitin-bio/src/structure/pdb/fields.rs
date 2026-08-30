//! Fixed-column and scalar conversion helpers for PDB records.

use crate::structure::error::PdbParseError;

/// Returns a bounded fixed-column field without panicking on short records.
pub(crate) fn field(line: &str, start: usize, end: usize) -> &str {
  let bytes = line.as_bytes();
  let start = start.min(bytes.len());
  let end = end.min(bytes.len()).max(start);
  line.get(start..end).unwrap_or_default()
}

/// Parses an optional signed integer field.
pub(crate) fn parse_optional_i32(
  line: &str,
  start: usize,
  end: usize,
  field_name: &'static str,
  line_number: usize,
) -> Result<Option<i32>, PdbParseError> {
  let value = field(line, start, end).trim();
  if value.is_empty() {
    return Ok(None);
  }
  value.parse().map(Some).map_err(|_| PdbParseError::InvalidField {
    line: line_number,
    field: field_name,
    value: value.to_owned(),
  })
}

/// Parses a required signed integer field.
pub(crate) fn parse_required_i32(
  line: &str,
  start: usize,
  end: usize,
  field_name: &'static str,
  line_number: usize,
) -> Result<i32, PdbParseError> {
  field(line, start, end)
    .trim()
    .parse()
    .map_err(|_| PdbParseError::InvalidField {
      line: line_number,
      field: field_name,
      value: field(line, start, end).to_owned(),
    })
}

/// Parses an optional floating-point field.
pub(crate) fn parse_optional_f32(
  line: &str,
  start: usize,
  end: usize,
  field_name: &'static str,
  line_number: usize,
) -> Result<Option<f32>, PdbParseError> {
  let value = field(line, start, end).trim();
  if value.is_empty() {
    return Ok(None);
  }
  value.parse().map(Some).map_err(|_| PdbParseError::InvalidField {
    line: line_number,
    field: field_name,
    value: value.to_owned(),
  })
}

/// Parses a required finite floating-point field.
pub(crate) fn parse_required_f32(
  line: &str,
  start: usize,
  end: usize,
  field_name: &'static str,
  line_number: usize,
) -> Result<f32, PdbParseError> {
  let value =
    parse_optional_f32(line, start, end, field_name, line_number)?.ok_or_else(|| PdbParseError::InvalidField {
      line: line_number,
      field: field_name,
      value: field(line, start, end).to_owned(),
    })?;
  if value.is_finite() {
    Ok(value)
  } else {
    Err(PdbParseError::InvalidField {
      line: line_number,
      field: field_name,
      value: field(line, start, end).to_owned(),
    })
  }
}

/// Extracts a single non-whitespace character from a fixed-column field.
pub(crate) fn single_char(value: &str) -> Option<char> {
  value.trim().chars().next()
}

/// Parses PDB's optional charge notation, such as `1+` or `2-`.
pub(crate) fn parse_charge(value: &str) -> Option<i8> {
  let value = value.trim();
  let sign = value.chars().last()?;
  let magnitude = value[..value.len().saturating_sub(sign.len_utf8())]
    .parse::<i8>()
    .ok()?;
  match sign {
    '+' => Some(magnitude),
    '-' => Some(-magnitude),
    _ => None,
  }
}

/// Converts a whitespace-only field into `None`.
pub(crate) fn non_empty(value: &str) -> Option<String> {
  let value = value.trim();
  (!value.is_empty()).then(|| value.to_owned())
}
