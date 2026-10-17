//! Detached normalized coordinates: x-right, y-up, with unit-length bonds.
//!
//! Domain ring blocks constrain rigid polygon seeds; branch expansion is followed
//! by bounded overlap refinement. Layout constraints never assign aromaticity.

use crate::geometry::{Bounds, GeometryError, Point};
use chitin_chem::{AtomId, BondId, GraphError, MolecularGraph};
use std::collections::BTreeMap;

mod angles;
mod placement;
mod refinement;
mod rings;

/// Resource, spacing, and bounded refinement policy for deterministic layout.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutOptions {
  /// Horizontal separation between component atom-center bounds, in bond units.
  pub component_gap: f64,
  /// Distance below which unresolved atom crowding is reported; between 0 and 1.
  pub minimum_spacing: f64,
  /// Maximum input vertex count; exceeding this returns an error, never truncation.
  pub max_atoms: usize,
  /// Work units for ring-cycle discovery, constraint assembly, and rigid embedding.
  /// Exceeding this budget returns an error, never a partially drawn ring system.
  pub ring_search_budget: usize,
  /// Maximum improving branch-search passes; zero disables refinement.
  pub refinement_passes: usize,
  /// Work units for candidate traversal/indexing/geometric tests, shared by fragments.
  /// Initial and final quality inspection are outside this optimization budget.
  pub refinement_budget: usize,
}

impl Default for LayoutOptions {
  /// Uses unit geometry, 4096 atoms, and bounded ring discovery and refinement.
  fn default() -> Self {
    Self {
      component_gap: 3.,
      minimum_spacing: 0.55,
      max_atoms: 4096,
      ring_search_budget: 2_000_000,
      refinement_passes: 4,
      refinement_budget: 1_000_000,
    }
  }
}

/// A layout-quality limitation, not a claim that the molecule is invalid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutDiagnostic {
  /// Final geometry retains two atom centers closer than the requested threshold.
  CrowdedAtoms {
    /// First input handle.
    a: AtomId,
    /// Second input handle in the same graph.
    b: AtomId,
  },
  /// Two nonincident skeleton edges still cross after bounded refinement.
  CrossingBonds {
    /// First input edge handle.
    a: BondId,
    /// Second input edge handle.
    b: BondId,
  },
  /// Optimization exhausted its work budget; the complete best-so-far layout remains valid.
  RefinementBudgetExhausted,
}

/// Layout cannot provide coordinates without dropping or inventing information.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LayoutError {
  /// Options are nonfinite, nonpositive, or have an invalid threshold/budget.
  #[error("invalid layout options")]
  Options,
  /// Input exceeds the caller's vertex budget.
  #[error("layout atom budget exceeded: {count} > {limit}")]
  AtomBudget {
    /// Actual vertex count.
    count: usize,
    /// Requested limit.
    limit: usize,
  },
  /// Ring discovery or embedding exhausted the caller's explicit work budget.
  #[error("ring layout work budget exhausted")]
  RingSearchBudgetExceeded,
  /// No aromatic/unknown bond semantics are supplied to this initial model.
  #[error("unsupported bond {0} in automatic layout")]
  Bond(BondId),
  /// This biconnected block needs a more general ring-system embedding model.
  #[error("ring block at atom {atom} has {cycles} independent cycles and needs a more general embedding model")]
  RingSystem {
    /// Smallest block handle for locating the unsupported input.
    atom: AtomId,
    /// E - V + 1, not an SSSR or aromatic-ring count.
    cycles: usize,
  },
  /// A derived coordinate exceeds the finite spatial-index range.
  #[error("layout coordinate range exceeded")]
  CoordinateRange,
  /// An internal topology/placement invariant was violated.
  #[error("layout placement invariant failed at atom {0}")]
  Invariant(AtomId),
  /// A graph reference is inconsistent.
  #[error(transparent)]
  Topology(#[from] GraphError),
  /// Coordinates or bounds are not representable.
  #[error(transparent)]
  Geometry(#[from] GeometryError),
}

/// Immutable atom coordinates, independent of graph ownership and scene primitives.
///
/// IDs must belong to the consumer's graph namespace. Reusing coordinates after
/// edits is explicit: depiction checks exact live-ID coverage, not graph versioning.
#[derive(Debug, Clone, PartialEq)]
pub struct Layout2d {
  /// Input graph-local handles to normalized y-up positions.
  positions: BTreeMap<AtomId, Point>,
  /// Atom-center bounds only, not glyph or stroke envelopes.
  bounds: Option<Bounds>,
  /// Placement quality reports, distinct from unsupported-model failures.
  diagnostics: Vec<LayoutDiagnostic>,
}

impl Layout2d {
  /// Accepts caller-supplied coordinates without invoking automatic embedding.
  ///
  /// # Parameters
  /// - `positions`: graph-local atom handles and normalized y-up coordinates.
  ///
  /// # Returns
  /// A finite coordinate snapshot with no automatic quality diagnostics. Empty
  /// input has no bounds; graph membership is checked later by depiction.
  ///
  /// # Errors
  /// Rejects nonfinite coordinates or overflowing aggregate extents.
  pub fn from_positions(positions: BTreeMap<AtomId, Point>) -> Result<Self, GeometryError> {
    let bounds = Bounds::from_points(positions.values().copied())?;
    Ok(Self {
      positions,
      bounds,
      diagnostics: Vec::new(),
    })
  }
  /// Returns one input atom's normalized position.
  pub fn position(&self, atom: AtomId) -> Option<Point> {
    self.positions.get(&atom).copied()
  }
  /// Returns all positions in input handle order.
  pub fn positions(&self) -> &BTreeMap<AtomId, Point> {
    &self.positions
  }
  /// Returns atom-center bounds, excluding labels and strokes.
  pub const fn bounds(&self) -> Option<Bounds> {
    self.bounds
  }
  /// Returns unresolved quality limitations from automatic placement.
  pub fn diagnostics(&self) -> &[LayoutDiagnostic] {
    &self.diagnostics
  }
}

/// Generates unit-bond coordinates without changing topology or chemical facts.
///
/// # Parameters
/// - `graph`: possibly disconnected input with single/double/triple bonds.
/// - `options`: fragment spacing, crowding threshold, and explicit resource budget.
///
/// # Returns
/// Deterministic positions for every live atom. Chains use zigzag placement,
/// triple/cumulated-double degree-two centers are linear, and supported ring
/// blocks use regular polygons. Whole-branch transforms improve geometric overlap
/// without stretching bonds. Unresolved crowding/crossings remain diagnostic;
/// this is not a global optimum or label-collision-free guarantee.
///
/// # Errors
/// Rejects invalid options, exceeded budgets, unsupported bond labels, and any
/// ring blocks incompatible with shared-edge regular-polygon constraints.
/// Connected independent rings, spiro systems, and compatible multi-ring fused
/// systems are supported. Ring work exhaustion is distinct from unsupported
/// geometry. No partial layout is returned.
pub fn generate(graph: &MolecularGraph, options: &LayoutOptions) -> Result<Layout2d, LayoutError> {
  if !options.component_gap.is_finite()
    || options.component_gap <= 0.
    || !options.minimum_spacing.is_finite()
    || options.minimum_spacing <= 0.
    || options.minimum_spacing >= 1.
    || options.max_atoms == 0
    || options.ring_search_budget == 0
    || (options.refinement_passes > 0 && options.refinement_budget == 0)
  {
    return Err(LayoutError::Options);
  }
  if graph.atom_count() > options.max_atoms {
    return Err(LayoutError::AtomBudget {
      count: graph.atom_count(),
      limit: options.max_atoms,
    });
  }
  placement::generate(graph, options)
}
