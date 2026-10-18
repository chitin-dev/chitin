//! Strict supplied-fact SMILES I/O, not yet a complete SMILES implementation.
//!
//! Syntax scanning and topology construction are separate from chemical
//! perception. Unsupported information is rejected rather than discarded.
//! See the crate's `docs/formats.md` for the compatibility and extension plan.

use std::{collections::BTreeMap, ops::Range};

use crate::{AtomId, BondId, ElementError, GraphError, IsotopeError, MolecularGraph};

mod parser;
mod profile;
mod syntax;
mod writer;

pub use writer::{SmilesAtomWriteError, SmilesWriteError, write_smiles};

/// A positioned read failure; offsets are zero-based UTF-8 byte offsets.
///
/// End-of-input errors use an empty range. A failed read never exposes the
/// partially constructed graph. Chemical model outcomes are not read errors.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{kind} at bytes {span:?}")]
pub struct SmilesError {
  /// Offending source range, or the location where a token was required.
  pub span: Range<usize>,
  /// Machine-readable failure category with its underlying cause.
  #[source]
  pub kind: SmilesErrorKind,
}

/// Read failures distinguish malformed syntax from missing codec capabilities.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SmilesErrorKind {
  /// The token sequence violates the supported grammar.
  #[error("invalid SMILES: {0}")]
  Syntax(&'static str),
  /// Recognized information cannot yet be represented without loss.
  #[error("unsupported SMILES feature: {0}")]
  Unsupported(&'static str),
  /// An integer exceeds the lexical/domain range; it was not truncated.
  #[error("{0} is out of range")]
  NumberOutOfRange(&'static str),
  /// The source does not name a known chemical element.
  #[error(transparent)]
  Element(#[from] ElementError),
  /// The isotope label cannot be represented by the current domain model.
  #[error(transparent)]
  Isotope(#[from] IsotopeError),
  /// Connectivity violates the simple-graph contract (e.g. duplicate edges).
  #[error(transparent)]
  Topology(#[from] GraphError),
  /// Ordered local stereo references do not match supported coordination.
  #[error(transparent)]
  Stereo(#[from] crate::StereoError),
}

/// Detached graph and source provenance from one successful SMILES read.
///
/// Only immutable graph access is exposed so source handles cannot become stale.
/// Consume with [`Self::into_graph`] to edit, deliberately dropping provenance.
/// Source offsets and IDs describe input order, not canonical atom ordering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSmiles {
  /// Exact immutable input used by all half-open byte ranges below.
  source: Box<str>,
  /// Supplied facts only; IDs index provenance within this snapshot's namespace.
  graph: MolecularGraph,
  /// One complete lexical atom token per graph vertex, including brackets.
  atom_spans: BTreeMap<AtomId, Range<usize>>,
  /// One ordinary bond site or two ring sites; omitted symbols use empty ranges.
  bond_spans: BTreeMap<BondId, Vec<Range<usize>>>,
}

impl ParsedSmiles {
  /// Returns the exact input, without trimming or normalization.
  pub fn source(&self) -> &str {
    &self.source
  }
  /// Returns the topology and supplied atomic facts, without inferred properties.
  pub const fn graph(&self) -> &MolecularGraph {
    &self.graph
  }
  /// Returns the input token for an atom in this graph's namespace.
  pub fn atom_span(&self, atom: AtomId) -> Option<Range<usize>> {
    self.atom_spans.get(&atom).cloned()
  }
  /// Returns bond source sites: two for ring closure, one for an ordinary edge.
  ///
  /// An omitted bond symbol has an empty range immediately before its next
  /// atom. Ring sites include any explicit bond symbol and the ring label.
  pub fn bond_spans(&self, bond: BondId) -> Option<&[Range<usize>]> {
    self.bond_spans.get(&bond).map(Vec::as_slice)
  }
  /// Consumes the read snapshot and releases its graph for independent editing.
  pub fn into_graph(self) -> MolecularGraph {
    self.graph
  }
}

/// Reads one strict SMILES string without perceiving aromaticity or radical state.
///
/// # Parameters
/// - `source`: SMILES only; whitespace, record names, and extensions are not ignored.
///
/// # Returns
/// A detached graph with source ranges. Empty input denotes an empty graph.
/// Bare organic-subset atoms permit implicit H and use a closed-shell reading;
/// bracket atoms disable implicit H and leave radical state unresolved. Missing
/// bracket charge/H mean explicit zero under SMILES rules, not unknown input.
/// Run a named perception model separately; parsing is not chemical validation.
/// Lowercase atomic designations and aromatic bonds are retained as supplied
/// facts. An omitted bond is aromatic only between two aromatic endpoints;
/// an explicit single bond always remains single, including ring closure.
/// Tetrahedral `@`/`@@` and `@TH1`/`@TH2` are stored with ordered ligand
/// identities, including ring-opening positions and any declared virtual H.
///
/// # Errors
/// Returns a positioned syntax, unsupported-feature, representation, or topology
/// error. Non-tetrahedral stereo, directional double-bond stereo,
/// wildcard/query atoms, atom classes, and
/// reaction/CXSMILES extensions are rejected, never silently stripped.
///
/// # Examples
/// ```
/// use chitin_chem::io::smiles::read_smiles;
/// use chitin_chem::perception::{analyze_valence, ValenceModel};
/// let parsed = read_smiles("CCO")?;
/// let report = analyze_valence(parsed.graph(), ValenceModel::MainGroupClosedShellV1)?;
/// assert_eq!(parsed.graph().atom_count(), 3);
/// assert!(report.atoms().all(|(_, row)| row.value().is_some()));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn read_smiles(source: &str) -> Result<ParsedSmiles, SmilesError> {
  let tokens = syntax::scan(source)?;
  parser::build(source, tokens)
}

/// Attaches an existing UTF-8 byte range without normalizing or relocating it.
fn error(span: Range<usize>, kind: SmilesErrorKind) -> SmilesError {
  SmilesError { span, kind }
}
