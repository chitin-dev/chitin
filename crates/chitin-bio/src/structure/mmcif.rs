use std::io::Read;

pub mod cif;

use super::error::{MmcifParseError, MmcifParseResult, PdbParseError};
use super::model::{Element, ResidueKind};
use super::pdb::{AtomRecord, StructureBuilder};
use cif::{CifCategory, CifDocument, CifParser, CifValue};

/// Reads the atom-site loop of an mmCIF document into the shared structure model.
#[derive(Debug, Clone, Copy, Default)]
pub struct MmcifParser;

impl MmcifParser {
  /// Creates an mmCIF parser.
  pub const fn new() -> Self {
    Self
  }

  /// Parses an mmCIF byte slice containing an atom-site loop.
  ///
  /// # Parameters
  ///
  /// * `bytes` is the complete UTF-8 mmCIF document.
  ///
  /// # Returns
  ///
  /// A structure snapshot and the diagnostics collected by the shared builder.
  /// The initial implementation requires `_atom_site` coordinates and ignores
  /// unrelated categories.
  ///
  /// # Examples
  ///
  /// ```
  /// use chitin_bio::structure::MmcifParser;
  ///
  /// let cif = br#"data_demo
  /// loop_
  /// _atom_site.group_PDB
  /// _atom_site.id
  /// _atom_site.label_atom_id
  /// _atom_site.label_comp_id
  /// _atom_site.label_asym_id
  /// _atom_site.label_seq_id
  /// _atom_site.Cartn_x
  /// _atom_site.Cartn_y
  /// _atom_site.Cartn_z
  /// _atom_site.type_symbol
  /// ATOM 1 CA ALA A 1 1.0 2.0 3.0 C
  /// "#;
  /// let parsed = MmcifParser::new().parse_bytes(cif)?;
  /// assert_eq!(parsed.structure.atom_count(), 1);
  /// # Ok::<(), Box<dyn std::error::Error>>(())
  /// ```
  pub fn parse_bytes(&self, bytes: &[u8]) -> Result<MmcifParseResult, MmcifParseError> {
    let input = std::str::from_utf8(bytes).map_err(|_| MmcifParseError::InvalidUtf8)?;
    let document = CifParser::parse(input).map_err(|error| MmcifParseError::InvalidToken {
      line: error.line,
      message: error.message,
    })?;
    let table = AtomSiteTable::from_document(&document)?;
    let mut builder = StructureBuilder::default();
    let mut current_model = None;

    for row in &table.rows {
      let model_number = optional_i32(row, "_atom_site.pdbx_PDB_model_num", "model number")?.unwrap_or(1);
      if current_model != Some(model_number) {
        builder.start_model(model_number);
        current_model = Some(model_number);
      }
      let atom = atom_record(row)?;
      builder.add_atom(atom).map_err(map_builder_error)?;
    }

    builder.finish(false).map_err(map_builder_error)
  }

  /// Reads all bytes from a reader and parses the mmCIF atom-site loop.
  ///
  /// # Parameters
  ///
  /// * `reader` supplies the complete UTF-8 mmCIF document.
  ///
  /// # Returns
  ///
  /// The parsed structure or a token, field, or I/O error.
  ///
  /// # Examples
  ///
  /// ```
  /// use std::io::Cursor;
  /// use chitin_bio::structure::MmcifParser;
  ///
  /// let parsed = MmcifParser::new().parse_reader(Cursor::new(b"data_empty\n"));
  /// assert!(parsed.is_err());
  /// ```
  pub fn parse_reader<R>(&self, mut reader: R) -> Result<MmcifParseResult, MmcifParseError>
  where
    R: Read,
  {
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes)?;
    self.parse_bytes(&bytes)
  }
}

/// Converts an atom-site row into the common builder record.
///
/// # Parameters
///
/// * `row` contains one complete atom-site loop row and its column tags.
///
/// # Returns
///
/// A PDB-independent [`AtomRecord`] using author identifiers when present and
/// label identifiers as a fallback.
fn atom_record(row: &AtomSiteRow) -> Result<AtomRecord, MmcifParseError> {
  let group = row.value("_atom_site.group_PDB").unwrap_or("ATOM");
  let name = required_text(row, &["_atom_site.auth_atom_id", "_atom_site.label_atom_id"])?;
  let residue_name = required_text(row, &["_atom_site.auth_comp_id", "_atom_site.label_comp_id"])?;
  let chain_id = optional_text(row, &["_atom_site.auth_asym_id", "_atom_site.label_asym_id"]);
  let sequence_number = required_i32(
    row,
    &["_atom_site.auth_seq_id", "_atom_site.label_seq_id"],
    "residue sequence",
  )?;
  let position = [
    required_f32(row, "_atom_site.Cartn_x")?,
    required_f32(row, "_atom_site.Cartn_y")?,
    required_f32(row, "_atom_site.Cartn_z")?,
  ];

  Ok(AtomRecord {
    line: row.row_number,
    serial: optional_i32(row, "_atom_site.id", "atom-site id")?,
    name: name.to_owned(),
    element: optional_text(row, &["_atom_site.type_symbol"]).map(|value| Element(value.to_owned())),
    chain_id: chain_id.map(str::to_owned),
    residue_name: residue_name.to_owned(),
    sequence_number,
    insertion_code: optional_char(row, "_atom_site.pdbx_PDB_ins_code"),
    altloc: optional_char(row, "_atom_site.label_alt_id"),
    occupancy: optional_f32(row, "_atom_site.occupancy")?,
    b_factor: optional_f32(row, "_atom_site.B_iso_or_equiv")?,
    formal_charge: optional_charge(row, "_atom_site.pdbx_formal_charge"),
    position,
    kind: if group == "HETATM" {
      ResidueKind::Hetero
    } else {
      ResidueKind::Polymer
    },
  })
}

/// Parses an optional integer cell from an atom-site row.
fn optional_i32(row: &AtomSiteRow, tag: &str, field: &'static str) -> Result<Option<i32>, MmcifParseError> {
  let Some(value) = row.value(tag) else {
    return Ok(None);
  };
  if is_missing(value) {
    return Ok(None);
  }
  value.parse().map(Some).map_err(|_| MmcifParseError::InvalidField {
    row: row.row_number,
    field,
    value: value.to_owned(),
  })
}

/// One row of the atom-site loop with its column names.
#[derive(Debug)]
struct AtomSiteRow {
  /// One-based row number within the atom-site loop.
  row_number: usize,
  /// Column tags in loop order.
  tags: Vec<String>,
  /// Values aligned positionally with `tags`, preserving CIF missing markers.
  values: Vec<CifValue>,
}

/// Columnar atom-site data selected from an mmCIF document.
#[derive(Debug)]
struct AtomSiteTable {
  /// Complete rows in source order.
  rows: Vec<AtomSiteRow>,
}

impl AtomSiteRow {
  /// Returns a cell by exact mmCIF tag name.
  fn value(&self, tag: &str) -> Option<&str> {
    self
      .tags
      .iter()
      .position(|candidate| candidate == tag)
      .and_then(|index| self.values.get(index))
      .and_then(CifValue::as_text)
  }
}

impl AtomSiteTable {
  /// Selects the first atom-site loop from a generic CIF document.
  ///
  /// # Parameters
  ///
  /// * `document` is the parsed CIF document whose categories are searched in
  ///   source order.
  ///
  /// # Returns
  ///
  /// A row-oriented atom-site table, or an error when no usable loop exists.
  fn from_document(document: &CifDocument) -> Result<Self, MmcifParseError> {
    for block in &document.blocks {
      for category in &block.categories {
        let CifCategory::Loop { tags, rows } = category else {
          continue;
        };
        if !tags.iter().any(|tag| tag.starts_with("_atom_site.")) {
          continue;
        }
        if rows.is_empty() {
          return Err(MmcifParseError::InvalidToken {
            line: 0,
            message: "atom_site loop has no rows".to_owned(),
          });
        }
        return Ok(Self {
          rows: rows
            .iter()
            .enumerate()
            .map(|(index, row)| AtomSiteRow {
              row_number: index + 1,
              tags: tags.clone(),
              values: row.clone(),
            })
            .collect(),
        });
      }
    }

    Err(MmcifParseError::InvalidToken {
      line: 0,
      message: "missing atom_site loop".to_owned(),
    })
  }
}

fn is_missing(value: &str) -> bool {
  matches!(value, "." | "?")
}

/// Returns the first defined, non-missing value among author/label candidates.
///
/// # Parameters
///
/// * `row` is the atom-site row being read.
/// * `tags` lists preferred tags in priority order.
///
/// # Returns
///
/// The first value that is neither `.` nor `?`, or `None` when all candidates
/// are absent.
fn required_text<'a>(row: &'a AtomSiteRow, tags: &[&str]) -> Result<&'a str, MmcifParseError> {
  optional_text(row, tags).ok_or_else(|| MmcifParseError::InvalidField {
    row: row.row_number,
    field: "_atom_site text field",
    value: String::new(),
  })
}

/// Returns the first defined, non-missing text value among candidate tags.
fn optional_text<'a>(row: &'a AtomSiteRow, tags: &[&str]) -> Option<&'a str> {
  tags
    .iter()
    .find_map(|tag| row.value(tag))
    .filter(|value| !is_missing(value) && !value.trim().is_empty())
}

/// Parses a required integer using author identifiers before label identifiers.
///
/// # Parameters
///
/// * `row` is the atom-site row being read.
/// * `tags` lists equivalent candidate columns in priority order.
/// * `field` names the semantic field in a parse error.
///
/// # Returns
///
/// The parsed integer or an error for a missing/non-numeric value.
fn required_i32(row: &AtomSiteRow, tags: &[&str], field: &'static str) -> Result<i32, MmcifParseError> {
  let value = required_text(row, tags)?;
  value.parse().map_err(|_| MmcifParseError::InvalidField {
    row: row.row_number,
    field,
    value: value.to_owned(),
  })
}

/// Parses an optional floating-point atom-site value.
fn optional_f32(row: &AtomSiteRow, tag: &str) -> Result<Option<f32>, MmcifParseError> {
  let Some(value) = row.value(tag).filter(|value| !is_missing(value)) else {
    return Ok(None);
  };
  value.parse().map(Some).map_err(|_| MmcifParseError::InvalidField {
    row: row.row_number,
    field: "atom-site float",
    value: value.to_owned(),
  })
}

/// Parses and validates a required finite Cartesian coordinate.
///
/// # Parameters
///
/// * `row` is the atom-site row being read.
/// * `tag` identifies one Cartesian coordinate column.
///
/// # Returns
///
/// A finite coordinate value, or an error for missing, malformed, or non-finite
/// input.
///
/// # Examples
///
/// ```text
/// _atom_site.Cartn_x  12.345
/// ```
fn required_f32(row: &AtomSiteRow, tag: &str) -> Result<f32, MmcifParseError> {
  let value = row.value(tag).ok_or_else(|| MmcifParseError::InvalidField {
    row: row.row_number,
    field: "atom-site coordinate",
    value: String::new(),
  })?;
  if is_missing(value) {
    return Err(MmcifParseError::InvalidField {
      row: row.row_number,
      field: "atom-site coordinate",
      value: value.to_owned(),
    });
  }
  let coordinate = value.parse::<f32>().map_err(|_| MmcifParseError::InvalidField {
    row: row.row_number,
    field: "atom-site coordinate",
    value: value.to_owned(),
  })?;
  if coordinate.is_finite() {
    Ok(coordinate)
  } else {
    Err(MmcifParseError::InvalidField {
      row: row.row_number,
      field: "atom-site coordinate",
      value: value.to_owned(),
    })
  }
}

/// Converts a defined one-character mmCIF field into an optional character.
fn optional_char(row: &AtomSiteRow, tag: &str) -> Option<char> {
  optional_text(row, &[tag]).and_then(|value| value.chars().next())
}

/// Parses mmCIF formal charges such as `1+`, `2-`, or signed integers.
///
/// Invalid optional charge fields are ignored because they do not prevent
/// constructing a coordinate-bearing structure snapshot.
///
/// # Examples
///
/// ```text
/// 1+ -> Some(1)
/// 2- -> Some(-2)
/// ```
fn optional_charge(row: &AtomSiteRow, tag: &str) -> Option<i8> {
  let value = optional_text(row, &[tag])?;
  let (magnitude, sign) = value.split_at(value.len().saturating_sub(1));
  let magnitude = magnitude.parse::<i8>().ok()?;
  match sign {
    "+" => Some(magnitude),
    "-" => Some(-magnitude),
    _ => value.parse().ok(),
  }
}

/// Converts a PDB builder error into the mmCIF error namespace.
fn map_builder_error(error: PdbParseError) -> MmcifParseError {
  MmcifParseError::InvalidStructure(error.to_string())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parses_quotes_and_comments() {
    let document =
      CifParser::parse("data_x\n# comment\n_tag 'quoted value'\n").unwrap_or_else(|error| panic!("valid CIF: {error}"));
    assert_eq!(document.blocks[0].categories.len(), 1);
  }

  #[test]
  fn parses_atom_site_rows_into_the_shared_structure() {
    let input = br#"data_demo
loop_
_atom_site.group_PDB
_atom_site.id
_atom_site.label_atom_id
_atom_site.label_comp_id
_atom_site.auth_asym_id
_atom_site.auth_seq_id
_atom_site.Cartn_x
_atom_site.Cartn_y
_atom_site.Cartn_z
_atom_site.type_symbol
ATOM 1 CA ALA A 1 1.0 2.0 3.0 C
"#;
    let parsed = MmcifParser::new()
      .parse_bytes(input)
      .unwrap_or_else(|error| panic!("fixture should parse: {error}"));
    assert_eq!(parsed.structure.atom_count(), 1);
    assert_eq!(parsed.structure.coordinates[0].positions[0], [1.0, 2.0, 3.0]);
  }

  #[test]
  fn uses_label_identifiers_when_author_identifiers_are_missing() {
    let input = br#"data_demo
loop_
_atom_site.group_PDB
_atom_site.id
_atom_site.label_atom_id
_atom_site.label_comp_id
_atom_site.label_asym_id
_atom_site.label_seq_id
_atom_site.Cartn_x
_atom_site.Cartn_y
_atom_site.Cartn_z
ATOM 1 CA ALA B 4 1.0 2.0 3.0
"#;
    let parsed = MmcifParser::new()
      .parse_bytes(input)
      .unwrap_or_else(|error| panic!("fixture should parse: {error}"));
    assert_eq!(parsed.structure.chains[0].auth_id.as_deref(), Some("B"));
    assert_eq!(parsed.structure.residues[0].sequence_number, 4);
  }
}
