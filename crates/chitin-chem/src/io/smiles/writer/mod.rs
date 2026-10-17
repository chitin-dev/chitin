//! Noncanonical serialization: semantic checks, traversal plan, then emission.
//!
//! This layer borrows graph facts only. It does not inspect a reader snapshot,
//! infer properties, renumber graph handles, or install caches. Atom spelling
//! and traversal are private boundaries for future stereo-aware encoding.

use crate::{AtomId, BondId, BondOrder, GraphError, MolecularGraph};

mod atom;
mod traversal;

/// A supplied atomic fact cannot survive the current strict SMILES round trip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SmilesAtomWriteError {
  /// Unknown charge cannot be encoded as the SMILES default of neutral.
  #[error("formal charge is unknown")]
  UnknownFormalCharge,
  /// Unknown attached H cannot be encoded as a known zero count.
  #[error("declared attached-H count is unknown")]
  UnknownDeclaredHydrogens,
  /// An unspecified H policy cannot be converted to inference or fixed zero.
  #[error("implicit-H policy is unspecified")]
  UnknownHydrogenPolicy,
  /// Only the reader's neutral, unlabelled organic-subset facts permit bare atoms.
  #[error("implicit-H inference is not representable for these atomic facts")]
  ImplicitInference,
  /// Fixed implicit H cannot be merged into the separate declared-H attribute.
  #[error("fixed implicit-H count {0} cannot be retained as implicit H")]
  FixedImplicitHydrogens(u8),
  /// The current bracket reader cannot recover a supplied radical count, even zero.
  #[error("explicit unpaired-electron count {0} cannot be retained by the current bracket codec")]
  ExplicitRadicalState(u8),
  /// The isotope exceeds the shared reader/writer lexical profile.
  #[error("isotope mass number {0} exceeds the supported spelling")]
  IsotopeOutOfRange(u16),
  /// The attached-H count exceeds the shared lexical profile.
  #[error("declared attached-H count {0} exceeds the supported spelling")]
  HydrogenCountOutOfRange(u8),
  /// The charge exceeds the shared lexical profile.
  #[error("formal charge {0} exceeds the supported spelling")]
  ChargeOutOfRange(i16),
  /// Bracket syntax does not permit a hydrogen atom to declare attached H.
  #[error("a hydrogen atom cannot declare attached H")]
  HydrogenAsHost,
}

/// Serialization failure with graph-local handles, not source-text offsets.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SmilesWriteError {
  /// An atomic fact would be lost or changed by this codec.
  #[error("cannot write atom {atom}: {reason}")]
  Atom {
    /// Handle in the input graph's namespace.
    atom: AtomId,
    /// The unsupported semantic or representation boundary.
    #[source]
    reason: SmilesAtomWriteError,
  },
  /// An edge is not a supported single/double/triple bond.
  #[error("cannot write bond {bond}: unsupported order {order:?}")]
  Bond {
    /// Handle of the offending input edge.
    bond: BondId,
    /// Supplied order, never replaced with a single bond.
    order: BondOrder,
  },
  /// The chosen traversal needs more simultaneously open labels than the profile.
  #[error("ring-label capacity exceeded at atom {atom}")]
  RingLabelCapacity {
    /// Atom where no free label remained; a different traversal may need fewer.
    atom: AtomId,
  },
  /// An internal ring-site pairing invariant failed; indicates a writer bug.
  #[error("writer ring-site invariant failed at atom {atom}")]
  TraversalInvariant {
    /// Atom whose closing site had no earlier opening in the traversal.
    atom: AtomId,
  },
  /// The input contains an inconsistent topology reference.
  #[error(transparent)]
  Topology(#[from] GraphError),
}

/// Writes deterministic, noncanonical SMILES without changing supplied facts.
///
/// # Parameters
/// - `graph`: graph facts only; reader provenance and source text are not needed.
///
/// # Returns
/// One SMILES string, or an empty string for an empty graph. The same graph
/// produces the same text; atom insertion order may affect it. No canonical
/// labeling, chemical normalization, valence validation, or H inference runs.
/// Reading the result with [`super::read_smiles`] preserves all current atomic
/// attributes and bond orders up to graph isomorphism, not graph-local IDs.
/// Explicit H vertices stay vertices; fixed implicit H is not merged with
/// declared H. Traversal and output are iterative, including deeply nested trees.
///
/// # Errors
/// Rejects missing/unrepresentable facts, unknown/aromatic bond orders, and a
/// traversal requiring more than 100 simultaneously open ring labels. Closed
/// labels are reused, so total cycle count is not limited to 100. A known
/// bracket radical count (including zero) is rejected because the current
/// reader cannot recover it. No partial output is returned on failure.
///
/// # Examples
/// ```
/// use chitin_chem::io::smiles::{read_smiles, write_smiles};
/// let parsed = read_smiles("CC(O)C.[NH4+]")?;
/// let text = write_smiles(parsed.graph())?;
/// let reread = read_smiles(&text)?;
/// assert_eq!(reread.graph().atom_count(), 5);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn write_smiles(graph: &MolecularGraph) -> Result<String, SmilesWriteError> {
  let plan = traversal::plan(graph)?;
  traversal::emit(plan)
}
