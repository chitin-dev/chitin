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

/// The normalized atom name from a PDB atom record.
pub type AtomName = String;

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
  pub name: AtomName,
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
  /// Residues in source order.
  pub residue_ids: Vec<ResidueId>,
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

/// Origin of a bond relationship.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BondSource {
  /// Explicit PDB CONECT record.
  Conect,
  /// Explicit mmCIF _struct_conn relation.
  StructConn,
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
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StructureMetadata {
  /// Optional PDB HEADER classification.
  pub classification: Option<String>,
  /// Optional four-character source identifier.
  pub identifier: Option<String>,
}

/// Indexed, renderer-independent molecular structure data.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Structure {
  /// Models in source order.
  pub models: Vec<Model>,
  /// Chains indexed by ChainId.
  pub chains: Vec<Chain>,
  /// Residues indexed by ResidueId.
  pub residues: Vec<Residue>,
  /// Atoms indexed by AtomId.
  pub atoms: Vec<Atom>,
  /// Explicit or inferred bonds.
  pub bonds: Vec<Bond>,
  /// Coordinate sets indexed by CoordinateSetId.
  pub coordinates: Vec<CoordinateSet>,
  /// File-level metadata.
  pub metadata: StructureMetadata,
  /// Secondary-structure intervals.
  pub secondary_ranges: Vec<SecondaryRange>,
}

impl Structure {
  /// Returns the number of atoms in the structure.
  pub fn atom_count(&self) -> usize {
    self.atoms.len()
  }

  /// Returns a coordinate set by its typed identifier.
  pub fn coordinate_set(&self, id: CoordinateSetId) -> Option<&CoordinateSet> {
    self.coordinates.get(id.index())
  }

  /// Validates all cross-table indices and coordinate lengths.
  ///
  /// # Returns
  ///
  /// `Ok(())` when every ID points into its owning table. Otherwise returns a
  /// description of the first broken invariant.
  ///
  /// # Examples
  ///
  /// ```
  /// use chitin_bio::structure::Structure;
  ///
  /// let structure = Structure {
  ///   models: Vec::new(),
  ///   chains: Vec::new(),
  ///   residues: Vec::new(),
  ///   atoms: Vec::new(),
  ///   bonds: Vec::new(),
  ///   coordinates: Vec::new(),
  ///   metadata: Default::default(),
  ///   secondary_ranges: Vec::new(),
  /// };
  /// assert!(structure.validate_invariants().is_ok());
  /// ```
  pub fn validate_invariants(&self) -> Result<(), String> {
    for (model_index, model) in self.models.iter().enumerate() {
      if model.coordinate_set_id.index() >= self.coordinates.len() {
        return Err(format!("model {model_index} references missing coordinates"));
      }
      if model
        .chain_ids
        .iter()
        .any(|chain_id| chain_id.index() >= self.chains.len())
      {
        return Err(format!("model {model_index} references a missing chain"));
      }
    }

    for (chain_index, chain) in self.chains.iter().enumerate() {
      if chain
        .residue_ids
        .iter()
        .any(|residue_id| residue_id.index() >= self.residues.len())
      {
        return Err(format!("chain {chain_index} references a missing residue"));
      }
    }

    for (residue_index, residue) in self.residues.iter().enumerate() {
      if residue.chain_id.index() >= self.chains.len() {
        return Err(format!("residue {residue_index} references missing chain"));
      }
      if residue
        .atom_ids
        .iter()
        .any(|atom_id| atom_id.index() >= self.atoms.len())
      {
        return Err(format!("residue {residue_index} references a missing atom"));
      }
    }

    for bond in &self.bonds {
      if bond.a.index() >= self.atoms.len() || bond.b.index() >= self.atoms.len() {
        return Err("bond references an atom outside the atom table".to_owned());
      }
    }

    for (range_index, range) in self.secondary_ranges.iter().enumerate() {
      if range.start.index() >= self.residues.len() || range.end.index() >= self.residues.len() {
        return Err(format!(
          "secondary-structure range {range_index} references a missing residue"
        ));
      }
      if self.residues[range.start.index()].chain_id != self.residues[range.end.index()].chain_id {
        return Err(format!("secondary-structure range {range_index} crosses chains"));
      }
    }

    for (coordinate_index, coordinates) in self.coordinates.iter().enumerate() {
      if coordinates.positions.len() != self.atoms.len() {
        return Err(format!(
          "coordinate set {coordinate_index} has {} positions for {} atoms",
          coordinates.positions.len(),
          self.atoms.len()
        ));
      }
    }

    Ok(())
  }
}
