use std::collections::HashMap;
use std::io::Read;

use super::error::{Diagnostic, DiagnosticSeverity, PdbParseError, PdbParseResult};
use super::model::{
  Atom, AtomId, AtomName, Chain, ChainId, CoordinateSet, CoordinateSetId, Element, Model, ModelId, Residue, ResidueId,
  ResidueKind, SecondaryStructure, Structure,
};

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
  pub fn parse_bytes(&self, bytes: &[u8]) -> Result<PdbParseResult, PdbParseError> {
    let mut builder = StructureBuilder::default();

    for (line_index, raw_line) in bytes.split(|byte| *byte == b'\n').enumerate() {
      let line_number = line_index + 1;
      let raw_line = raw_line.strip_suffix(b"\r").unwrap_or(raw_line);
      let line = std::str::from_utf8(raw_line).map_err(|_| PdbParseError::InvalidUtf8 { line: line_number })?;
      self.parse_line(line, line_number, &mut builder)?;
    }

    builder.finish()
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
  pub fn parse_reader<R>(&self, mut reader: R) -> Result<PdbParseResult, PdbParseError>
  where
    R: Read,
  {
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes)?;
    self.parse_bytes(&bytes)
  }

  /// Dispatches one decoded PDB record to the shared structure builder.
  ///
  /// # Parameters
  ///
  /// * `line` is one record without its line ending.
  /// * `line_number` is the one-based source line for diagnostics.
  /// * `builder` receives semantic structure events.
  ///
  /// # Returns
  ///
  /// `Ok(())` when the record was accepted or recorded as a diagnostic.
  fn parse_line(&self, line: &str, line_number: usize, builder: &mut StructureBuilder) -> Result<(), PdbParseError> {
    let record = field(line, 0, 6).trim();
    match record {
      "" | "END" | "ENDMDL" => {
        if record == "ENDMDL" {
          builder.finish_model();
        }
      }
      "HEADER" => builder.set_header(field(line, 10, 50), field(line, 62, 66)),
      "MODEL" => builder.start_model(parse_required_i32(line, 10, 14, "model", line_number)?),
      "ATOM" | "HETATM" => builder.add_atom(parse_atom(line, line_number, record == "HETATM")?)?,
      "TER" => builder.finish_chain(),
      // These records have dedicated fields in the model, but are intentionally
      // deferred until the minimal atom snapshot is stable.
      "ANISOU" | "CONECT" | "HELIX" | "SHEET" | "REMARK" | "TITLE" | "COMPND" | "SOURCE" | "KEYWDS" | "AUTHOR"
      | "JRNL" | "CRYST1" => {}
      _ => self.record_warning(
        builder,
        line_number,
        "PDB_UNKNOWN_RECORD",
        format!("ignored unsupported record {record:?}"),
      )?,
    }

    Ok(())
  }

  /// Converts a recoverable unsupported-record condition into a warning or a
  /// strict parse error.
  fn record_warning(
    &self,
    builder: &mut StructureBuilder,
    line: usize,
    code: &'static str,
    message: String,
  ) -> Result<(), PdbParseError> {
    if self.strict {
      return Err(PdbParseError::InvalidStructure { line, message });
    }
    builder.diagnostics.push(Diagnostic {
      code,
      line,
      severity: DiagnosticSeverity::Warning,
      message,
    });
    Ok(())
  }
}

/// Lookup key for a chain shared by all coordinate models.
///
/// The model is deliberately absent: when model 2 repeats chain A, it must
/// point to the same topology chain as model 1 while the coordinates go into
/// a different coordinate set.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ChainKey {
  /// Author chain identifier; `None` represents a blank chain identifier.
  auth_id: Option<String>,
}

/// Lookup key for a residue in the normalized structure topology.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ResidueKey {
  /// Parent chain in the structure table.
  chain_id: ChainId,
  /// Author residue sequence number.
  sequence_number: i32,
  /// PDB insertion code, if present.
  insertion_code: Option<char>,
  /// Residue name is included because malformed files can reuse a sequence
  /// number for different residue records.
  name: String,
  /// ATOM and HETATM records are kept as distinct residue kinds.
  kind: ResidueKind,
}

/// Lookup key for one atom/conformer in the shared topology.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct AtomKey {
  /// Parent residue.
  residue_id: ResidueId,
  /// Normalized atom name.
  name: String,
  /// Alternate location distinguishes conformers with the same atom name.
  altloc: Option<char>,
}

/// Mutable assembly state used while converting records into dense tables.
#[derive(Debug, Default)]
struct StructureBuilder {
  /// Structure tables being assembled.
  structure: Structure,
  /// Recoverable warnings collected in source order.
  diagnostics: Vec<Diagnostic>,
  /// Model receiving subsequent ATOM/HETATM records.
  current_model: Option<ModelId>,
  /// Maps source chain identity to the shared chain table.
  chain_ids: HashMap<ChainKey, ChainId>,
  /// Maps source residue identity to the shared residue table.
  residue_ids: HashMap<ResidueKey, ResidueId>,
  /// Maps source atom/conformer identity to the shared atom table.
  atom_ids: HashMap<AtomKey, AtomId>,
}

impl StructureBuilder {
  /// Adds an atom record while preserving one topology row across models.
  ///
  /// # Parameters
  ///
  /// * `record` contains the normalized fields and coordinates from one PDB
  ///   ATOM or HETATM line.
  ///
  /// # Returns
  ///
  /// `Ok(())` after inserting or updating the atom position. A duplicate atom
  /// in one model returns [`PdbParseError::InvalidStructure`].
  ///
  /// The topology map is consulted before allocation. If the atom already
  /// exists, only the current model's coordinate slot is written; if it is
  /// new, every existing coordinate set receives a NaN placeholder to keep
  /// all position vectors aligned with the atom table.
  fn add_atom(&mut self, record: AtomRecord) -> Result<(), PdbParseError> {
    let model_id = self.ensure_model();
    let chain_id = self.ensure_chain(model_id, record.chain_id.clone());
    let residue_id = self.ensure_residue(
      chain_id,
      record.residue_name.clone(),
      record.sequence_number,
      record.insertion_code,
      record.kind,
    );
    let atom_key = AtomKey {
      residue_id,
      name: record.name.clone(),
      altloc: record.altloc,
    };
    let coordinate_set_id = self.structure.models[model_id.index()].coordinate_set_id;

    let atom_id = if let Some(&atom_id) = self.atom_ids.get(&atom_key) {
      let position = &mut self.structure.coordinates[coordinate_set_id.index()].positions;
      if position[atom_id.index()][0].is_finite() {
        return Err(PdbParseError::InvalidStructure {
          line: record.line,
          message: format!("duplicate atom {} in model", record.name),
        });
      }
      atom_id
    } else {
      let atom_id = AtomId::from_index(self.structure.atoms.len());
      self.atom_ids.insert(atom_key, atom_id);
      self.structure.atoms.push(Atom {
        serial: record.serial,
        name: record.name,
        element: record.element,
        residue_id,
        altloc: record.altloc,
        occupancy: record.occupancy,
        b_factor: record.b_factor,
        formal_charge: record.formal_charge,
      });
      for coordinates in &mut self.structure.coordinates {
        coordinates.positions.push([f32::NAN; 3]);
      }
      self.structure.residues[residue_id.index()].atom_ids.push(atom_id);
      atom_id
    };

    self.structure.coordinates[coordinate_set_id.index()].positions[atom_id.index()] = record.position;
    Ok(())
  }

  /// Ensures that a model and its coordinate vector exist before an atom is added.
  ///
  /// # Returns
  ///
  /// The active model ID. Files without MODEL records receive one implicit
  /// model numbered one.
  fn ensure_model(&mut self) -> ModelId {
    if let Some(model_id) = self.current_model {
      return model_id;
    }
    self.create_model(1)
  }

  /// Creates a dense model ID and a coordinate vector aligned with all atoms.
  ///
  /// # Parameters
  ///
  /// * `source_number` is the model number written in the PDB record.
  ///
  /// # Returns
  ///
  /// The newly allocated dense model ID.
  fn create_model(&mut self, source_number: i32) -> ModelId {
    let model_id = ModelId::from_index(self.structure.models.len());
    let coordinate_set_id = CoordinateSetId::from_index(self.structure.coordinates.len());
    self.structure.coordinates.push(CoordinateSet {
      positions: vec![[f32::NAN; 3]; self.structure.atoms.len()],
    });
    self.structure.models.push(Model {
      id: model_id,
      source_number,
      chain_ids: Vec::new(),
      coordinate_set_id,
    });
    self.current_model = Some(model_id);
    model_id
  }

  /// Starts an explicit MODEL record, replacing the empty implicit model.
  ///
  /// PDB files commonly begin with MODEL 1. The builder initially creates no
  /// model, so the first explicit record creates model 1 directly. The special
  /// replacement branch handles a parser-created empty implicit model without
  /// leaving an unused coordinate set in the result.
  ///
  /// # Parameters
  ///
  /// * `source_number` is the source MODEL number.
  fn start_model(&mut self, source_number: i32) {
    if let Some(model_id) = self.current_model {
      let coordinate_set_id = self.structure.models[model_id.index()].coordinate_set_id;
      let has_atoms = self.structure.coordinates[coordinate_set_id.index()]
        .positions
        .iter()
        .any(|position| position[0].is_finite());
      if !has_atoms && self.structure.models.len() == 1 {
        self.structure.models[model_id.index()].source_number = source_number;
        return;
      }
      self.finish_model();
    }
    self.create_model(source_number);
  }

  /// Ends the current model and clears record-local context.
  fn finish_model(&mut self) {
    self.current_model = None;
  }

  /// Ends the current chain without changing the active model.
  fn finish_chain(&mut self) {
    // TER is currently a topology boundary marker. Chain identity is resolved
    // from each subsequent atom record, so no mutable chain cursor is needed.
  }

  /// Adds or reuses a chain for the current model.
  ///
  /// # Parameters
  ///
  /// * `model_id` receives the chain in its ordered chain list.
  /// * `auth_id` is the source chain identifier, or `None` for a blank ID.
  ///
  /// # Returns
  ///
  /// A shared chain ID. Reusing it across models preserves topology while each
  /// model records its own membership.
  fn ensure_chain(&mut self, model_id: ModelId, auth_id: Option<String>) -> ChainId {
    let key = ChainKey {
      auth_id: auth_id.clone(),
    };
    if let Some(&chain_id) = self.chain_ids.get(&key) {
      if !self.structure.models[model_id.index()].chain_ids.contains(&chain_id) {
        self.structure.models[model_id.index()].chain_ids.push(chain_id);
      }
      return chain_id;
    }
    let chain_id = ChainId::from_index(self.structure.chains.len());
    self.chain_ids.insert(key, chain_id);
    self.structure.chains.push(Chain {
      auth_id,
      residue_ids: Vec::new(),
    });
    self.structure.models[model_id.index()].chain_ids.push(chain_id);
    chain_id
  }

  /// Adds or reuses a residue and keeps source order in its chain.
  ///
  /// # Parameters
  ///
  /// * `chain_id` identifies the parent chain.
  /// * `name` and `kind` distinguish polymer and hetero records.
  /// * `sequence_number` and `insertion_code` identify source residue order.
  ///
  /// # Returns
  ///
  /// A shared residue ID suitable for indexing the atom table membership.
  fn ensure_residue(
    &mut self,
    chain_id: ChainId,
    name: String,
    sequence_number: i32,
    insertion_code: Option<char>,
    kind: ResidueKind,
  ) -> ResidueId {
    let key = ResidueKey {
      chain_id,
      sequence_number,
      insertion_code,
      name: name.clone(),
      kind,
    };
    if let Some(&residue_id) = self.residue_ids.get(&key) {
      return residue_id;
    }
    let residue_id = ResidueId::from_index(self.structure.residues.len());
    self.residue_ids.insert(key, residue_id);
    self.structure.residues.push(Residue {
      name,
      chain_id,
      sequence_number,
      insertion_code,
      kind,
      atom_ids: Vec::new(),
      secondary_structure: SecondaryStructure::Unknown,
    });
    self.structure.chains[chain_id.index()].residue_ids.push(residue_id);
    residue_id
  }

  /// Stores the fixed-column HEADER values when they are present.
  ///
  /// Blank fixed-column fields are normalized to `None`; the original spacing
  /// is not meaningful once the source record has been decoded.
  fn set_header(&mut self, classification: &str, identifier: &str) {
    self.structure.metadata.classification = non_empty(classification);
    self.structure.metadata.identifier = non_empty(identifier);
  }

  /// Finalizes the snapshot and checks all index/coordinate invariants.
  ///
  /// # Returns
  ///
  /// The completed structure and recoverable diagnostics, or an error when a
  /// builder invariant is broken.
  fn finish(mut self) -> Result<PdbParseResult, PdbParseError> {
    if self.structure.models.is_empty() {
      // An END-only file is a valid empty snapshot and needs no implicit model.
    }
    self.finish_model();
    self
      .structure
      .validate_invariants()
      .map_err(|message| PdbParseError::InvalidStructure { line: 0, message })?;
    Ok(PdbParseResult {
      structure: self.structure,
      diagnostics: self.diagnostics,
    })
  }
}

/// Normalized fields extracted from one ATOM or HETATM record.
#[derive(Debug)]
struct AtomRecord {
  /// One-based source line for errors raised during insertion.
  line: usize,
  /// Source serial number, when present.
  serial: Option<i32>,
  /// Atom name from columns 13–16.
  name: AtomName,
  /// Element from columns 77–78.
  element: Option<Element>,
  /// Chain identifier from column 22.
  chain_id: Option<String>,
  /// Residue name from columns 18–20.
  residue_name: String,
  /// Residue sequence number from columns 23–26.
  sequence_number: i32,
  /// Insertion code from column 27.
  insertion_code: Option<char>,
  /// Alternate-location code from column 17.
  altloc: Option<char>,
  /// Occupancy from columns 55–60.
  occupancy: Option<f32>,
  /// B-factor from columns 61–66.
  b_factor: Option<f32>,
  /// Optional formal charge from columns 79–80.
  formal_charge: Option<i8>,
  /// Cartesian coordinates from columns 31–54, in ångström units.
  position: [f32; 3],
  /// Whether the source record was ATOM or HETATM.
  kind: ResidueKind,
}

/// Parses one ATOM or HETATM record using the PDB fixed-column layout.
///
/// # Parameters
///
/// * `line` is one decoded PDB record without its line ending.
/// * `line_number` identifies the record in a parse diagnostic.
/// * `hetero` selects the residue classification for HETATM records.
///
/// # Returns
///
/// The normalized atom fields and Cartesian position, or the first invalid
/// required field.
///
/// # Examples
///
/// ```text
/// ATOM      1  CA  ALA A   1       1.000   2.000   3.000  1.00 20.00           C
/// ```
fn parse_atom(line: &str, line_number: usize, hetero: bool) -> Result<AtomRecord, PdbParseError> {
  Ok(AtomRecord {
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

/// Returns a bounded fixed-column field without panicking on short records.
///
/// # Parameters
///
/// * `line` is a decoded source record.
/// * `start` and `end` are zero-based byte-column bounds, with `end` exclusive.
///
/// # Returns
///
/// The requested substring, or an empty string when a short/non-ASCII boundary
/// cannot be represented as a UTF-8 slice.
fn field(line: &str, start: usize, end: usize) -> &str {
  let bytes = line.as_bytes();
  let start = start.min(bytes.len());
  let end = end.min(bytes.len()).max(start);
  // PDB is ASCII by specification; parse_bytes validates UTF-8 before this
  // helper is called, so slicing these byte boundaries is safe here.
  line.get(start..end).unwrap_or_default()
}

/// Parses a required signed integer field.
///
/// # Parameters
///
/// * `line` is the source record.
/// * `start` and `end` delimit the fixed-width field.
/// * `field_name` labels the returned parse error.
/// * `line_number` identifies the source record.
///
/// # Returns
///
/// The parsed integer or [`PdbParseError::InvalidField`].
fn parse_required_i32(
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

/// Parses an optional signed integer field.
///
/// # Parameters
///
/// * `line` is the source record.
/// * `start` and `end` delimit the fixed-width field.
/// * `field_name` labels the returned parse error.
/// * `line_number` identifies the source record.
///
/// # Returns
///
/// `Ok(None)` for a blank field, otherwise the parsed integer or a field error.
fn parse_optional_i32(
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

/// Parses an optional floating-point field.
///
/// # Parameters
///
/// * `line` is the source record.
/// * `start` and `end` delimit the fixed-width field.
/// * `field_name` labels the returned parse error.
/// * `line_number` identifies the source record.
///
/// # Returns
///
/// `Ok(None)` for a blank field, otherwise the parsed float or a field error.
fn parse_optional_f32(
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
///
/// # Parameters
///
/// * `line` is the source record.
/// * `start` and `end` delimit the fixed-width field.
/// * `field_name` labels an error.
/// * `line_number` identifies the source record.
///
/// # Returns
///
/// A finite value, or an error for an empty, malformed, or non-finite field.
fn parse_required_f32(
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
///
/// PDB uses blank columns for absent insertion and alternate-location values;
/// normalizing those blanks to `None` keeps them out of topology keys.
fn single_char(value: &str) -> Option<char> {
  value.trim().chars().next()
}

/// Parses PDB's optional charge notation, such as 1+ or 2-.
///
/// Values with an unknown sign or magnitude are treated as absent because the
/// charge field is optional and is not needed to construct coordinates.
fn parse_charge(value: &str) -> Option<i8> {
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
fn non_empty(value: &str) -> Option<String> {
  let value = value.trim();
  (!value.is_empty()).then(|| value.to_owned())
}

#[cfg(test)]
mod tests {
  use super::*;

  const ATOM_1: &str = "ATOM      1  N   GLY A   1       0.000   0.000   0.000  1.00 10.00           N  ";
  const ATOM_2: &str = "ATOM      2  CA  GLY A   1       1.000   2.000   3.000  1.00 11.00           C  ";

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
}
