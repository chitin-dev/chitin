//! Projection from typed PDB records into format-neutral structure input.
//!
//! Cross-record PDB constructs such as `COMPND`, `SEQRES`, and `REMARK 350`
//! are resolved here. The shared structure builder therefore receives only
//! normalized semantic data and has no dependency on fixed-column syntax.

use std::collections::{HashMap, HashSet};

use super::fields::{field, non_empty, parse_required_f32, parse_required_i32};
use super::records::{PdbAtomRecord, PdbDocument, PdbProjectionRecordKind, PdbRecord, PdbSecondaryRange};
use crate::structure::error::{Diagnostic, DiagnosticSeverity, PdbParseError};
use crate::structure::model::{
  AssemblyGeneration, BiologicalAssembly, BondSource, PolymerEntity, PolymerSequenceResidue, PolymerType,
  StructureOperation, Symmetry, UnitCell,
};
use crate::structure::projection::{
  ProjectedAssemblyGeneration, ProjectedAtom, ProjectedMissingResidue, ProjectedSecondaryRange, ProjectedSerialBond,
  StructureInput,
};

/// A molecular entity collected across PDB `COMPND` continuation records.
#[derive(Debug, Default)]
struct PdbCompound {
  /// Author chain identifiers assigned to the entity.
  chains: Vec<String>,
}

/// One PDB `BIOMT` operation collected from its three matrix rows.
#[derive(Debug, Clone)]
struct PdbBiomt {
  /// Rotation matrix populated in BIOMT1/2/3 order.
  rotation: [[f32; 3]; 3],
  /// Translation vector populated alongside the matrix rows.
  translation: [f32; 3],
  /// Whether each of the three rows has been observed.
  rows: [bool; 3],
}

/// Stateful PDB semantic projection accumulated across source records.
#[derive(Debug, Default)]
struct PdbProjection {
  input: StructureInput,
  compounds: HashMap<String, PdbCompound>,
  current_compound: Option<String>,
  sequences: HashMap<String, Vec<String>>,
  assembly_chains: HashMap<String, Vec<String>>,
  biomts: HashMap<String, PdbBiomt>,
  current_assembly: Option<String>,
  strict: bool,
}

/// Projects a parsed PDB document into shared semantic structure input.
///
/// # Parameters
///
/// * `document` contains typed fixed-column records in source order.
/// * `strict` determines whether unsupported records become fatal errors or
///   recoverable diagnostics.
///
/// # Returns
///
/// A format-neutral [`StructureInput`] ready for common topology generation,
/// or a [`PdbParseError`] when PDB-specific semantic fields are malformed.
///
/// # Examples
///
/// ```text
/// PdbDocument -> PDB projection -> StructureInput
/// ```
pub(super) fn project(document: PdbDocument, strict: bool) -> Result<StructureInput, PdbParseError> {
  let mut projection = PdbProjection {
    strict,
    ..PdbProjection::default()
  };
  for record in document.records {
    projection.project_record(record)?;
  }
  Ok(projection.finish())
}

impl PdbProjection {
  /// Projects one typed PDB record into semantic state.
  ///
  /// # Parameters
  ///
  /// * `record` is a syntax-level PDB record produced by the fixed-column
  ///   parser.
  ///
  /// # Returns
  ///
  /// `Ok(())` after the record is projected or deliberately ignored, or a PDB
  /// semantic error when its cross-record values are invalid.
  fn project_record(&mut self, record: PdbRecord) -> Result<(), PdbParseError> {
    match record {
      PdbRecord::Header {
        classification,
        identifier,
      } => {
        self.input.classification = classification;
        self.input.identifier = identifier;
      }
      PdbRecord::Model(source_number) => self.input.start_model(source_number),
      PdbRecord::EndModel => self.input.finish_model(),
      PdbRecord::Atom(atom) => self.input.add_atom(project_atom(atom)),
      PdbRecord::Ter | PdbRecord::Ignored => {}
      PdbRecord::Conect(record) => {
        self
          .input
          .serial_bonds
          .extend(record.targets.into_iter().map(|target| ProjectedSerialBond {
            line: record.line,
            source: record.source,
            target,
            bond_source: BondSource::Conect,
            unresolved_code: "PDB_CONECT_UNKNOWN_ATOM",
          }));
      }
      PdbRecord::Secondary(range) => self.input.secondary_ranges.push(project_secondary_range(range)),
      PdbRecord::ProjectionRecord {
        line_number,
        kind,
        line,
      } => match kind {
        PdbProjectionRecordKind::Compnd => self.project_compnd(&line, line_number)?,
        PdbProjectionRecordKind::Seqres => self.project_seqres(&line, line_number)?,
        PdbProjectionRecordKind::Remark => self.project_remark(&line, line_number)?,
        PdbProjectionRecordKind::Cryst1 => self.project_cryst1(&line, line_number)?,
      },
      PdbRecord::Unknown { line, name } => self.record_warning(
        line,
        "PDB_UNKNOWN_RECORD",
        format!("ignored unsupported record {name:?}"),
      )?,
    }
    Ok(())
  }

  /// Projects one `COMPND` continuation into entity-association state.
  ///
  /// # Parameters
  ///
  /// * `line` is the original fixed-column record.
  /// * `line_number` is its one-based source location.
  ///
  /// # Returns
  ///
  /// `Ok(())` after a supported key is retained. A blank `MOL_ID` returns a
  /// field error, while unsupported keys are intentionally ignored.
  fn project_compnd(&mut self, line: &str, line_number: usize) -> Result<(), PdbParseError> {
    let Some((key, raw_value)) = field(line, 12, 80).trim().split_once(':') else {
      self.input.diagnostics.push(Diagnostic {
        code: "PDB_INVALID_COMPND",
        line: line_number,
        severity: DiagnosticSeverity::Warning,
        message: "COMPND record is missing a key/value separator".to_owned(),
      });
      return Ok(());
    };
    let value = raw_value.trim().trim_end_matches(';').trim();
    match key.trim().to_ascii_uppercase().as_str() {
      "MOL_ID" => {
        if value.is_empty() {
          return Err(PdbParseError::InvalidField {
            line: line_number,
            field: "MOL_ID",
            value: value.to_owned(),
          });
        }
        self.current_compound = Some(value.to_owned());
        self.compounds.entry(value.to_owned()).or_default();
      }
      "CHAIN" => {
        let Some(mol_id) = self.current_compound.as_deref() else {
          self.input.diagnostics.push(Diagnostic {
            code: "PDB_COMPND_CHAIN_WITHOUT_MOL_ID",
            line: line_number,
            severity: DiagnosticSeverity::Warning,
            message: "COMPND CHAIN appears before MOL_ID".to_owned(),
          });
          return Ok(());
        };
        let compound = self.compounds.entry(mol_id.to_owned()).or_default();
        for chain in value.split(',').map(str::trim).filter(|chain| !chain.is_empty()) {
          if !compound.chains.iter().any(|existing| existing == chain) {
            compound.chains.push(chain.to_owned());
          }
        }
      }
      _ => {}
    }
    Ok(())
  }

  /// Projects one `SEQRES` record into a chain's declared sequence.
  ///
  /// # Parameters
  ///
  /// * `line` is the original fixed-column record.
  /// * `line_number` is its one-based source location.
  ///
  /// # Returns
  ///
  /// `Ok(())` after appending component names, or a field error when the chain
  /// identifier or declared residue count is malformed.
  fn project_seqres(&mut self, line: &str, line_number: usize) -> Result<(), PdbParseError> {
    let chain_id = non_empty(field(line, 11, 12)).ok_or_else(|| PdbParseError::InvalidField {
      line: line_number,
      field: "SEQRES chain",
      value: field(line, 11, 12).to_owned(),
    })?;
    parse_required_i32(line, 13, 17, "SEQRES residue count", line_number)?;
    self
      .sequences
      .entry(chain_id)
      .or_default()
      .extend(field(line, 19, 70).split_whitespace().map(str::to_owned));
    Ok(())
  }

  /// Dispatches supported REMARK records to PDB-specific projection logic.
  fn project_remark(&mut self, line: &str, line_number: usize) -> Result<(), PdbParseError> {
    match field(line, 7, 10).trim() {
      "465" => self.project_missing_residue(line),
      "350" => self.project_assembly_remark(line, line_number),
      _ => Ok(()),
    }
  }

  /// Projects one `REMARK 465` missing-residue data row.
  ///
  /// The `REMARK 465` section also contains headings and explanatory text.
  /// Those lines occupy the same fixed columns but do not contain a numeric
  /// sequence number, so they are intentionally ignored here.
  fn project_missing_residue(&mut self, line: &str) -> Result<(), PdbParseError> {
    let residue_name = field(line, 15, 18).trim();
    let chain_id = field(line, 19, 20).trim();
    let sequence = field(line, 21, 26).trim();
    if residue_name.is_empty() || chain_id.is_empty() || sequence.is_empty() {
      return Ok(());
    }
    let Ok(sequence_number) = sequence.parse::<i32>() else {
      return Ok(());
    };
    self.input.missing_residues.push(ProjectedMissingResidue {
      chain_id: chain_id.to_owned(),
      sequence_number,
      monomer: residue_name.to_owned(),
      hetero: false,
    });
    Ok(())
  }

  /// Projects one `REMARK 350` line into assembly operations and chain sets.
  ///
  /// # Parameters
  ///
  /// * `line` is the original remark record.
  /// * `line_number` is its one-based source location.
  ///
  /// # Returns
  ///
  /// `Ok(())` after retaining an assembly declaration, chain continuation, or
  /// BIOMT row. Malformed assembly IDs and numeric matrix values are fatal.
  fn project_assembly_remark(&mut self, line: &str, line_number: usize) -> Result<(), PdbParseError> {
    let text = field(line, 11, 80).trim();
    if let Some(value) = text.strip_prefix("BIOMOLECULE:") {
      let assembly_id = value.trim();
      if assembly_id.is_empty() {
        return Err(PdbParseError::InvalidField {
          line: line_number,
          field: "REMARK 350 BIOMOLECULE",
          value: value.to_owned(),
        });
      }
      self.current_assembly = Some(assembly_id.to_owned());
      self.assembly_chains.entry(assembly_id.to_owned()).or_default();
      return Ok(());
    }
    if let Some(value) = text.strip_prefix("APPLY THE FOLLOWING TO CHAINS:") {
      self.add_assembly_chains(value);
      return Ok(());
    }
    if let Some(value) = text.strip_prefix("AND:") {
      self.add_assembly_chains(value);
      return Ok(());
    }
    let Some(operation_text) = text.strip_prefix("BIOMT") else {
      return Ok(());
    };
    let Some(row_digit) = operation_text.chars().next().and_then(|value| value.to_digit(10)) else {
      return Ok(());
    };
    if !(1..=3).contains(&row_digit) {
      return Ok(());
    }
    let values = operation_text[1..].split_whitespace().collect::<Vec<_>>();
    if values.len() != 5 {
      return Err(PdbParseError::InvalidField {
        line: line_number,
        field: "REMARK 350 BIOMT row",
        value: text.to_owned(),
      });
    }
    let operation_id = values[0].to_owned();
    let row = row_digit as usize - 1;
    let mut parsed = [0.0; 4];
    for (index, value) in values[1..].iter().enumerate() {
      parsed[index] = value.parse::<f32>().map_err(|_| PdbParseError::InvalidField {
        line: line_number,
        field: "REMARK 350 BIOMT value",
        value: (*value).to_owned(),
      })?;
    }
    let operation = self.biomts.entry(operation_id).or_insert_with(|| PdbBiomt {
      rotation: [[0.0; 3]; 3],
      translation: [0.0; 3],
      rows: [false; 3],
    });
    operation.rotation[row] = [parsed[0], parsed[1], parsed[2]];
    operation.translation[row] = parsed[3];
    operation.rows[row] = true;
    Ok(())
  }

  /// Adds chain identifiers to the active PDB biological assembly.
  fn add_assembly_chains(&mut self, value: &str) {
    let Some(assembly_id) = self.current_assembly.as_deref() else {
      return;
    };
    let chains = self.assembly_chains.entry(assembly_id.to_owned()).or_default();
    for chain in value
      .trim_end_matches(',')
      .split(',')
      .map(str::trim)
      .filter(|chain| !chain.is_empty())
    {
      if !chains.iter().any(|existing| existing == chain) {
        chains.push(chain.to_owned());
      }
    }
  }

  /// Projects crystallographic cell parameters from a `CRYST1` record.
  ///
  /// # Parameters
  ///
  /// * `line` is the original fixed-column record.
  /// * `line_number` is its one-based source location.
  ///
  /// # Returns
  ///
  /// `Ok(())` after storing a valid cell and symmetry declaration. Non-positive
  /// lengths and angles outside the open interval `(0, 180)` are rejected.
  fn project_cryst1(&mut self, line: &str, line_number: usize) -> Result<(), PdbParseError> {
    let lengths = [
      parse_required_f32(line, 6, 15, "CRYST1 length a", line_number)?,
      parse_required_f32(line, 15, 24, "CRYST1 length b", line_number)?,
      parse_required_f32(line, 24, 33, "CRYST1 length c", line_number)?,
    ];
    let angles = [
      parse_required_f32(line, 33, 40, "CRYST1 angle alpha", line_number)?,
      parse_required_f32(line, 40, 47, "CRYST1 angle beta", line_number)?,
      parse_required_f32(line, 47, 54, "CRYST1 angle gamma", line_number)?,
    ];
    if lengths.iter().any(|length| *length <= 0.0) || angles.iter().any(|angle| !(0.0..180.0).contains(angle)) {
      return Err(PdbParseError::InvalidStructure {
        line: line_number,
        message: "CRYST1 contains invalid unit-cell parameters".to_owned(),
      });
    }
    self.input.unit_cell = Some(UnitCell { lengths, angles });
    self.input.symmetry = Some(Symmetry {
      space_group_name: non_empty(field(line, 55, 66)),
      international_tables_number: None,
    });
    Ok(())
  }

  /// Converts accumulated cross-record PDB state into shared projection rows.
  fn finish(mut self) -> StructureInput {
    self.project_polymer_entities();
    self.project_assemblies();
    self.input
  }

  /// Converts `COMPND` and `SEQRES` state into normalized polymer entities.
  fn project_polymer_entities(&mut self) {
    let mut chain_entities = HashMap::new();
    for (entity_id, compound) in std::mem::take(&mut self.compounds) {
      for chain_id in compound.chains {
        chain_entities.insert(chain_id, entity_id.clone());
      }
      self.input.polymer_entities.push(PolymerEntity {
        id: entity_id,
        polymer_type: PolymerType::Other("unknown".to_owned()),
        sequence: Vec::new(),
        chain_ids: Vec::new(),
      });
    }

    let mut projected_entities = HashSet::new();
    for (chain_id, monomers) in std::mem::take(&mut self.sequences) {
      let entity_id = if let Some(entity_id) = chain_entities.get(&chain_id).cloned() {
        entity_id
      } else {
        // SEQRES may occur without a complete COMPND block. The author chain
        // is then the only stable entity identity available to projection.
        let entity_id = chain_id.clone();
        chain_entities.insert(chain_id.clone(), entity_id.clone());
        self.input.polymer_entities.push(PolymerEntity {
          id: entity_id.clone(),
          polymer_type: PolymerType::Other("unknown".to_owned()),
          sequence: Vec::new(),
          chain_ids: Vec::new(),
        });
        entity_id
      };
      let polymer_type = polymer_type_from_pdb_sequence(&monomers);
      if let Some(entity) = self
        .input
        .polymer_entities
        .iter_mut()
        .find(|entity| entity.id == entity_id)
      {
        entity.polymer_type = polymer_type;
        if projected_entities.insert(entity_id) {
          entity.sequence.extend(
            monomers
              .into_iter()
              .enumerate()
              .map(|(index, monomer)| PolymerSequenceResidue {
                number: index as i32 + 1,
                monomer,
                hetero: false,
              }),
          );
        }
      }
    }
    self.input.label_chain_entities.extend(chain_entities);
  }

  /// Converts complete BIOMT matrices and chain selections into assembly data.
  fn project_assemblies(&mut self) {
    let mut operation_ids = Vec::new();
    for (id, operation) in std::mem::take(&mut self.biomts) {
      if !operation.rows.iter().all(|row| *row) {
        self.input.diagnostics.push(Diagnostic {
          code: "PDB_INCOMPLETE_BIOMT",
          line: 0,
          severity: DiagnosticSeverity::Warning,
          message: format!("BIOMT operation {id} is missing one or more matrix rows"),
        });
        continue;
      }
      operation_ids.push(id.clone());
      self.input.structure_operations.push(StructureOperation {
        id,
        rotation: operation.rotation,
        translation: operation.translation,
      });
    }
    operation_ids.sort();
    let operator_expression = operation_ids.join(",");
    for (assembly_id, chains) in std::mem::take(&mut self.assembly_chains) {
      self.input.biological_assemblies.push(BiologicalAssembly {
        id: assembly_id.clone(),
        details: Some("PDB REMARK 350 biological assembly".to_owned()),
        generations: Vec::new(),
      });
      if !chains.is_empty() && !operator_expression.is_empty() {
        self.input.assembly_generations.push(ProjectedAssemblyGeneration {
          assembly_id,
          generation: AssemblyGeneration {
            asym_ids: chains.clone(),
            auth_asym_ids: chains,
            entity_instance_ids: Vec::new(),
            operator_expression: operator_expression.clone(),
          },
        });
      }
    }
  }

  /// Records a recoverable PDB projection warning or returns a strict failure.
  fn record_warning(&mut self, line: usize, code: &'static str, message: String) -> Result<(), PdbParseError> {
    if self.strict {
      return Err(PdbParseError::InvalidStructure { line, message });
    }
    self.input.diagnostics.push(Diagnostic {
      code,
      line,
      severity: DiagnosticSeverity::Warning,
      message,
    });
    Ok(())
  }
}

/// Converts one PDB atom record into the shared atom projection.
fn project_atom(record: PdbAtomRecord) -> ProjectedAtom {
  ProjectedAtom {
    line: record.line,
    serial: record.serial,
    name: record.name,
    element: record.element,
    chain_id: record.chain_id,
    residue_name: record.residue_name,
    sequence_number: record.sequence_number,
    insertion_code: record.insertion_code,
    altloc: record.altloc,
    occupancy: record.occupancy,
    b_factor: record.b_factor,
    formal_charge: record.formal_charge,
    position: record.position,
    kind: record.kind,
    label_chain_id: None,
    label_sequence_number: None,
    label_atom_name: None,
    label_entity_id: None,
  }
}

/// Converts PDB secondary-structure endpoints into normalized projection data.
fn project_secondary_range(range: PdbSecondaryRange) -> ProjectedSecondaryRange {
  ProjectedSecondaryRange {
    line: range.line,
    chain_id: range.chain_id,
    start_sequence_number: range.start_sequence_number,
    start_insertion_code: range.start_insertion_code,
    end_sequence_number: range.end_sequence_number,
    end_insertion_code: range.end_insertion_code,
    kind: range.kind,
    unresolved_code: "PDB_SECONDARY_UNKNOWN_RESIDUE",
  }
}

/// Infers the polymer family represented by PDB `SEQRES` component names.
fn polymer_type_from_pdb_sequence(sequence: &[String]) -> PolymerType {
  if sequence.iter().all(|monomer| {
    matches!(
      monomer.as_str(),
      "DA" | "DC" | "DG" | "DT" | "DI" | "DU" | "A" | "C" | "G" | "U" | "I"
    )
  }) {
    if sequence.iter().any(|monomer| monomer.len() == 2) {
      PolymerType::Polydeoxyribonucleotide
    } else {
      PolymerType::Polyribonucleotide
    }
  } else {
    PolymerType::Polypeptide
  }
}
