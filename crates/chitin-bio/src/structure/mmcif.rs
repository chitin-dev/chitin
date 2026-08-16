//! mmCIF parsing and projection into Chitin's shared structure model.
//!
//! [`cif`] parses syntax without biological assumptions. The private category
//! layer compiles dictionary item names into typed borrowed rows, while the
//! category modules project only model-relevant values into `StructureBuilder`.

use std::io::Read;

mod categories;
mod category;
pub mod cif;
mod schema;

use super::error::{MmcifParseError, MmcifParseResult};
use super::pdb::StructureBuilder;
use cif::CifParser;

/// Reads an mmCIF document into the shared structure model.
#[derive(Debug, Clone, Copy, Default)]
pub struct MmcifParser;

impl MmcifParser {
  /// Creates an mmCIF parser.
  pub const fn new() -> Self {
    Self
  }

  /// Parses an mmCIF byte slice and projects supported biological categories.
  ///
  /// # Parameters
  ///
  /// * `bytes` is the complete UTF-8 mmCIF document.
  ///
  /// # Returns
  ///
  /// A structure snapshot and diagnostics collected by the shared builder.
  /// `_atom_site` is required; polymer entities, connectivity, and secondary
  /// structure are projected when their categories are present.
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
    let mut builder = StructureBuilder::default();

    // Entity declarations must precede atom projection so label-chain and
    // complete-sequence metadata are available while topology is constructed.
    categories::entity::parse(&document, &mut builder)?;
    categories::atom_site::parse(&document, &mut builder)?;
    categories::connectivity::parse(&document, &mut builder)?;
    categories::secondary::parse(&document, &mut builder)?;

    builder.finish(false).map_err(categories::map_builder_error)
  }

  /// Reads all bytes from a reader and parses the mmCIF document.
  ///
  /// # Parameters
  ///
  /// * `reader` supplies the complete UTF-8 mmCIF document.
  ///
  /// # Returns
  ///
  /// The parsed structure or a token, field, structure, or I/O error.
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

#[cfg(test)]
mod tests {
  use super::*;
  use crate::structure::SecondaryStructure;

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

  #[test]
  fn records_complete_entity_sequence_and_missing_positions() {
    let input = br#"data_demo
loop_
_struct_asym.id
_struct_asym.entity_id
A 1
loop_
_entity_poly.entity_id
_entity_poly.type
1 polypeptide(L)
loop_
_entity_poly_seq.entity_id
_entity_poly_seq.num
_entity_poly_seq.mon_id
_entity_poly_seq.hetero
1 1 ALA n
1 2 GLY n
1 3 SER n
loop_
_atom_site.group_PDB
_atom_site.id
_atom_site.auth_atom_id
_atom_site.auth_comp_id
_atom_site.auth_asym_id
_atom_site.auth_seq_id
_atom_site.label_atom_id
_atom_site.label_comp_id
_atom_site.label_asym_id
_atom_site.label_seq_id
_atom_site.label_entity_id
_atom_site.Cartn_x
_atom_site.Cartn_y
_atom_site.Cartn_z
ATOM 1 CA ALA X 10 CA ALA A 1 1 1.0 2.0 3.0
ATOM 2 CA SER X 12 CA SER A 3 1 4.0 5.0 6.0
"#;
    let parsed = MmcifParser::new()
      .parse_bytes(input)
      .unwrap_or_else(|error| panic!("entity sequence fixture should parse: {error}"));
    assert_eq!(parsed.structure.polymer_entities.len(), 1);
    assert_eq!(parsed.structure.polymer_entities[0].sequence.len(), 3);
    assert_eq!(parsed.structure.missing_polymer_residues.len(), 1);
    assert_eq!(parsed.structure.missing_polymer_residues[0].sequence_number, 2);
    assert_eq!(parsed.structure.chains[0].entity_id.as_deref(), Some("1"));
  }

  #[test]
  fn parses_struct_conn_with_auth_and_label_identifiers() {
    let input = br#"data_demo
loop_
_atom_site.group_PDB
_atom_site.id
_atom_site.auth_atom_id
_atom_site.auth_comp_id
_atom_site.auth_asym_id
_atom_site.auth_seq_id
_atom_site.label_atom_id
_atom_site.label_comp_id
_atom_site.label_asym_id
_atom_site.label_seq_id
_atom_site.Cartn_x
_atom_site.Cartn_y
_atom_site.Cartn_z
_atom_site.type_symbol
ATOM 1 CA ALA A 10 CA ALA A 1 1.0 2.0 3.0 C
ATOM 2 O GLY A 11 O GLY A 2 4.0 5.0 6.0 O
loop_
_struct_conn.conn_type_id
_struct_conn.ptnr1_auth_asym_id
_struct_conn.ptnr1_auth_seq_id
_struct_conn.ptnr1_auth_atom_id
_struct_conn.ptnr1_label_asym_id
_struct_conn.ptnr1_label_seq_id
_struct_conn.ptnr1_label_atom_id
_struct_conn.ptnr2_auth_asym_id
_struct_conn.ptnr2_auth_seq_id
_struct_conn.ptnr2_auth_atom_id
_struct_conn.ptnr2_label_asym_id
_struct_conn.ptnr2_label_seq_id
_struct_conn.ptnr2_label_atom_id
covale A 10 CA A invalid CA A 11 O A invalid O
covale A 10 ? A invalid CA A 11 ? A invalid O
loop_
_struct_conf.conf_type_id
_struct_conf.beg_auth_asym_id
_struct_conf.beg_auth_seq_id
_struct_conf.end_auth_asym_id
_struct_conf.end_auth_seq_id
_struct_conf.beg_label_asym_id
_struct_conf.beg_label_seq_id
_struct_conf.end_label_asym_id
_struct_conf.end_label_seq_id
HELX_RH_3T_P . . . . A 1 A 2
loop_
_struct_sheet_range.sheet_id
_struct_sheet_range.id
_struct_sheet_range.beg_label_asym_id
_struct_sheet_range.beg_label_seq_id
_struct_sheet_range.pdbx_beg_PDB_ins_code
_struct_sheet_range.end_label_asym_id
_struct_sheet_range.end_label_seq_id
_struct_sheet_range.pdbx_end_PDB_ins_code
sheet1 1 A 2 . A 2 .
"#;
    let parsed = MmcifParser::new()
      .parse_bytes(input)
      .unwrap_or_else(|error| panic!("struct_conn fixture should parse: {error}"));
    assert_eq!(parsed.structure.bonds.len(), 1);
    assert_eq!(parsed.structure.secondary_ranges.len(), 2);
    assert_eq!(
      parsed.structure.residues[0].secondary_structure,
      SecondaryStructure::Helix310
    );
    assert_eq!(
      parsed.structure.residues[1].secondary_structure,
      SecondaryStructure::Sheet
    );
    assert_eq!(parsed.structure.secondary_ranges[1].kind, SecondaryStructure::Sheet);
  }
}
