//! PDB fixed-column parsing and semantic projection.
//!
//! [`records`] converts source lines into PDB-specific typed records, while
//! [`projection`] resolves cross-record semantics into the format-neutral
//! structure input consumed by the shared builder.

use std::io::Read;

mod fields;
mod projection;
mod records;

use super::builder::build_structure;
use super::error::{PdbParseError, StructureParseResult};
use records::{PdbDocument, parse_record};

/// Reads fixed-column PDB records into an indexed structure snapshot.
#[derive(Debug, Clone, Copy, Default)]
pub struct PdbParser {
  strict: bool,
}

impl PdbParser {
  /// Creates a parser that preserves recoverable issues as diagnostics.
  ///
  /// # Examples
  ///
  /// ```
  /// use chitin_bio::structure::PdbParser;
  ///
  /// let result = PdbParser::new().parse_bytes(
  ///   b"ATOM      1  CA  ALA A   1       1.000   2.000   3.000  1.00 20.00           C  \n",
  /// )?;
  /// assert_eq!(result.structure.atom_count(), 1);
  /// # Ok::<(), Box<dyn std::error::Error>>(())
  /// ```
  pub const fn new() -> Self {
    Self { strict: false }
  }

  /// Enables strict handling of records that would otherwise become warnings.
  pub const fn strict(self, strict: bool) -> Self {
    Self { strict }
  }

  /// Parses a complete PDB byte slice.
  ///
  /// # Parameters
  ///
  /// * `bytes` is the complete fixed-column PDB input.
  ///
  /// # Returns
  ///
  /// A structure snapshot and recoverable diagnostics. Fatal malformed fields
  /// return [`PdbParseError`] instead.
  ///
  /// # Examples
  ///
  /// ```
  /// use chitin_bio::structure::PdbParser;
  ///
  /// let pdb = b"ATOM      1  N   GLY A   1       0.000   0.000   0.000  1.00 10.00           N  \n";
  /// let parsed = PdbParser::new().parse_bytes(pdb)?;
  /// assert_eq!(parsed.structure.atoms[0].name, "N");
  /// # Ok::<(), Box<dyn std::error::Error>>(())
  /// ```
  pub fn parse_bytes(&self, bytes: &[u8]) -> Result<StructureParseResult, PdbParseError> {
    let mut document = PdbDocument::default();

    for (line_index, raw_line) in bytes.split(|byte| *byte == b'\n').enumerate() {
      let line_number = line_index + 1;
      let raw_line = raw_line.strip_suffix(b"\r").unwrap_or(raw_line);
      let line = std::str::from_utf8(raw_line).map_err(|_| PdbParseError::InvalidUtf8 { line: line_number })?;
      document.records.push(parse_record(line, line_number)?);
    }

    let input = projection::project(document, self.strict)?;
    build_structure(input, self.strict).map_err(|error| PdbParseError::InvalidStructure {
      line: error.line,
      message: error.message,
    })
  }

  /// Reads all bytes from a reader and parses them as a PDB stream.
  ///
  /// # Parameters
  ///
  /// * `reader` supplies the complete PDB stream.
  ///
  /// # Returns
  ///
  /// The parsed structure and recoverable diagnostics, or an I/O/format error.
  ///
  /// # Examples
  ///
  /// ```
  /// use std::io::Cursor;
  /// use chitin_bio::structure::PdbParser;
  ///
  /// let input = Cursor::new(b"END\n");
  /// let parsed = PdbParser::new().parse_reader(input)?;
  /// assert!(parsed.structure.models.is_empty());
  /// # Ok::<(), Box<dyn std::error::Error>>(())
  /// ```
  pub fn parse_reader<R>(&self, mut reader: R) -> Result<StructureParseResult, PdbParseError>
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

  const ATOM_1: &str = "ATOM      1  N   GLY A   1       0.000   0.000   0.000  1.00 10.00           N  ";
  const ATOM_2: &str = "ATOM      2  CA  GLY A   1       1.000   2.000   3.000  1.00 11.00           C  ";
  const ATOM_3: &str = "ATOM      3  N   GLY A   2       2.000   3.000   4.000  1.00 12.00           N  ";

  fn fixed_record(record: &str, fields: &[(usize, &str)]) -> String {
    let mut line = vec![b' '; 80];
    line[..record.len()].copy_from_slice(record.as_bytes());
    for &(start, value) in fields {
      line[start..start + value.len()].copy_from_slice(value.as_bytes());
    }
    String::from_utf8(line).unwrap_or_else(|error| panic!("test record should be ASCII: {error}"))
  }

  #[test]
  fn parses_atom_fields_and_indexes_coordinates() {
    let parsed = PdbParser::new().parse_bytes(format!("{ATOM_1}\n{ATOM_2}\n").as_bytes());
    let parsed = parsed.unwrap_or_else(|error| panic!("fixture should parse: {error}"));
    assert_eq!(parsed.structure.atom_count(), 2);
    assert_eq!(parsed.structure.residues.len(), 1);
    assert_eq!(parsed.structure.coordinates[0].positions[1], [1.0, 2.0, 3.0]);
    assert!(parsed.structure.validate_invariants().is_ok());
  }

  #[test]
  fn shares_atom_topology_across_models() {
    let input = format!("MODEL        1\n{ATOM_1}\nENDMDL\nMODEL        2\n{ATOM_1}\nENDMDL\n");
    let parsed = PdbParser::new()
      .parse_bytes(input.as_bytes())
      .unwrap_or_else(|error| panic!("fixture should parse: {error}"));
    assert_eq!(parsed.structure.models.len(), 2);
    assert_eq!(parsed.structure.atoms.len(), 1);
    assert_eq!(parsed.structure.coordinates.len(), 2);
  }

  #[test]
  fn unknown_records_are_diagnostics_in_default_mode() {
    let parsed = PdbParser::new()
      .parse_bytes(b"FOOBAR\n")
      .unwrap_or_else(|error| panic!("unknown records are recoverable: {error}"));
    assert_eq!(parsed.diagnostics[0].code, "PDB_UNKNOWN_RECORD");
  }

  #[test]
  fn strict_mode_rejects_unknown_records() {
    let error = PdbParser::new().strict(true).parse_bytes(b"FOOBAR\n");
    assert!(matches!(error, Err(PdbParseError::InvalidStructure { .. })));
  }

  #[test]
  fn parses_conect_bonds_and_deduplicates_reverse_edges() {
    let input = format!("{ATOM_1}\n{ATOM_2}\nCONECT    1    2\nCONECT    2    1\n");
    let parsed = PdbParser::new()
      .parse_bytes(input.as_bytes())
      .unwrap_or_else(|error| panic!("CONECT fixture should parse: {error}"));
    assert_eq!(parsed.structure.bonds.len(), 1);
    assert_eq!(parsed.structure.bonds[0].a.index(), 0);
    assert_eq!(parsed.structure.bonds[0].b.index(), 1);
  }

  #[test]
  fn parses_helix_and_sheet_ranges_and_marks_residues() {
    let helix = fixed_record("HELIX", &[(19, "A"), (21, "1"), (33, "1"), (38, "1")]);
    let sheet = fixed_record("SHEET", &[(21, "A"), (22, "2"), (32, "A"), (33, "2")]);
    let input = format!("{ATOM_1}\n{ATOM_2}\n{ATOM_3}\n{helix}\n{sheet}\n");
    let parsed = PdbParser::new()
      .parse_bytes(input.as_bytes())
      .unwrap_or_else(|error| panic!("secondary-structure fixture should parse: {error}"));
    assert_eq!(parsed.structure.secondary_ranges.len(), 2);
    assert_eq!(
      parsed.structure.residues[0].secondary_structure,
      SecondaryStructure::Helix
    );
    assert_eq!(
      parsed.structure.residues[1].secondary_structure,
      SecondaryStructure::Sheet
    );
  }

  #[test]
  fn projects_pdb_compnd_seqres_and_cryst1_metadata() {
    let input = concat!(
      "HEADER    TEST STRUCTURE                              1ABC\n",
      "COMPND    1 MOL_ID: 1;\n",
      "COMPND    2 MOLECULE: TEST PROTEIN;\n",
      "COMPND    3 CHAIN: A;\n",
      "SEQRES   1 A    3  GLY ALA SER\n",
      "CRYST1   10.000   11.000   12.000  90.00  91.00  92.00 P 1           1\n",
      "ATOM      1  CA  GLY A   1       1.000   2.000   3.000  1.00 20.00           C  \n",
    );
    let parsed = PdbParser::new()
      .parse_bytes(input.as_bytes())
      .unwrap_or_else(|error| panic!("PDB metadata fixture should parse: {error}"));
    assert_eq!(parsed.structure.polymer_entities.len(), 1);
    assert_eq!(parsed.structure.polymer_entities[0].sequence.len(), 3);
    assert_eq!(parsed.structure.chains[0].entity_id.as_deref(), Some("1"));
    assert_eq!(
      parsed.structure.metadata.unit_cell.as_ref().map(|cell| cell.lengths),
      Some([10.0, 11.0, 12.0])
    );
    assert_eq!(
      parsed
        .structure
        .metadata
        .symmetry
        .as_ref()
        .and_then(|symmetry| symmetry.space_group_name.as_deref()),
      Some("P 1")
    );
  }

  #[test]
  fn projects_pdb_remark_350_assembly_without_expanding_atoms() {
    let input = concat!(
      "REMARK 350 BIOMOLECULE: 1\n",
      "REMARK 350 APPLY THE FOLLOWING TO CHAINS: A,\n",
      "REMARK 350                    AND: B\n",
      "REMARK 350   BIOMT1   1  1.000000  0.000000  0.000000        0.00000\n",
      "REMARK 350   BIOMT2   1  0.000000  1.000000  0.000000        0.00000\n",
      "REMARK 350   BIOMT3   1  0.000000  0.000000  1.000000        0.00000\n",
      "ATOM      1  CA  GLY A   1       1.000   2.000   3.000  1.00 20.00           C  \n",
    );
    let parsed = PdbParser::new()
      .parse_bytes(input.as_bytes())
      .unwrap_or_else(|error| panic!("PDB assembly fixture should parse: {error}"));
    assert_eq!(parsed.structure.metadata.assembly.operations.len(), 1);
    assert_eq!(parsed.structure.metadata.assembly.assemblies.len(), 1);
    assert_eq!(
      parsed.structure.metadata.assembly.assemblies[0].generations[0].asym_ids,
      ["A", "B"]
    );
  }
}
