use thiserror::Error;

macro_rules! id_type {
  ($name:ident) => {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub struct $name(u32);

    impl $name {
      pub(crate) const fn from_index(index: usize) -> Self {
        Self(index as u32)
      }

      /// Returns the dense table index represented by this identifier.
      pub const fn index(self) -> usize {
        self.0 as usize
      }
    }
  };
}

id_type!(AtomId);
id_type!(ResidueId);
id_type!(ChainId);
id_type!(ModelId);
id_type!(CoordinateSetId);

/// The element inferred from the PDB element column when present.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Element(pub String);

/// Classification of a residue based on the source record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResidueKind {
  /// A standard or modified polymer residue from an ATOM record.
  Polymer,
  /// A non-polymer residue from a HETATM record.
  Hetero,
}

/// A parsed atom and its stable topology membership.
#[derive(Debug, Clone, PartialEq)]
pub struct Atom {
  /// Source serial number, if the record supplied one.
  pub serial: Option<i32>,
  /// Normalized atom name.
  pub name: String,
  /// Element from the source element field, when present.
  pub element: Option<Element>,
  /// Parent residue.
  pub residue_id: ResidueId,
  /// Alternate-location identifier, if present.
  pub altloc: Option<char>,
  /// Occupancy from the source record.
  pub occupancy: Option<f32>,
  /// Temperature factor or B-factor.
  pub b_factor: Option<f32>,
  /// Formal charge, when it can be parsed from the source field.
  pub formal_charge: Option<i8>,
}

/// A residue and its ordered atom range.
#[derive(Debug, Clone, PartialEq)]
pub struct Residue {
  /// Three-letter or source residue name.
  pub name: String,
  /// Parent chain.
  pub chain_id: ChainId,
  /// Author residue sequence number.
  pub sequence_number: i32,
  /// Insertion code, if present.
  pub insertion_code: Option<char>,
  /// Whether the source record describes polymer or hetero chemistry.
  pub kind: ResidueKind,
  /// Atoms belonging to the residue, in source order.
  pub atom_ids: Vec<AtomId>,
  /// Explicit secondary-structure annotation, if one is added later.
  pub secondary_structure: SecondaryStructure,
}

/// A chain within a model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chain {
  /// Author chain identifier. A blank chain is represented by `None`.
  pub auth_id: Option<String>,
  /// Label/asym chain identifier from mmCIF, when available.
  pub label_id: Option<String>,
  /// Polymer entity identifier associated with this chain, when available.
  pub entity_id: Option<String>,
  /// Residues in source order.
  pub residue_ids: Vec<ResidueId>,
}

/// The polymer family declared by an mmCIF entity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolymerType {
  /// A peptide or protein polymer.
  Polypeptide,
  /// A DNA polymer.
  Polydeoxyribonucleotide,
  /// An RNA polymer.
  Polyribonucleotide,
  /// A polymer type not covered by the common families.
  Other(String),
}

/// One position in an entity's declared polymer sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolymerSequenceResidue {
  /// One-based label sequence position from `_entity_poly_seq.num`.
  pub number: i32,
  /// Chemical component identifier from `_entity_poly_seq.mon_id`.
  pub monomer: String,
  /// Whether the source marks this position as a hetero polymer component.
  pub hetero: bool,
}

/// A declared polymer position without coordinates in one model and chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingPolymerResidue {
  /// Coordinate model in which the position is absent.
  pub model_id: ModelId,
  /// Chain containing the declared position.
  pub chain_id: ChainId,
  /// One-based label sequence position.
  pub sequence_number: i32,
  /// Chemical component identifier declared for the position.
  pub monomer: String,
  /// Whether the source marks this position as a hetero polymer component.
  pub hetero: bool,
}

/// A complete polymer entity and the chains that instantiate it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolymerEntity {
  /// Entity identifier from `_entity_poly.entity_id`.
  pub id: String,
  /// Polymer family from `_entity_poly.type`.
  pub polymer_type: PolymerType,
  /// Complete declared sequence, including positions without coordinates.
  pub sequence: Vec<PolymerSequenceResidue>,
  /// Chains mapped to this entity through `_struct_asym`.
  pub chain_ids: Vec<ChainId>,
}

/// A coordinate-bearing PDB model.
#[derive(Debug, Clone, PartialEq)]
pub struct Model {
  /// Internal dense model identifier.
  pub id: ModelId,
  /// Number from the source MODEL record, or one for an implicit model.
  pub source_number: i32,
  /// Chains belonging to this model.
  pub chain_ids: Vec<ChainId>,
  /// Coordinate set associated with this model.
  pub coordinate_set_id: CoordinateSetId,
}

/// Coordinates for one model, indexed by AtomId.
#[derive(Debug, Clone, PartialEq)]
pub struct CoordinateSet {
  /// Cartesian positions in ångström units. Missing positions are NaN.
  pub positions: Vec<[f32; 3]>,
}

/// A bond between two atoms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bond {
  /// First endpoint.
  pub a: AtomId,
  /// Second endpoint.
  pub b: AtomId,
  /// Bond order when known.
  pub order: BondOrder,
  /// Origin of the bond relationship.
  pub source: BondSource,
}

/// Bond order supported by the initial model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BondOrder {
  /// Bond order was not supplied.
  Unknown,
}

/// A cross-table invariant violation in an indexed [`Structure`].
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum StructureInvariantError {
  /// A model references a coordinate set outside the coordinate table.
  #[error("model {model_index} references missing coordinates")]
  ModelCoordinates { model_index: usize },
  /// A model references a chain outside the chain table.
  #[error("model {model_index} references a missing chain")]
  ModelChain { model_index: usize },
  /// A chain references a residue outside the residue table.
  #[error("chain {chain_index} references a missing residue")]
  ChainResidue { chain_index: usize },
  /// A polymer entity references a chain outside the chain table.
  #[error("polymer entity {entity_index} references a missing chain")]
  PolymerEntityChain { entity_index: usize },
  /// A polymer entity sequence is not strictly ordered by position.
  #[error("polymer entity {entity_index} sequence is not strictly ordered")]
  PolymerSequenceOrder { entity_index: usize },
  /// A residue references a chain outside the chain table.
  #[error("residue {residue_index} references missing chain")]
  ResidueChain { residue_index: usize },
  /// A residue references an atom outside the atom table.
  #[error("residue {residue_index} references a missing atom")]
  ResidueAtom { residue_index: usize },
  /// A bond references an atom outside the atom table.
  #[error("bond references an atom outside the atom table")]
  BondAtom,
  /// A secondary-structure range references a residue outside the residue table.
  #[error("secondary-structure range {range_index} references a missing residue")]
  SecondaryRangeResidue { range_index: usize },
  /// A secondary-structure range crosses two different chains.
  #[error("secondary-structure range {range_index} crosses chains")]
  SecondaryRangeChain { range_index: usize },
  /// A coordinate set has a position count different from the atom table.
  #[error("coordinate set {coordinate_index} has {positions} positions for {atoms} atoms")]
  CoordinateLength {
    /// Coordinate-set table index.
    coordinate_index: usize,
    /// Number of positions in the coordinate set.
    positions: usize,
    /// Number of atoms in the topology table.
    atoms: usize,
  },
}

/// Origin of a bond relationship.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BondSource {
  /// Explicit PDB CONECT record.
  Conect,
  /// Explicit mmCIF _struct_conn relation.
  StructConn,
  /// Metal-coordination relation declared by mmCIF `_struct_conn`.
  StructConnMetalCoordination,
  /// Inferred from polymer topology.
  PolymerInference,
  /// Inferred from a distance heuristic.
  DistanceInference,
}

/// Secondary-structure value attached to a residue or interval.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecondaryStructure {
  /// No explicit annotation is available.
  Unknown,
  /// Alpha or other helix annotation.
  Helix,
  /// Three-ten helix annotation.
  Helix310,
  /// Pi helix annotation.
  PiHelix,
  /// Beta-sheet annotation.
  Sheet,
  /// Explicit coil annotation.
  Coil,
}

/// Source of a secondary-structure annotation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnotationSource {
  /// Annotation was read from the source file.
  File,
  /// Annotation was inferred by a later analysis pass.
  Inferred,
}

/// A secondary-structure interval.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecondaryRange {
  /// Chain containing the interval.
  pub chain_id: ChainId,
  /// First residue in the interval.
  pub start: ResidueId,
  /// Last residue in the interval.
  pub end: ResidueId,
  /// Interval kind.
  pub kind: SecondaryStructure,
  /// Whether this came from the file or an analysis pass.
  pub source: AnnotationSource,
}

/// Metadata shared by all models in a structure.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StructureMetadata {
  /// Optional PDB HEADER classification.
  pub classification: Option<String>,
  /// Optional four-character source identifier.
  pub identifier: Option<String>,
  /// Unit-cell parameters, when supplied by the source format.
  pub unit_cell: Option<UnitCell>,
  /// Crystallographic space-group metadata, when supplied by the source format.
  pub symmetry: Option<Symmetry>,
  /// Biological assembly operations and generation rules.
  pub assembly: AssemblyMetadata,
}

/// Parameters describing a crystallographic unit cell.
#[derive(Debug, Clone, PartialEq)]
pub struct UnitCell {
  /// Cell edge lengths a, b, and c in ångströms.
  pub lengths: [f32; 3],
  /// Cell angles alpha, beta, and gamma in degrees.
  pub angles: [f32; 3],
}

/// Space-group identifiers declared by a structure file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symmetry {
  /// Hermann–Mauguin space-group name.
  pub space_group_name: Option<String>,
  /// International Tables number, when provided.
  pub international_tables_number: Option<i32>,
}

/// A rigid operation that can be applied when expanding a biological assembly.
#[derive(Debug, Clone, PartialEq)]
pub struct StructureOperation {
  /// Dictionary operation identifier.
  pub id: String,
  /// Rotation matrix in Cartesian coordinates.
  pub rotation: [[f32; 3]; 3],
  /// Translation vector in ångströms.
  pub translation: [f32; 3],
}

/// One row describing which chains an operator expression targets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssemblyGeneration {
  /// Label asym identifiers selected by this generation rule.
  pub asym_ids: Vec<String>,
  /// Author asym identifiers, when supplied by the source.
  pub auth_asym_ids: Vec<String>,
  /// Entity-instance identifiers, used by files that do not provide asym IDs.
  pub entity_instance_ids: Vec<String>,
  /// Raw operator expression, preserved for a later expansion pass.
  pub operator_expression: String,
}

/// A biological assembly definition without materialized duplicate coordinates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BiologicalAssembly {
  /// Assembly identifier.
  pub id: String,
  /// Optional source description.
  pub details: Option<String>,
  /// Rules used to select chains and operations for this assembly.
  pub generations: Vec<AssemblyGeneration>,
}

/// Assembly operations and biological assembly generation rules.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AssemblyMetadata {
  /// Operations indexed by their source identifiers.
  pub operations: Vec<StructureOperation>,
  /// Biological assemblies declared by the source file.
  pub assemblies: Vec<BiologicalAssembly>,
}

/// Indexed, renderer-independent molecular structure data.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Structure {
  /// Models in source order.
  pub(crate) models: Vec<Model>,
  /// Chains indexed by ChainId.
  pub(crate) chains: Vec<Chain>,
  /// Residues indexed by ResidueId.
  pub(crate) residues: Vec<Residue>,
  /// Atoms indexed by AtomId.
  pub(crate) atoms: Vec<Atom>,
  /// Explicit or inferred bonds.
  pub(crate) bonds: Vec<Bond>,
  /// Coordinate sets indexed by CoordinateSetId.
  pub(crate) coordinates: Vec<CoordinateSet>,
  /// File-level metadata.
  pub(crate) metadata: StructureMetadata,
  /// Declared polymer entities and their complete sequences.
  pub(crate) polymer_entities: Vec<PolymerEntity>,
  /// Declared polymer positions missing from individual model/chain coordinate sets.
  pub(crate) missing_polymer_residues: Vec<MissingPolymerResidue>,
  /// Secondary-structure intervals.
  pub(crate) secondary_ranges: Vec<SecondaryRange>,
}

impl Structure {
  /// Returns the number of atoms in the structure.
  pub fn atom_count(&self) -> usize {
    self.atoms.len()
  }

  /// Returns the models in source order.
  pub fn models(&self) -> &[Model] {
    &self.models
  }

  /// Returns the normalized chain table.
  pub fn chains(&self) -> &[Chain] {
    &self.chains
  }

  /// Returns the normalized residue table.
  pub fn residues(&self) -> &[Residue] {
    &self.residues
  }

  /// Returns the normalized atom table.
  pub fn atoms(&self) -> &[Atom] {
    &self.atoms
  }

  /// Returns explicit and inferred bonds.
  pub fn bonds(&self) -> &[Bond] {
    &self.bonds
  }

  /// Returns coordinate sets indexed by [`CoordinateSetId`].
  pub fn coordinates(&self) -> &[CoordinateSet] {
    &self.coordinates
  }

  /// Returns file-level metadata.
  pub fn metadata(&self) -> &StructureMetadata {
    &self.metadata
  }

  /// Returns declared polymer entities.
  pub fn polymer_entities(&self) -> &[PolymerEntity] {
    &self.polymer_entities
  }

  /// Returns declared polymer positions absent from coordinate sets.
  pub fn missing_polymer_residues(&self) -> &[MissingPolymerResidue] {
    &self.missing_polymer_residues
  }

  /// Returns secondary-structure intervals.
  pub fn secondary_ranges(&self) -> &[SecondaryRange] {
    &self.secondary_ranges
  }

  /// Returns a coordinate set by its typed identifier.
  pub fn coordinate_set(&self, id: CoordinateSetId) -> Option<&CoordinateSet> {
    self.coordinates.get(id.index())
  }

  /// Validates all cross-table indices and coordinate lengths.
  ///
  /// # Returns
  ///
  /// `Ok(())` when every ID points into its owning table. Otherwise returns the
  /// first broken invariant.
  ///
  /// # Examples
  ///
  /// ```
  /// use chitin_bio::structure::Structure;
  ///
  /// let structure = Structure::default();
  /// assert!(structure.validate_invariants().is_ok());
  /// ```
  pub fn validate_invariants(&self) -> Result<(), StructureInvariantError> {
    for (model_index, model) in self.models.iter().enumerate() {
      if model.coordinate_set_id.index() >= self.coordinates.len() {
        return Err(StructureInvariantError::ModelCoordinates { model_index });
      }
      if model
        .chain_ids
        .iter()
        .any(|chain_id| chain_id.index() >= self.chains.len())
      {
        return Err(StructureInvariantError::ModelChain { model_index });
      }
    }

    for (chain_index, chain) in self.chains.iter().enumerate() {
      if chain
        .residue_ids
        .iter()
        .any(|residue_id| residue_id.index() >= self.residues.len())
      {
        return Err(StructureInvariantError::ChainResidue { chain_index });
      }
    }

    for (entity_index, entity) in self.polymer_entities.iter().enumerate() {
      if entity
        .chain_ids
        .iter()
        .any(|chain_id| chain_id.index() >= self.chains.len())
      {
        return Err(StructureInvariantError::PolymerEntityChain { entity_index });
      }
      for window in entity.sequence.windows(2) {
        if window[0].number >= window[1].number {
          return Err(StructureInvariantError::PolymerSequenceOrder { entity_index });
        }
      }
    }

    for (residue_index, residue) in self.residues.iter().enumerate() {
      if residue.chain_id.index() >= self.chains.len() {
        return Err(StructureInvariantError::ResidueChain { residue_index });
      }
      if residue
        .atom_ids
        .iter()
        .any(|atom_id| atom_id.index() >= self.atoms.len())
      {
        return Err(StructureInvariantError::ResidueAtom { residue_index });
      }
    }

    for bond in &self.bonds {
      if bond.a.index() >= self.atoms.len() || bond.b.index() >= self.atoms.len() {
        return Err(StructureInvariantError::BondAtom);
      }
    }

    for (range_index, range) in self.secondary_ranges.iter().enumerate() {
      if range.start.index() >= self.residues.len() || range.end.index() >= self.residues.len() {
        return Err(StructureInvariantError::SecondaryRangeResidue { range_index });
      }
      if self.residues[range.start.index()].chain_id != self.residues[range.end.index()].chain_id {
        return Err(StructureInvariantError::SecondaryRangeChain { range_index });
      }
    }

    for (coordinate_index, coordinates) in self.coordinates.iter().enumerate() {
      if coordinates.positions.len() != self.atoms.len() {
        return Err(StructureInvariantError::CoordinateLength {
          coordinate_index,
          positions: coordinates.positions.len(),
          atoms: self.atoms.len(),
        });
      }
    }

    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::{CoordinateSet, Structure, StructureInvariantError};

  #[test]
  fn validate_invariants_should_return_typed_coordinate_length_error() {
    let mut structure = Structure::default();
    structure.coordinates.push(CoordinateSet {
      positions: vec![[0.0, 0.0, 0.0]],
    });

    assert_eq!(
      structure.validate_invariants(),
      Err(StructureInvariantError::CoordinateLength {
        coordinate_index: 0,
        positions: 1,
        atoms: 0,
      })
    );
  }
}
