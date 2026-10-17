//! Detached normalized coordinates: x-right, y-up, with unit-length bonds.
//!
//! The first automatic model handles trees and connected components containing
//! one simple cycle. Multiple cycles in one component are explicitly unsupported.
//! Ring peeling here is layout bookkeeping, not aromaticity or SSSR perception.

use crate::geometry::{Bounds, GeometryError, Point};
use chitin_chem::{AtomId, BondId, GraphError, MolecularGraph};
use std::collections::BTreeMap;

mod placement;

/// Resource and spacing policy for the initial deterministic layout model.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutOptions {
  /// Horizontal separation between component atom-center bounds, in bond units.
  pub component_gap: f64,
  /// Distance below which unresolved atom crowding is reported; between 0 and 1.
  pub minimum_spacing: f64,
  /// Maximum input vertex count; exceeding this returns an error, never truncation.
  pub max_atoms: usize,
}

impl Default for LayoutOptions {
  /// Uses three bond units between fragments, a 0.55 crowding threshold, and 4096 atoms.
  fn default() -> Self {
    Self {
      component_gap: 3.,
      minimum_spacing: 0.55,
      max_atoms: 4096,
    }
  }
}

/// A layout-quality limitation, not a claim that the molecule is invalid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutDiagnostic {
  /// Bounded local placement could not separate two atom centers sufficiently.
  CrowdedAtoms {
    /// First input handle.
    a: AtomId,
    /// Second input handle in the same graph.
    b: AtomId,
  },
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
  /// No aromatic/unknown bond semantics are supplied to this initial model.
  #[error("unsupported bond {0} in automatic layout")]
  Bond(BondId),
  /// This component needs a later ring-system embedding model.
  #[error("component at atom {atom} has {cycles} independent cycles; only one is supported")]
  RingSystem {
    /// Smallest component handle for locating the unsupported input.
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
/// triple/cumulated-double degree-two centers are linear, and a single cycle is
/// a regular polygon. Bounded angle search reduces nearby atom overlap; unresolved
/// crowding is diagnostic. This is not a global crossing-free layout guarantee.
///
/// # Errors
/// Rejects invalid options, exceeded budgets, unsupported bond labels, and any
/// component with more than one independent cycle. No partial layout is returned.
pub fn generate(graph: &MolecularGraph, options: &LayoutOptions) -> Result<Layout2d, LayoutError> {
  if !options.component_gap.is_finite()
    || options.component_gap <= 0.
    || !options.minimum_spacing.is_finite()
    || options.minimum_spacing <= 0.
    || options.minimum_spacing >= 1.
    || options.max_atoms == 0
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
