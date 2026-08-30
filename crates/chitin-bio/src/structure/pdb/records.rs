//! Typed records produced by fixed-column PDB syntax parsing.
//!
//! These types preserve PDB-specific identities and record semantics. They are
//! consumed only by the sibling projection module and never by the shared
//! structure builder.

use super::fields::{
  field, non_empty, parse_charge, parse_optional_f32, parse_optional_i32, parse_required_f32, parse_required_i32,
  single_char,
};
use crate::structure::error::PdbParseError;
use crate::structure::model::{Element, ResidueKind, SecondaryStructure};

/// A parsed PDB document retaining records in source order.
#[derive(Debug, Default)]
pub(super) struct PdbDocument {
  /// Typed records recognized by the syntax parser.
  pub(super) records: Vec<PdbRecord>,
}

/// One supported or diagnosable PDB record.
#[derive(Debug)]
pub(super) enum PdbRecord {
  /// Fixed-column structure classification and identifier.
  Header {
    /// Optional classification text.
    classification: Option<String>,
    /// Optional four-character structure identifier.
    identifier: Option<String>,
  },
  /// Start of an explicit coordinate model.
  Model(i32),
  /// End of the current coordinate model.
  EndModel,
  /// One ATOM or HETATM coordinate record.
  Atom(PdbAtomRecord),
  /// A TER topology boundary marker.
  Ter,
  /// A serial-number connectivity record.
  Conect(PdbConectRecord),
  /// A HELIX or SHEET annotation.
  Secondary(PdbSecondaryRange),
  /// A PDB-specific semantic record interpreted during projection.
  ProjectionRecord {
    /// One-based source line.
    line_number: usize,
    /// Six-column PDB record name.
    kind: PdbProjectionRecordKind,
    /// Original record text without its line ending.
    line: String,
  },
  /// A known record that currently has no projected model field.
  Ignored,
  /// An unsupported record retained for strict-mode handling.
  Unknown {
    /// One-based source line.
    line: usize,
    /// Trimmed record name.
    name: String,
  },
}

/// PDB records whose meaning requires cross-record projection state.
#[derive(Debug, Clone, Copy)]
pub(super) enum PdbProjectionRecordKind {
  /// Molecular entity declaration.
  Compnd,
  /// Declared polymer sequence.
  Seqres,
  /// Remark-based missing residues or biological assembly metadata.
  Remark,
  /// Crystallographic unit cell and space group.
  Cryst1,
}

/// Fields extracted from one PDB ATOM or HETATM record.
#[derive(Debug)]
pub(super) struct PdbAtomRecord {
  /// One-based source line.
  pub(super) line: usize,
  /// Source atom serial number.
  pub(super) serial: Option<i32>,
  /// PDB atom name.
  pub(super) name: String,
  /// Chemical element when present in columns 77-78.
  pub(super) element: Option<Element>,
  /// Author chain identifier.
  pub(super) chain_id: Option<String>,
  /// Three-character residue name.
  pub(super) residue_name: String,
  /// Author residue sequence number.
  pub(super) sequence_number: i32,
  /// Residue insertion code.
  pub(super) insertion_code: Option<char>,
  /// Alternate conformer identifier.
  pub(super) altloc: Option<char>,
  /// Crystallographic occupancy.
  pub(super) occupancy: Option<f32>,
  /// Isotropic B-factor.
  pub(super) b_factor: Option<f32>,
  /// Formal charge encoded in the PDB suffix columns.
  pub(super) formal_charge: Option<i8>,
  /// Cartesian coordinates in angstroms.
  pub(super) position: [f32; 3],
  /// Whether the source record was ATOM or HETATM.
  pub(super) kind: ResidueKind,
}

/// Serial-number connectivity extracted from one CONECT record.
#[derive(Debug)]
pub(super) struct PdbConectRecord {
  /// One-based source line.
  pub(super) line: usize,
  /// Source endpoint serial.
  pub(super) source: i32,
  /// Target endpoint serials written on the record.
  pub(super) targets: Vec<i32>,
}

/// HELIX or SHEET endpoints expressed in PDB author identifiers.
#[derive(Debug)]
pub(super) struct PdbSecondaryRange {
  /// One-based source line.
  pub(super) line: usize,
  /// Author chain identifier.
  pub(super) chain_id: Option<String>,
  /// First residue sequence number.
  pub(super) start_sequence_number: i32,
  /// First residue insertion code.
  pub(super) start_insertion_code: Option<char>,
  /// Last residue sequence number.
  pub(super) end_sequence_number: i32,
  /// Last residue insertion code.
  pub(super) end_insertion_code: Option<char>,
  /// Secondary-structure classification.
  pub(super) kind: SecondaryStructure,
}

/// Parses one source line into a typed PDB record.
///
/// # Parameters
///
/// * `line` is a fixed-column record without its line ending.
/// * `line_number` is the one-based source location used in errors.
///
/// # Returns
///
/// A typed record containing only PDB syntax-level data, or a
/// [`PdbParseError`] when a required fixed-column value is malformed.
///
/// # Examples
///
/// ```text
/// "MODEL        2" -> PdbRecord::Model(2)
/// "ATOM ..."       -> PdbRecord::Atom(PdbAtomRecord { ... })
/// ```
pub(super) fn parse_record(line: &str, line_number: usize) -> Result<PdbRecord, PdbParseError> {
  let record = field(line, 0, 6).trim();
  match record {
    "" | "END" => Ok(PdbRecord::Ignored),
    "ENDMDL" => Ok(PdbRecord::EndModel),
    "HEADER" => Ok(PdbRecord::Header {
      classification: non_empty(field(line, 10, 50)),
      identifier: non_empty(field(line, 62, 66)),
    }),
    "MODEL" => Ok(PdbRecord::Model(parse_required_i32(
      line,
      10,
      14,
      "model",
      line_number,
    )?)),
    "ATOM" | "HETATM" => Ok(PdbRecord::Atom(parse_atom(line, line_number, record == "HETATM")?)),
    "TER" => Ok(PdbRecord::Ter),
    "CONECT" => Ok(PdbRecord::Conect(parse_conect(line, line_number)?)),
    "HELIX" => Ok(PdbRecord::Secondary(parse_helix(line, line_number)?)),
    "SHEET" => Ok(PdbRecord::Secondary(parse_sheet(line, line_number)?)),
    "COMPND" => Ok(projection_record(PdbProjectionRecordKind::Compnd, line, line_number)),
    "SEQRES" => Ok(projection_record(PdbProjectionRecordKind::Seqres, line, line_number)),
    "REMARK" => Ok(projection_record(PdbProjectionRecordKind::Remark, line, line_number)),
    "CRYST1" => Ok(projection_record(PdbProjectionRecordKind::Cryst1, line, line_number)),
    "ANISOU" | "TITLE" | "SOURCE" | "KEYWDS" | "AUTHOR" | "JRNL" => Ok(PdbRecord::Ignored),
    _ => Ok(PdbRecord::Unknown {
      line: line_number,
      name: record.to_owned(),
    }),
  }
}

/// Wraps a raw semantic record for the projection stage.
fn projection_record(kind: PdbProjectionRecordKind, line: &str, line_number: usize) -> PdbRecord {
  PdbRecord::ProjectionRecord {
    line_number,
    kind,
    line: line.to_owned(),
  }
}

/// Parses one ATOM or HETATM fixed-column record.
fn parse_atom(line: &str, line_number: usize, hetero: bool) -> Result<PdbAtomRecord, PdbParseError> {
  Ok(PdbAtomRecord {
    line: line_number,
    serial: parse_optional_i32(line, 6, 11, "serial", line_number)?,
    name: field(line, 12, 16).trim().to_owned(),
    element: non_empty(field(line, 76, 78)).map(Element),
    chain_id: non_empty(field(line, 21, 22)),
    residue_name: field(line, 17, 20).trim().to_owned(),
    sequence_number: parse_required_i32(line, 22, 26, "residue sequence", line_number)?,
    insertion_code: single_char(field(line, 26, 27)),
    altloc: single_char(field(line, 16, 17)),
    occupancy: parse_optional_f32(line, 54, 60, "occupancy", line_number)?,
    b_factor: parse_optional_f32(line, 60, 66, "B-factor", line_number)?,
    formal_charge: parse_charge(field(line, 78, 80)),
    position: [
      parse_required_f32(line, 30, 38, "x coordinate", line_number)?,
      parse_required_f32(line, 38, 46, "y coordinate", line_number)?,
      parse_required_f32(line, 46, 54, "z coordinate", line_number)?,
    ],
    kind: if hetero {
      ResidueKind::Hetero
    } else {
      ResidueKind::Polymer
    },
  })
}

/// Parses a CONECT record into source and target serial numbers.
fn parse_conect(line: &str, line_number: usize) -> Result<PdbConectRecord, PdbParseError> {
  let source = parse_required_i32(line, 6, 11, "CONECT source serial", line_number)?;
  let targets = [(11, 16), (16, 21), (21, 26), (26, 31)]
    .into_iter()
    .filter_map(|(start, end)| {
      let value = field(line, start, end).trim();
      (!value.is_empty()).then(|| value.parse::<i32>())
    })
    .collect::<Result<Vec<_>, _>>()
    .map_err(|_| PdbParseError::InvalidField {
      line: line_number,
      field: "CONECT target serial",
      value: field(line, 11, 31).to_owned(),
    })?;
  Ok(PdbConectRecord {
    line: line_number,
    source,
    targets,
  })
}

/// Parses a HELIX record into PDB author-identifier endpoints.
fn parse_helix(line: &str, line_number: usize) -> Result<PdbSecondaryRange, PdbParseError> {
  Ok(PdbSecondaryRange {
    line: line_number,
    chain_id: non_empty(field(line, 19, 20)),
    start_sequence_number: parse_required_i32(line, 21, 25, "HELIX start sequence", line_number)?,
    start_insertion_code: single_char(field(line, 25, 26)),
    end_sequence_number: parse_required_i32(line, 33, 37, "HELIX end sequence", line_number)?,
    end_insertion_code: single_char(field(line, 37, 38)),
    kind: SecondaryStructure::Helix,
  })
}

/// Parses a SHEET record into PDB author-identifier endpoints.
fn parse_sheet(line: &str, line_number: usize) -> Result<PdbSecondaryRange, PdbParseError> {
  Ok(PdbSecondaryRange {
    line: line_number,
    chain_id: non_empty(field(line, 21, 22)),
    start_sequence_number: parse_required_i32(line, 22, 26, "SHEET start sequence", line_number)?,
    start_insertion_code: single_char(field(line, 26, 27)),
    end_sequence_number: parse_required_i32(line, 33, 37, "SHEET end sequence", line_number)?,
    end_insertion_code: single_char(field(line, 37, 38)),
    kind: SecondaryStructure::Sheet,
  })
}
