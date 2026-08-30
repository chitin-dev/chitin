//! Format-neutral input produced by structure-format projections.
//!
//! Parsers preserve source syntax in their own modules. Their projection
//! layers translate that syntax into the types in this module, and the shared
//! structure builder consumes the resulting [`StructureInput`]. Consequently,
//! neither PDB fixed columns nor mmCIF category names cross the builder
//! boundary.

use super::error::Diagnostic;
use super::model::{
  AssemblyGeneration, BiologicalAssembly, BondSource, Element, PolymerEntity, PolymerSequenceResidue, ResidueKind,
  SecondaryStructure, StructureOperation, Symmetry, UnitCell,
};

/// Semantic input from one source file before dense structure IDs are assigned.
///
/// A projection may fill these collections in source order. The shared builder
/// later resolves references, deduplicates topology shared by coordinate
/// models, and validates the final [`super::Structure`].
#[derive(Debug, Default)]
pub(crate) struct StructureInput {
  /// Optional source classification or descriptive header.
  pub(crate) classification: Option<String>,
  /// Optional source structure identifier.
  pub(crate) identifier: Option<String>,
  /// Crystallographic unit-cell parameters.
  pub(crate) unit_cell: Option<UnitCell>,
  /// Crystallographic space-group declaration.
  pub(crate) symmetry: Option<Symmetry>,
  /// Coordinate models in source order.
  pub(crate) models: Vec<ProjectedModel>,
  /// Associations from normalized label-chain IDs to entity IDs.
  pub(crate) label_chain_entities: Vec<(String, String)>,
  /// Polymer entities declared by the source.
  pub(crate) polymer_entities: Vec<PolymerEntity>,
  /// Sequence rows attached to an entity after entity declarations are read.
  pub(crate) polymer_sequences: Vec<ProjectedPolymerSequence>,
  /// Bonds addressed through source atom serial numbers.
  pub(crate) serial_bonds: Vec<ProjectedSerialBond>,
  /// Bonds addressed through normalized atom identity fields.
  pub(crate) named_bonds: Vec<ProjectedNamedBond>,
  /// Secondary-structure ranges awaiting residue-ID resolution.
  pub(crate) secondary_ranges: Vec<ProjectedSecondaryRange>,
  /// Explicit residues declared absent from source coordinates.
  pub(crate) missing_residues: Vec<ProjectedMissingResidue>,
  /// Coordinate operations used by biological assemblies.
  pub(crate) structure_operations: Vec<StructureOperation>,
  /// Biological assembly descriptions and generation rules.
  pub(crate) biological_assemblies: Vec<BiologicalAssembly>,
  /// Generation rules whose assembly references are resolved by the builder.
  pub(crate) assembly_generations: Vec<ProjectedAssemblyGeneration>,
  /// Recoverable source-format diagnostics.
  pub(crate) diagnostics: Vec<Diagnostic>,
  current_model: Option<usize>,
}

impl StructureInput {
  /// Starts a coordinate model and makes it the destination for subsequent atoms.
  pub(crate) fn start_model(&mut self, source_number: i32) {
    self.models.push(ProjectedModel {
      source_number,
      atoms: Vec::new(),
    });
    self.current_model = Some(self.models.len() - 1);
  }

  /// Selects an existing source model or creates it when first encountered.
  ///
  /// mmCIF atom-site rows are not required to be contiguous by model number,
  /// so returning to a previously seen number must append to that model rather
  /// than creating a duplicate coordinate model.
  pub(crate) fn start_or_reuse_model(&mut self, source_number: i32) {
    let model_index = match self
      .models
      .iter()
      .position(|model| model.source_number == source_number)
    {
      Some(index) => index,
      None => {
        self.models.push(ProjectedModel {
          source_number,
          atoms: Vec::new(),
        });
        self.models.len() - 1
      }
    };
    self.current_model = Some(model_index);
  }

  /// Ends the current coordinate model.
  pub(crate) fn finish_model(&mut self) {
    self.current_model = None;
  }

  /// Appends an atom to the current model, creating implicit model 1 when needed.
  pub(crate) fn add_atom(&mut self, atom: ProjectedAtom) {
    let model_index = match self.current_model {
      Some(index) => index,
      None => {
        self.start_model(1);
        self.models.len() - 1
      }
    };
    self.models[model_index].atoms.push(atom);
  }

  /// Associates a label-chain identifier with a polymer entity.
  pub(crate) fn add_label_chain_entity(&mut self, label_id: String, entity_id: String) {
    self.label_chain_entities.push((label_id, entity_id));
  }

  /// Adds a projected polymer entity declaration.
  pub(crate) fn add_polymer_entity(&mut self, entity: PolymerEntity) {
    self.polymer_entities.push(entity);
  }

  /// Adds one projected polymer sequence row.
  pub(crate) fn add_polymer_sequence(&mut self, entity_id: String, residue: PolymerSequenceResidue) {
    self
      .polymer_sequences
      .push(ProjectedPolymerSequence { entity_id, residue });
  }

  /// Adds a projected biological-assembly operation.
  pub(crate) fn add_structure_operation(&mut self, operation: StructureOperation) {
    self.structure_operations.push(operation);
  }

  /// Adds a projected biological assembly declaration.
  pub(crate) fn add_biological_assembly(&mut self, assembly: BiologicalAssembly) {
    self.biological_assemblies.push(assembly);
  }

  /// Adds a generation rule addressed by its source assembly identifier.
  pub(crate) fn add_assembly_generation(&mut self, assembly_id: String, generation: AssemblyGeneration) {
    self.assembly_generations.push(ProjectedAssemblyGeneration {
      assembly_id,
      generation,
    });
  }

  /// Adds a projected named-atom bond for deferred reference resolution.
  ///
  /// # Parameters
  ///
  /// * `line` identifies the source row for diagnostics.
  /// * `first` and `second` contain normalized endpoint identities.
  /// * `bond_source` records the relation's provenance in the final model.
  /// * `unresolved_code` is emitted if either endpoint is absent.
  ///
  /// # Returns
  ///
  /// This method stores the relation and returns no value; endpoint resolution
  /// occurs only after every atom has been indexed.
  pub(crate) fn add_named_bond(
    &mut self,
    line: usize,
    first: AtomLookupKey,
    second: AtomLookupKey,
    bond_source: BondSource,
    unresolved_code: &'static str,
  ) {
    self.named_bonds.push(ProjectedNamedBond {
      line,
      first,
      second,
      bond_source,
      unresolved_code,
    });
  }

  /// Adds a projected secondary-structure range.
  pub(crate) fn add_secondary_range(&mut self, range: ProjectedSecondaryRange) {
    self.secondary_ranges.push(range);
  }
}

/// One coordinate model projected from a source file.
#[derive(Debug)]
pub(crate) struct ProjectedModel {
  /// Model number declared by the source format.
  pub(crate) source_number: i32,
  /// Atom sites belonging to this model.
  pub(crate) atoms: Vec<ProjectedAtom>,
}

/// Format-neutral semantic fields for one atom site.
///
/// The author and label identifiers are retained as aliases because mmCIF may
/// use either namespace in cross-category references. PDB projection normally
/// supplies only author identifiers.
#[derive(Debug)]
pub(crate) struct ProjectedAtom {
  /// One-based source line or category row used for diagnostics.
  pub(crate) line: usize,
  /// Optional source atom serial number.
  pub(crate) serial: Option<i32>,
  /// Author-preferred atom name.
  pub(crate) name: String,
  /// Chemical element when declared by the source.
  pub(crate) element: Option<Element>,
  /// Author chain identifier.
  pub(crate) chain_id: Option<String>,
  /// Residue or chemical-component name.
  pub(crate) residue_name: String,
  /// Author-preferred residue sequence number.
  pub(crate) sequence_number: i32,
  /// Residue insertion code.
  pub(crate) insertion_code: Option<char>,
  /// Alternate conformer identifier.
  pub(crate) altloc: Option<char>,
  /// Crystallographic occupancy.
  pub(crate) occupancy: Option<f32>,
  /// Isotropic displacement or B-factor.
  pub(crate) b_factor: Option<f32>,
  /// Formal atomic charge.
  pub(crate) formal_charge: Option<i8>,
  /// Cartesian coordinates in angstroms.
  pub(crate) position: [f32; 3],
  /// Polymer or heterogen classification.
  pub(crate) kind: ResidueKind,
  /// mmCIF label-chain alias, when available.
  pub(crate) label_chain_id: Option<String>,
  /// mmCIF label-sequence alias, when available.
  pub(crate) label_sequence_number: Option<i32>,
  /// mmCIF label-atom alias, when available.
  pub(crate) label_atom_name: Option<String>,
  /// Entity identifier associated with this atom site.
  pub(crate) label_entity_id: Option<String>,
}

/// Normalized atom identity used by projected cross-references.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct AtomLookupKey {
  /// Chain identifier in either the author or label namespace.
  pub(crate) chain_id: Option<String>,
  /// Residue sequence number in the selected namespace.
  pub(crate) sequence_number: i32,
  /// Atom name in the selected namespace.
  pub(crate) atom_name: String,
  /// Residue insertion code.
  pub(crate) insertion_code: Option<char>,
  /// Alternate conformer identifier.
  pub(crate) altloc: Option<char>,
}

/// A source bond whose endpoints use integer atom serials.
#[derive(Debug)]
pub(crate) struct ProjectedSerialBond {
  /// Source location used for diagnostics.
  pub(crate) line: usize,
  /// Source endpoint serial.
  pub(crate) source: i32,
  /// Target endpoint serial.
  pub(crate) target: i32,
  /// Provenance retained on the generated bond.
  pub(crate) bond_source: BondSource,
  /// Diagnostic code used when either endpoint cannot be resolved.
  pub(crate) unresolved_code: &'static str,
}

/// A source bond whose endpoints use normalized atom identities.
#[derive(Debug)]
pub(crate) struct ProjectedNamedBond {
  /// Source location used for diagnostics.
  pub(crate) line: usize,
  /// First endpoint.
  pub(crate) first: AtomLookupKey,
  /// Second endpoint.
  pub(crate) second: AtomLookupKey,
  /// Provenance retained on the generated bond.
  pub(crate) bond_source: BondSource,
  /// Diagnostic code used when either endpoint cannot be resolved.
  pub(crate) unresolved_code: &'static str,
}

/// A secondary-structure range before residue IDs are assigned.
#[derive(Debug)]
pub(crate) struct ProjectedSecondaryRange {
  /// Source location used for diagnostics.
  pub(crate) line: usize,
  /// Chain containing both endpoints.
  pub(crate) chain_id: Option<String>,
  /// First residue sequence number.
  pub(crate) start_sequence_number: i32,
  /// First residue insertion code.
  pub(crate) start_insertion_code: Option<char>,
  /// Last residue sequence number.
  pub(crate) end_sequence_number: i32,
  /// Last residue insertion code.
  pub(crate) end_insertion_code: Option<char>,
  /// Secondary-structure classification applied to the range.
  pub(crate) kind: SecondaryStructure,
  /// Diagnostic code used when projected endpoints cannot be resolved.
  pub(crate) unresolved_code: &'static str,
}

/// A polymer residue declared by the source but absent from coordinates.
#[derive(Debug)]
pub(crate) struct ProjectedMissingResidue {
  /// Author or normalized chain identifier.
  pub(crate) chain_id: String,
  /// Declared residue sequence number.
  pub(crate) sequence_number: i32,
  /// Declared chemical-component name.
  pub(crate) monomer: String,
  /// Whether the sequence row is heterogeneous.
  pub(crate) hetero: bool,
}

/// A declared polymer sequence row associated with an entity.
#[derive(Debug)]
pub(crate) struct ProjectedPolymerSequence {
  /// Entity receiving the sequence row.
  pub(crate) entity_id: String,
  /// Declared sequence position.
  pub(crate) residue: PolymerSequenceResidue,
}

/// A biological-assembly generation attached by source assembly identifier.
#[derive(Debug)]
pub(crate) struct ProjectedAssemblyGeneration {
  /// Assembly receiving the generation rule.
  pub(crate) assembly_id: String,
  /// Chains and operations used to generate assembly instances.
  pub(crate) generation: AssemblyGeneration,
}
