//! Shared construction of indexed molecular structures.
//!
//! This module consumes format-neutral projection data. It assigns dense IDs,
//! shares topology between coordinate models, resolves semantic references,
//! and validates the completed [`Structure`]. Source syntax belongs in the
//! PDB and mmCIF projection modules and must not be interpreted here.

use std::collections::{HashMap, HashSet};

use super::error::{Diagnostic, DiagnosticSeverity, StructureBuildError, StructureParseResult};
use super::model::*;
use super::projection::{
  AtomLookupKey, ProjectedAtom, ProjectedMissingResidue, ProjectedNamedBond, ProjectedSecondaryRange,
  ProjectedSerialBond, StructureInput,
};

/// Lookup key for a chain shared by all coordinate models.
///
/// The model is deliberately absent: when model 2 repeats chain A, it must
/// point to the same topology chain as model 1 while the coordinates go into
/// a different coordinate set.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ChainKey {
  /// Author chain identifier; `None` represents a blank chain identifier.
  auth_id: Option<String>,
  /// Label/asym chain identifier when the source provides one.
  label_id: Option<String>,
}

/// Lookup key for a residue in the normalized structure topology.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ResidueKey {
  /// Parent chain in the structure table.
  chain_id: ChainId,
  /// Author residue sequence number.
  sequence_number: i32,
  /// Source insertion code, if present.
  insertion_code: Option<char>,
  /// Residue name is included because malformed files can reuse a sequence
  /// number for different residue records.
  name: String,
  /// ATOM and HETATM records are kept as distinct residue kinds. HETATM represents
  /// for "heterogen atom"
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
pub(crate) struct StructureBuilder {
  structure: Structure,
  /// Recoverable warnings collected in source order.
  pub(super) diagnostics: Vec<Diagnostic>,
  /// Model receiving subsequent ATOM/HETATM records.
  current_model: Option<ModelId>,
  /// Maps source chain identity to the shared chain table.
  chain_ids: HashMap<ChainKey, ChainId>,
  /// Maps source residue identity to the shared residue table.
  residue_ids: HashMap<ResidueKey, ResidueId>,
  /// Maps source atom/conformer identity to the shared atom table.
  atom_ids: HashMap<AtomKey, AtomId>,
  /// Maps projected integer serials to normalized atom identifiers.
  serial_ids: HashMap<i32, AtomId>,
  /// Maps label/auth atom aliases to normalized atom identifiers.
  lookup_ids: HashMap<AtomLookupKey, AtomId>,
  /// Maps projected label chain identifiers to polymer entities.
  label_chain_entities: HashMap<String, String>,
  /// Model/chain/entity positions observed in atom-site coordinates.
  observed_polymer_positions: HashSet<(ModelId, String, String, i32)>,
  /// Serial-addressed bonds resolved after all atoms have been indexed.
  pending_bonds: Vec<ProjectedSerialBond>,
  /// Identity-addressed bonds resolved after all atoms have been indexed.
  pending_named_bonds: Vec<ProjectedNamedBond>,
  /// Secondary-structure records are resolved after all residues have been read.
  pending_secondary_ranges: Vec<ProjectedSecondaryRange>,
  /// Missing-residue declarations awaiting normalized chain IDs.
  pending_missing_residues: Vec<ProjectedMissingResidue>,
}

/// Builds an indexed molecular structure from format-neutral projection data.
///
/// # Parameters
///
/// * `input` contains semantic atoms, entities, annotations, and metadata
///   emitted by one format-specific projection.
/// * `strict` determines whether unresolved projected references are fatal or
///   retained as diagnostics.
///
/// # Returns
///
/// The validated structure and its recoverable diagnostics, or a
/// [`StructureBuildError`] when projection data violates a shared invariant.
///
/// # Examples
///
/// The format boundary is intentionally one-way:
///
/// ```text
/// PdbDocument or CifDocument -> StructureInput -> Structure
/// ```
pub(crate) fn build_structure(
  input: StructureInput,
  strict: bool,
) -> Result<StructureParseResult, StructureBuildError> {
  let StructureInput {
    classification,
    identifier,
    unit_cell,
    symmetry,
    models,
    label_chain_entities,
    polymer_entities,
    polymer_sequences,
    serial_bonds,
    named_bonds,
    secondary_ranges,
    missing_residues,
    structure_operations,
    biological_assemblies,
    assembly_generations,
    diagnostics,
    ..
  } = input;
  let mut builder = StructureBuilder {
    diagnostics,
    ..StructureBuilder::default()
  };

  builder.structure.metadata.classification = classification;
  builder.structure.metadata.identifier = identifier;
  builder.structure.metadata.unit_cell = unit_cell;
  builder.structure.metadata.symmetry = symmetry;

  for (label_id, entity_id) in label_chain_entities {
    builder.add_label_chain_entity(label_id, entity_id);
  }
  for entity in polymer_entities {
    builder.add_polymer_entity(entity)?;
  }
  for sequence in polymer_sequences {
    builder.add_polymer_sequence(sequence.entity_id, sequence.residue)?;
  }
  for operation in structure_operations {
    builder.add_structure_operation(operation)?;
  }
  for assembly in biological_assemblies {
    builder.add_biological_assembly(assembly)?;
  }
  for projected in assembly_generations {
    builder.add_assembly_generation(&projected.assembly_id, projected.generation)?;
  }

  // Build each model as one unit so repeated atom identities reuse topology
  // while their Cartesian coordinates enter distinct coordinate sets.
  for model in models {
    builder.start_model(model.source_number);
    for atom in model.atoms {
      builder.add_atom(atom)?;
    }
    builder.finish_model();
  }

  for bond in serial_bonds {
    builder.add_serial_bond(bond);
  }
  builder.pending_named_bonds.extend(named_bonds);
  builder.pending_secondary_ranges.extend(secondary_ranges);
  builder.pending_missing_residues.extend(missing_residues);
  builder.finish(strict)
}

impl StructureBuilder {
  /// Adds an atom record while preserving one topology row across models.
  ///
  /// # Parameters
  ///
  /// * `record` contains normalized semantic fields and coordinates from one
  ///   projected atom site.
  ///
  /// # Returns
  ///
  /// `Ok(())` after inserting or updating the atom position. A duplicate atom
  /// in one model returns [`StructureBuildError`].
  ///
  /// The topology map is consulted before allocation. If the atom already
  /// exists, only the current model's coordinate slot is written; if it is
  /// new, every existing coordinate set receives a NaN placeholder to keep
  /// all position vectors aligned with the atom table.
  pub(crate) fn add_atom(&mut self, record: ProjectedAtom) -> Result<(), StructureBuildError> {
    let model_id = self.ensure_model();
    let entity_id = record
      .label_entity_id
      .clone()
      .or_else(|| {
        record
          .label_chain_id
          .as_ref()
          .and_then(|label_id| self.label_chain_entities.get(label_id).cloned())
      })
      .or_else(|| {
        record
          .chain_id
          .as_ref()
          .and_then(|chain_id| self.label_chain_entities.get(chain_id).cloned())
      });
    let chain_id = self.ensure_chain(
      model_id,
      record.chain_id.clone(),
      record.label_chain_id.clone(),
      entity_id.clone(),
    );
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
    let lookup_chain_id = record.chain_id.clone();
    let lookup_sequence_number = record.sequence_number;
    let lookup_atom_name = record.name.clone();
    let lookup_insertion_code = record.insertion_code;
    let lookup_altloc = record.altloc;
    let label_lookup = record
      .label_chain_id
      .clone()
      .zip(record.label_sequence_number)
      .zip(record.label_atom_name.clone());
    let coordinate_set_id = self.structure.models[model_id.index()].coordinate_set_id;

    let atom_id = if let Some(&atom_id) = self.atom_ids.get(&atom_key) {
      let position = &mut self.structure.coordinates[coordinate_set_id.index()].positions;
      if position[atom_id.index()][0].is_finite() {
        return Err(StructureBuildError {
          line: record.line,
          message: format!("duplicate atom {} in model", record.name),
        });
      }
      atom_id
    } else {
      let atom_id = AtomId::from_index(self.structure.atoms.len());
      self.atom_ids.insert(atom_key, atom_id);
      if let Some(serial) = record.serial {
        self.serial_ids.entry(serial).or_insert(atom_id);
      }
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

    self.lookup_ids.insert(
      AtomLookupKey {
        chain_id: lookup_chain_id,
        sequence_number: lookup_sequence_number,
        atom_name: lookup_atom_name,
        insertion_code: lookup_insertion_code,
        altloc: lookup_altloc,
      },
      atom_id,
    );
    if let Some(((chain_id, sequence_number), atom_name)) = label_lookup {
      self.lookup_ids.insert(
        AtomLookupKey {
          chain_id: Some(chain_id),
          sequence_number,
          atom_name,
          insertion_code: lookup_insertion_code,
          altloc: lookup_altloc,
        },
        atom_id,
      );
    }

    self.structure.coordinates[coordinate_set_id.index()].positions[atom_id.index()] = record.position;
    if let (Some(entity_id), Some(label_id), Some(label_sequence_number)) =
      (entity_id, record.label_chain_id, record.label_sequence_number)
    {
      self
        .observed_polymer_positions
        .insert((model_id, label_id, entity_id, label_sequence_number));
    }
    Ok(())
  }

  /// Associates a label chain with its polymer entity.
  pub(crate) fn add_label_chain_entity(&mut self, label_id: String, entity_id: String) {
    self.label_chain_entities.insert(label_id, entity_id);
  }

  /// Adds one assembly operation while rejecting duplicate source identifiers.
  pub(crate) fn add_structure_operation(&mut self, operation: StructureOperation) -> Result<(), StructureBuildError> {
    if self
      .structure
      .metadata
      .assembly
      .operations
      .iter()
      .any(|existing| existing.id == operation.id)
    {
      return Err(StructureBuildError {
        line: 0,
        message: format!("duplicate assembly operation {}", operation.id),
      });
    }
    self.structure.metadata.assembly.operations.push(operation);
    Ok(())
  }

  /// Adds an assembly definition while rejecting duplicate source identifiers.
  pub(crate) fn add_biological_assembly(&mut self, assembly: BiologicalAssembly) -> Result<(), StructureBuildError> {
    if self
      .structure
      .metadata
      .assembly
      .assemblies
      .iter()
      .any(|existing| existing.id == assembly.id)
    {
      return Err(StructureBuildError {
        line: 0,
        message: format!("duplicate biological assembly {}", assembly.id),
      });
    }
    self.structure.metadata.assembly.assemblies.push(assembly);
    Ok(())
  }

  /// Appends one generation rule to its declared biological assembly.
  pub(crate) fn add_assembly_generation(
    &mut self,
    assembly_id: &str,
    generation: AssemblyGeneration,
  ) -> Result<(), StructureBuildError> {
    let Some(assembly) = self
      .structure
      .metadata
      .assembly
      .assemblies
      .iter_mut()
      .find(|assembly| assembly.id == assembly_id)
    else {
      return Err(StructureBuildError {
        line: 0,
        message: format!("assembly generation references unknown assembly {assembly_id}"),
      });
    };
    assembly.generations.push(generation);
    Ok(())
  }

  /// Associates a normalized chain with a polymer entity exactly once.
  pub(crate) fn attach_chain_entity(&mut self, chain_id: ChainId, entity_id: Option<&str>) {
    let Some(entity_id) = entity_id else {
      return;
    };
    self.structure.chains[chain_id.index()].entity_id = Some(entity_id.to_owned());
    if let Some(entity) = self
      .structure
      .polymer_entities
      .iter_mut()
      .find(|entity| entity.id == entity_id)
      && !entity.chain_ids.contains(&chain_id)
    {
      entity.chain_ids.push(chain_id);
    }
  }

  /// Adds or updates an entity declared by `_entity_poly`.
  pub(crate) fn add_polymer_entity(&mut self, entity: PolymerEntity) -> Result<(), StructureBuildError> {
    if self
      .structure
      .polymer_entities
      .iter()
      .any(|existing| existing.id == entity.id)
    {
      return Err(StructureBuildError {
        line: 0,
        message: format!("duplicate polymer entity {}", entity.id),
      });
    }
    self.structure.polymer_entities.push(entity);
    Ok(())
  }

  /// Adds one declared sequence position to a polymer entity.
  pub(crate) fn add_polymer_sequence(
    &mut self,
    entity_id: String,
    sequence: PolymerSequenceResidue,
  ) -> Result<(), StructureBuildError> {
    let Some(entity) = self
      .structure
      .polymer_entities
      .iter_mut()
      .find(|entity| entity.id == entity_id)
    else {
      let entity_index = self.structure.polymer_entities.len();
      self.structure.polymer_entities.push(PolymerEntity {
        id: entity_id,
        polymer_type: PolymerType::Other("unknown".to_owned()),
        sequence: Vec::new(),
        chain_ids: Vec::new(),
      });
      self.structure.polymer_entities[entity_index].sequence.push(sequence);
      return Ok(());
    };
    if entity.sequence.iter().any(|entry| entry.number == sequence.number) {
      return Err(StructureBuildError {
        line: 0,
        message: format!(
          "duplicate sequence position {} for entity {}",
          sequence.number, entity.id
        ),
      });
    }
    entity.sequence.push(sequence);
    Ok(())
  }

  /// Queues a serial-addressed relation for deferred atom-ID resolution.
  pub(crate) fn add_serial_bond(&mut self, bond: ProjectedSerialBond) {
    self.pending_bonds.push(bond);
  }

  /// Ensures that a model and its coordinate vector exist before an atom is added.
  ///
  /// # Returns
  ///
  /// The active model ID. Files without MODEL records receive one implicit
  /// model numbered one.
  pub(crate) fn ensure_model(&mut self) -> ModelId {
    if let Some(model_id) = self.current_model {
      return model_id;
    }
    self.create_model(1)
  }

  /// Creates a dense model ID and a coordinate vector aligned with all atoms.
  ///
  /// # Parameters
  ///
  /// * `source_number` is the model number declared by the source.
  ///
  /// # Returns
  ///
  /// The newly allocated dense model ID.
  pub(crate) fn create_model(&mut self, source_number: i32) -> ModelId {
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

  /// Starts a projected model, replacing an empty implicit model when present.
  ///
  /// The builder initially creates no model, so the first projected model is
  /// allocated directly. The replacement branch prevents a projection-created
  /// empty implicit model from leaving an unused coordinate set.
  ///
  /// # Parameters
  ///
  /// * `source_number` is the source MODEL number.
  pub(crate) fn start_model(&mut self, source_number: i32) {
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
  pub(crate) fn finish_model(&mut self) {
    self.current_model = None;
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
  pub(crate) fn ensure_chain(
    &mut self,
    model_id: ModelId,
    auth_id: Option<String>,
    label_id: Option<String>,
    entity_id: Option<String>,
  ) -> ChainId {
    let key = ChainKey {
      auth_id: auth_id.clone(),
      label_id: label_id.clone(),
    };
    if let Some(&chain_id) = self.chain_ids.get(&key) {
      if !self.structure.models[model_id.index()].chain_ids.contains(&chain_id) {
        self.structure.models[model_id.index()].chain_ids.push(chain_id);
      }
      self.attach_chain_entity(chain_id, entity_id.as_deref());
      return chain_id;
    }
    let chain_id = ChainId::from_index(self.structure.chains.len());
    self.chain_ids.insert(key, chain_id);
    let chain_entity_id = entity_id.clone();
    self.structure.chains.push(Chain {
      auth_id,
      label_id,
      entity_id,
      residue_ids: Vec::new(),
    });
    self.structure.models[model_id.index()].chain_ids.push(chain_id);
    self.attach_chain_entity(chain_id, chain_entity_id.as_deref());
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
  pub(crate) fn ensure_residue(
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

  /// Finalizes deferred bonds and annotations, then checks structure invariants.
  ///
  /// # Returns
  ///
  /// The completed structure and recoverable diagnostics, or an error when a
  /// builder invariant is broken.
  pub(crate) fn finish(mut self, strict: bool) -> Result<StructureParseResult, StructureBuildError> {
    if self.structure.models.is_empty() {
      // An END-only file is a valid empty snapshot and needs no implicit model.
    }
    self.finish_model();
    self.resolve_bonds(strict)?;
    self.resolve_named_bonds(strict)?;
    self.resolve_secondary_ranges(strict)?;
    self.resolve_polymer_sequences();
    self.resolve_missing_residues();
    self
      .structure
      .validate_invariants()
      .map_err(|error| StructureBuildError {
        line: 0,
        message: error.to_string(),
      })?;
    Ok(StructureParseResult {
      structure: self.structure,
      diagnostics: self.diagnostics,
    })
  }

  /// Resolves projected missing residues against normalized chains and models.
  pub(crate) fn resolve_missing_residues(&mut self) {
    let pending = std::mem::take(&mut self.pending_missing_residues);
    for missing in pending {
      let Some(chain_id) = self
        .structure
        .chains
        .iter()
        .position(|chain| chain.auth_id.as_deref() == Some(missing.chain_id.as_str()))
        .map(ChainId::from_index)
      else {
        continue;
      };
      for model in &self.structure.models {
        let already_present = self.structure.missing_polymer_residues.iter().any(|entry| {
          entry.model_id == model.id && entry.chain_id == chain_id && entry.sequence_number == missing.sequence_number
        });
        if !already_present {
          self.structure.missing_polymer_residues.push(MissingPolymerResidue {
            model_id: model.id,
            chain_id,
            sequence_number: missing.sequence_number,
            monomer: missing.monomer.clone(),
            hetero: missing.hetero,
          });
        }
      }
    }
  }

  /// Sorts declared sequences and records positions absent from each model.
  pub(crate) fn resolve_polymer_sequences(&mut self) {
    for entity in &mut self.structure.polymer_entities {
      entity.sequence.sort_by_key(|residue| residue.number);
    }

    let models = self.structure.models.clone();
    let entities = self.structure.polymer_entities.clone();
    for entity in entities {
      for chain_id in entity.chain_ids {
        let Some(label_id) = self.structure.chains[chain_id.index()].label_id.as_deref() else {
          continue;
        };
        for model in &models {
          for residue in &entity.sequence {
            let observed = self.observed_polymer_positions.contains(&(
              model.id,
              label_id.to_owned(),
              entity.id.clone(),
              residue.number,
            ));
            if !observed {
              self.structure.missing_polymer_residues.push(MissingPolymerResidue {
                model_id: model.id,
                chain_id,
                sequence_number: residue.number,
                monomer: residue.monomer.clone(),
                hetero: residue.hetero,
              });
            }
          }
        }
      }
    }
  }

  /// Resolves source serial numbers into atom IDs and deduplicates edges.
  ///
  /// # Parameters
  ///
  /// * `strict` controls whether references to unknown atom serials are fatal.
  ///
  /// # Returns
  ///
  /// `Ok(())` after all resolvable edges are stored, or a strict-mode parse
  /// error for an unresolved reference.
  pub(crate) fn resolve_bonds(&mut self, strict: bool) -> Result<(), StructureBuildError> {
    let pending_bonds = std::mem::take(&mut self.pending_bonds);
    for pending in pending_bonds {
      let Some(&source) = self.serial_ids.get(&pending.source) else {
        self.deferred_warning(
          strict,
          pending.line,
          pending.unresolved_code,
          format!("bond source serial {} was not found", pending.source),
        )?;
        continue;
      };
      let Some(&target) = self.serial_ids.get(&pending.target) else {
        self.deferred_warning(
          strict,
          pending.line,
          pending.unresolved_code,
          format!("bond target serial {} was not found", pending.target),
        )?;
        continue;
      };
      if source == target {
        continue;
      }
      let (a, b) = if source.index() < target.index() {
        (source, target)
      } else {
        (target, source)
      };
      if !self.structure.bonds.iter().any(|bond| bond.a == a && bond.b == b) {
        self.structure.bonds.push(Bond {
          a,
          b,
          order: BondOrder::Unknown,
          source: pending.bond_source,
        });
      }
    }
    Ok(())
  }

  /// Resolves projected label/author atom aliases for named bond relations.
  ///
  /// # Parameters
  ///
  /// * `strict` controls whether an endpoint absent from the atom table is
  ///   fatal.
  ///
  /// # Returns
  ///
  /// `Ok(())` after all resolvable relations are stored, or a strict-mode
  /// parse error for an unresolved endpoint.
  pub(crate) fn resolve_named_bonds(&mut self, strict: bool) -> Result<(), StructureBuildError> {
    let pending_bonds = std::mem::take(&mut self.pending_named_bonds);
    for pending in pending_bonds {
      let Some(&first) = self.lookup_ids.get(&pending.first) else {
        self.deferred_warning(
          strict,
          pending.line,
          pending.unresolved_code,
          "first named bond atom was not found".to_owned(),
        )?;
        continue;
      };
      let Some(&second) = self.lookup_ids.get(&pending.second) else {
        self.deferred_warning(
          strict,
          pending.line,
          pending.unresolved_code,
          "second named bond atom was not found".to_owned(),
        )?;
        continue;
      };
      if first == second {
        continue;
      }
      let (a, b) = if first.index() < second.index() {
        (first, second)
      } else {
        (second, first)
      };
      if !self.structure.bonds.iter().any(|bond| bond.a == a && bond.b == b) {
        self.structure.bonds.push(Bond {
          a,
          b,
          order: BondOrder::Unknown,
          source: pending.bond_source,
        });
      }
    }
    Ok(())
  }

  /// Resolves projected interval endpoints and annotates residues in each range.
  ///
  /// # Parameters
  ///
  /// * `strict` controls whether an endpoint that is absent from the atom
  ///   records is fatal.
  ///
  /// # Returns
  ///
  /// `Ok(())` after valid ranges are stored and residue annotations applied.
  pub(crate) fn resolve_secondary_ranges(&mut self, strict: bool) -> Result<(), StructureBuildError> {
    let pending_ranges = std::mem::take(&mut self.pending_secondary_ranges);
    for pending in pending_ranges {
      let Some(start) = self.find_residue(
        pending.chain_id.as_deref(),
        pending.start_sequence_number,
        pending.start_insertion_code,
      ) else {
        self.deferred_warning(
          strict,
          pending.line,
          pending.unresolved_code,
          "secondary-structure start residue was not found".to_owned(),
        )?;
        continue;
      };
      let Some(end) = self.find_residue(
        pending.chain_id.as_deref(),
        pending.end_sequence_number,
        pending.end_insertion_code,
      ) else {
        self.deferred_warning(
          strict,
          pending.line,
          pending.unresolved_code,
          "secondary-structure end residue was not found".to_owned(),
        )?;
        continue;
      };
      if self.structure.residues[start.index()].chain_id != self.structure.residues[end.index()].chain_id {
        self.deferred_warning(
          strict,
          pending.line,
          pending.unresolved_code,
          "secondary-structure range crosses chains".to_owned(),
        )?;
        continue;
      }
      let chain_id = self.structure.residues[start.index()].chain_id;
      let residue_ids = self.structure.chains[chain_id.index()].residue_ids.clone();
      let start_position = residue_ids.iter().position(|id| *id == start);
      let end_position = residue_ids.iter().position(|id| *id == end);
      let (Some(start_position), Some(end_position)) = (start_position, end_position) else {
        continue;
      };
      let (first, last) = if start_position <= end_position {
        (start_position, end_position)
      } else {
        (end_position, start_position)
      };
      for residue_id in &residue_ids[first..=last] {
        self.structure.residues[residue_id.index()].secondary_structure = pending.kind;
      }
      self.structure.secondary_ranges.push(SecondaryRange {
        chain_id,
        start,
        end,
        kind: pending.kind,
        source: AnnotationSource::File,
      });
    }
    Ok(())
  }

  /// Finds a residue by author chain, sequence number, and insertion code.
  pub(crate) fn find_residue(
    &self,
    chain_id: Option<&str>,
    sequence_number: i32,
    insertion_code: Option<char>,
  ) -> Option<ResidueId> {
    let residue = self
      .chains_for_id(chain_id)
      .into_iter()
      .flat_map(|chain| chain.residue_ids.iter().copied())
      .find(|residue_id| {
        let residue = &self.structure.residues[residue_id.index()];
        residue.sequence_number == sequence_number && residue.insertion_code == insertion_code
      });
    residue.or_else(|| {
      self.lookup_ids.iter().find_map(|(key, atom_id)| {
        (key.chain_id.as_deref() == chain_id
          && key.sequence_number == sequence_number
          && key.insertion_code == insertion_code)
          .then(|| self.structure.atoms[atom_id.index()].residue_id)
      })
    })
  }

  /// Returns chains matching an author identifier.
  pub(crate) fn chains_for_id(&self, chain_id: Option<&str>) -> Vec<&Chain> {
    self
      .structure
      .chains
      .iter()
      .filter(|chain| chain.auth_id.as_deref() == chain_id)
      .collect()
  }

  /// Records a deferred warning or converts it into a strict parse failure.
  pub(crate) fn deferred_warning(
    &mut self,
    strict: bool,
    line: usize,
    code: &'static str,
    message: String,
  ) -> Result<(), StructureBuildError> {
    if strict {
      return Err(StructureBuildError { line, message });
    }
    self.diagnostics.push(Diagnostic {
      code,
      line,
      severity: DiagnosticSeverity::Warning,
      message,
    });
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::structure::projection::ProjectedAtom;

  #[test]
  fn build_structure_should_accept_format_neutral_atom_input() {
    let mut input = StructureInput::default();
    input.add_atom(ProjectedAtom {
      line: 1,
      serial: Some(1),
      name: "CA".to_owned(),
      element: Some(Element("C".to_owned())),
      chain_id: Some("A".to_owned()),
      residue_name: "ALA".to_owned(),
      sequence_number: 1,
      insertion_code: None,
      altloc: None,
      occupancy: Some(1.0),
      b_factor: Some(10.0),
      formal_charge: None,
      position: [1.0, 2.0, 3.0],
      kind: ResidueKind::Polymer,
      label_chain_id: None,
      label_sequence_number: None,
      label_atom_name: None,
      label_entity_id: None,
    });

    let parsed =
      build_structure(input, false).unwrap_or_else(|error| panic!("format-neutral projection should build: {error}"));

    assert_eq!(parsed.structure.atom_count(), 1);
  }
}
