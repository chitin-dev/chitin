//! mmCIF parsing and projection into Chitin's shared structure model.
//!
//! [`cif`] parses syntax without biological assumptions. The private category
//! layer compiles dictionary item names into typed borrowed rows, while the
//! category modules project only model-relevant values into the format-neutral
//! [`StructureInput`](super::projection::StructureInput).

use std::io::Read;

mod categories;
mod category;
pub mod cif;
mod schema;

use super::builder::build_structure;
use super::error::{MmcifParseError, StructureParseResult};
use super::projection::StructureInput;
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
  pub fn parse_bytes(&self, bytes: &[u8]) -> Result<StructureParseResult, MmcifParseError> {
    let input = std::str::from_utf8(bytes).map_err(|_| MmcifParseError::InvalidUtf8)?;
    let document = CifParser::parse(input).map_err(|error| MmcifParseError::InvalidToken {
      line: error.line,
      message: error.message,
    })?;
    let mut input = StructureInput::default();

    // Entity declarations must precede atom projection so label-chain and
    // complete-sequence metadata are available while topology is constructed.
    categories::metadata::parse(&document, &mut input)?;
    categories::assembly::parse(&document, &mut input)?;
    categories::entity::parse(&document, &mut input)?;
    categories::atom_site::parse(&document, &mut input)?;
    categories::connectivity::parse(&document, &mut input)?;
    categories::secondary::parse(&document, &mut input)?;

    build_structure(input, false).map_err(|error| MmcifParseError::InvalidStructure {
      line: error.line,
      message: error.message,
    })
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
  pub fn parse_reader<R>(&self, mut reader: R) -> Result<StructureParseResult, MmcifParseError>
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
  use crate::structure::{BondSource, SecondaryStructure};

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
  fn groups_interleaved_atom_site_rows_by_model_number() {
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
_atom_site.pdbx_PDB_model_num
ATOM 1 CA ALA A 1 1.0 2.0 3.0 C 1
ATOM 1 CA ALA A 1 4.0 5.0 6.0 C 2
ATOM 2 C ALA A 1 7.0 8.0 9.0 C 1
"#;
    let parsed = MmcifParser::new()
      .parse_bytes(input)
      .unwrap_or_else(|error| panic!("interleaved model fixture should parse: {error}"));

    assert_eq!(parsed.structure.models.len(), 2);
    assert_eq!(parsed.structure.models[0].source_number, 1);
    assert_eq!(parsed.structure.models[1].source_number, 2);
    assert_eq!(
      parsed.structure.coordinates[parsed.structure.models[0].coordinate_set_id.index()].positions,
      vec![[1.0, 2.0, 3.0], [7.0, 8.0, 9.0]]
    );
    assert_eq!(
      parsed.structure.coordinates[parsed.structure.models[1].coordinate_set_id.index()].positions[0],
      [4.0, 5.0, 6.0]
    );
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
  fn projects_scalar_cell_and_symmetry_items() {
    let input = br#"data_demo
_cell.length_a 10.0
_cell.length_b 20.0
_cell.length_c 30.0
_cell.angle_alpha 90.0
_cell.angle_beta 91.0
_cell.angle_gamma 92.0
_symmetry.space_group_name_H-M 'P 21 21 21'
_symmetry.Int_Tables_number 19
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
_atom_site.type_symbol
ATOM 1 CA ALA A 1 1.0 2.0 3.0 C
"#;
    let parsed = MmcifParser::new()
      .parse_bytes(input)
      .unwrap_or_else(|error| panic!("scalar metadata fixture should parse: {error}"));
    assert_eq!(
      parsed.structure.metadata.unit_cell.as_ref().map(|cell| cell.lengths),
      Some([10.0, 20.0, 30.0])
    );
    assert_eq!(
      parsed
        .structure
        .metadata
        .symmetry
        .as_ref()
        .and_then(|symmetry| symmetry.international_tables_number),
      Some(19)
    );
  }

  #[test]
  fn preserves_assembly_operations_without_expanding_coordinates() {
    let input = br#"data_demo
loop_
_pdbx_struct_oper_list.id
_pdbx_struct_oper_list.matrix[1][1]
_pdbx_struct_oper_list.matrix[1][2]
_pdbx_struct_oper_list.matrix[1][3]
_pdbx_struct_oper_list.matrix[2][1]
_pdbx_struct_oper_list.matrix[2][2]
_pdbx_struct_oper_list.matrix[2][3]
_pdbx_struct_oper_list.matrix[3][1]
_pdbx_struct_oper_list.matrix[3][2]
_pdbx_struct_oper_list.matrix[3][3]
_pdbx_struct_oper_list.vector[1]
_pdbx_struct_oper_list.vector[2]
_pdbx_struct_oper_list.vector[3]
1 1 0 0 0 1 0 0 0 1 2 3 4
loop_
_pdbx_struct_assembly.id
_pdbx_struct_assembly.details
1 'biological unit'
loop_
_pdbx_struct_assembly_gen.assembly_id
_pdbx_struct_assembly_gen.asym_id_list
_pdbx_struct_assembly_gen.auth_asym_id_list
_pdbx_struct_assembly_gen.oper_expression
1 A A 1
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
_atom_site.type_symbol
ATOM 1 CA ALA A 1 1.0 2.0 3.0 C
"#;
    let parsed = MmcifParser::new()
      .parse_bytes(input)
      .unwrap_or_else(|error| panic!("assembly fixture should parse: {error}"));
    assert_eq!(parsed.structure.metadata.assembly.operations.len(), 1);
    assert_eq!(parsed.structure.metadata.assembly.assemblies.len(), 1);
    assert_eq!(parsed.structure.metadata.assembly.assemblies[0].generations.len(), 1);
    assert_eq!(parsed.structure.atom_count(), 1);
    assert_eq!(
      parsed.structure.metadata.assembly.operations[0].translation,
      [2.0, 3.0, 4.0]
    );
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
    assert_eq!(parsed.structure.bonds[0].source, BondSource::StructConn);
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

  #[test]
  fn classifies_metal_coordination_struct_conn() {
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
ATOM 1 O HOH A 1 O HOH A 1 0.0 0.0 0.0 O
HETATM 2 MG MG A 2 MG MG B 1 2.1 0.0 0.0 MG
loop_
_struct_conn.conn_type_id
_struct_conn.ptnr1_auth_asym_id
_struct_conn.ptnr1_auth_seq_id
_struct_conn.ptnr1_auth_atom_id
_struct_conn.ptnr2_auth_asym_id
_struct_conn.ptnr2_auth_seq_id
_struct_conn.ptnr2_auth_atom_id
metalc A 1 O A 2 MG
"#;
    let parsed = MmcifParser::new()
      .parse_bytes(input)
      .unwrap_or_else(|error| panic!("metal-coordination fixture should parse: {error}"));

    assert_eq!(
      parsed.structure.bonds[0].source,
      BondSource::StructConnMetalCoordination
    );
  }

  #[test]
  fn classifies_non_covalent_struct_conn_types() {
    let expected = [
      ("hydrog", BondSource::StructConnHydrogenBond),
      ("saltbr", BondSource::StructConnSaltBridge),
      ("disulf", BondSource::StructConnDisulfide),
      ("mismat", BondSource::StructConnBaseMismatch),
      ("covale_base", BondSource::StructConnCovalentBase),
      ("covale_phosphate", BondSource::StructConnCovalentPhosphate),
      ("covale_sugar", BondSource::StructConnCovalentSugar),
      ("modres", BondSource::StructConnResidueModification),
    ];
    for (connection_type, expected_source) in expected {
      let input = format!(
        r#"data_demo
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
ATOM 1 N ARG A 1 N ARG A 1 0.0 0.0 0.0 N
ATOM 2 O GLU A 2 O GLU A 2 3.0 0.0 0.0 O
loop_
_struct_conn.conn_type_id
_struct_conn.ptnr1_auth_asym_id
_struct_conn.ptnr1_auth_seq_id
_struct_conn.ptnr1_auth_atom_id
_struct_conn.ptnr2_auth_asym_id
_struct_conn.ptnr2_auth_seq_id
_struct_conn.ptnr2_auth_atom_id
{connection_type} A 1 N A 2 O
"#
      );
      let parsed = MmcifParser::new()
        .parse_bytes(input.as_bytes())
        .unwrap_or_else(|error| panic!("special struct-conn fixture should parse: {error}"));
      assert_eq!(parsed.structure.bonds[0].source, expected_source);
    }
  }

  #[test]
  fn author_atom_lookup_should_not_be_overwritten_by_a_label_alias() {
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
ATOM 1 OD1 ASP G 87 OD1 ASP A 87 0.0 0.0 0.0 O
ATOM 2 OD1 ASP F 87 OD1 ASP G 87 40.0 0.0 0.0 O
HETATM 3 MG MG G 601 MG MG O . 2.1 0.0 0.0 MG
loop_
_struct_conn.conn_type_id
_struct_conn.ptnr1_auth_asym_id
_struct_conn.ptnr1_auth_seq_id
_struct_conn.ptnr1_auth_atom_id
_struct_conn.ptnr2_auth_asym_id
_struct_conn.ptnr2_auth_seq_id
_struct_conn.ptnr2_auth_atom_id
metalc G 87 OD1 G 601 MG
"#;
    let parsed = MmcifParser::new()
      .parse_bytes(input)
      .unwrap_or_else(|error| panic!("lookup collision fixture should parse: {error}"));

    assert_eq!(parsed.structure.bonds[0].a.index(), 0);
    assert_eq!(parsed.structure.bonds[0].b.index(), 2);
  }

  #[test]
  fn label_atom_lookup_should_not_be_overwritten_by_a_later_author_alias() {
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
ATOM 1 OD1 ASP G 87 OD1 ASP A 87 0.0 0.0 0.0 O
ATOM 2 OD1 ASP F 87 OD1 ASP G 87 40.0 0.0 0.0 O
loop_
_struct_conn.conn_type_id
_struct_conn.ptnr1_label_asym_id
_struct_conn.ptnr1_label_seq_id
_struct_conn.ptnr1_label_atom_id
_struct_conn.ptnr2_label_asym_id
_struct_conn.ptnr2_label_seq_id
_struct_conn.ptnr2_label_atom_id
metalc G 87 OD1 A 87 OD1
"#;
    let parsed = MmcifParser::new()
      .parse_bytes(input)
      .unwrap_or_else(|error| panic!("label lookup collision fixture should parse: {error}"));

    assert_eq!(parsed.structure.bonds[0].a.index(), 0);
    assert_eq!(parsed.structure.bonds[0].b.index(), 1);
  }
}
