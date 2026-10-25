//! Native rigid-fragment coordinates in normalized x-right, y-up space.
//! Source chemistry is immutable; drawing forces may relax nominal unit bonds.

use crate::geometry::{Bounds, GeometryError, Point};
use chitin_chem::{AtomId, BondId, GraphError, MolecularGraph};
use std::collections::BTreeMap;

mod budget;
mod engine;
mod labels;
pub mod optimization;
mod quality;
pub(crate) use labels::{clear_labels, contact_counts, terminal_label_trials};

/// Resource, spacing, and bounded refinement policy for deterministic layout.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutOptions {
  /// Horizontal separation between component atom-center bounds, in bond units.
  pub component_gap: f64,
  /// Distance below which unresolved atom crowding is reported; between 0 and 1.
  pub minimum_spacing: f64,
  /// Maximum input vertex count; exceeding this returns an error, never truncation.
  pub max_atoms: usize,
  /// Label-repair pass limit; zero also disables optional native refinement.
  /// Native search and force integration have their own finite iteration caps.
  pub refinement_passes: usize,
  /// Work units for candidate traversal/indexing/geometric tests, shared by fragments.
  /// Initial and final quality inspection are outside this optimization budget.
  pub refinement_budget: usize,
  /// Work units for required double-bond stereo constraints, independent of refinement.
  pub stereo_budget: usize,
  /// Shared work for native topology, ring construction, search and simulation.
  /// Exhaustion returns an error without invoking a different layout backend.
  pub work_budget: usize,
}

impl Default for LayoutOptions {
  /// Uses nominal unit geometry, 4096 atoms, and bounded ring/refinement work.
  fn default() -> Self {
    Self {
      component_gap: 3.,
      minimum_spacing: 0.55,
      max_atoms: 4096,
      refinement_passes: 4,
      refinement_budget: 1_000_000,
      stereo_budget: 1_000_000,
      work_budget: 16_000_000,
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
  /// Two nonincident skeleton edges have an unresolved, unannotated crossing.
  CrossingBonds {
    /// First input edge handle.
    a: BondId,
    /// Second input edge handle.
    b: BondId,
  },
  /// Optimization exhausted its work budget; the complete best-so-far layout remains valid.
  RefinementBudgetExhausted,
  /// Projected search exhausted its separate allowance; the best validated layout remains.
  OptimizationBudgetExhausted,
  /// Optional native fragment search stopped at its evaluation or work limit.
  /// The best completely evaluated assembly remains available.
  FragmentSearchLimited {
    /// Number of unique complete states scored before stopping.
    evaluations: usize,
  },
}

/// Intentional perspective crossing, distinct from an unresolved layout overlap.
///
/// Template rear bonds have visible gaps; dense spatial cores retain all bond
/// strokes and use their camera depths for front/back opacity cues. These
/// source identities survive rigid layout transforms and never create a vertex
/// or a chemical stereo descriptor. Caller-supplied positions have no crossings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BondCrossing {
  /// Continuous foreground bond.
  pub front: BondId,
  /// Background bond, interrupted or depth-cued according to the layout model.
  pub back: BondId,
}

/// Layout cannot provide coordinates without dropping or inventing information.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LayoutError {
  /// Required stereochemical constraints exceeded their separate work budget.
  #[error("double-bond stereochemistry work budget exhausted")]
  StereoBudgetExceeded,
  /// Existing cyclic/degenerate geometry cannot express the supplied relation.
  #[error("cannot satisfy double-bond configuration at bond {0}")]
  DoubleBondStereo(BondId),
  /// Supplied aromatic facts cannot be assigned by the restricted domain model.
  #[error(transparent)]
  Kekule(#[from] chitin_chem::perception::KekuleError),
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
  /// Native ring construction has no admissible complete embedding.
  #[error("native fragment embedding incomplete at atom {0}")]
  FragmentEmbeddingIncomplete(AtomId),
  /// An unknown order cannot provide integer-bond geometry.
  #[error("unsupported bond {0} in automatic layout")]
  Bond(BondId),
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
  /// Explicit authored over/under presentation, not inferred from arbitrary overlaps.
  crossings: Vec<BondCrossing>,
  /// Camera-space drawing depth for spatial cores only; never chemical z coordinates.
  depths: BTreeMap<AtomId, f64>,
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
  /// No perspective order is inferred. Clone an automatic layout to retain
  /// its authored crossings rather than copying only its position map.
  ///
  /// # Errors
  /// Rejects nonfinite coordinates or overflowing aggregate extents.
  pub fn from_positions(positions: BTreeMap<AtomId, Point>) -> Result<Self, GeometryError> {
    let bounds = Bounds::from_points(positions.values().copied())?;
    Ok(Self {
      positions,
      bounds,
      diagnostics: Vec::new(),
      crossings: Vec::new(),
      depths: BTreeMap::new(),
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
  /// Returns intentional perspective crossings for gap or camera-depth rendering.
  pub fn bond_crossings(&self) -> &[BondCrossing] {
    &self.crossings
  }
  /// Returns drawing-only camera depth; positive values face the viewer.
  pub fn drawing_depths(&self) -> &BTreeMap<AtomId, f64> {
    &self.depths
  }
}

/// Generates complete native rigid-fragment geometry without alternate backends.
///
/// # Parameters
/// - graph: immutable chemistry and graph-local source handles.
/// - options: bounded input, simulation and stereo work.
///
/// # Errors
/// Invalid options, unsupported chemistry, incomplete construction and exhausted
/// work fail explicitly; no partial or compatibility layout is returned.
pub fn generate(graph: &MolecularGraph, options: &LayoutOptions) -> Result<Layout2d, LayoutError> {
  validate_options(graph, options)?;
  let resolved = crate::chemical::integer_bonds(graph)?;
  engine::generate(&resolved, options)
}

/// Shares option and source-size checks across coordinate engines.
fn validate_options(graph: &MolecularGraph, options: &LayoutOptions) -> Result<(), LayoutError> {
  if !options.component_gap.is_finite()
    || options.component_gap <= 0.
    || !options.minimum_spacing.is_finite()
    || options.minimum_spacing <= 0.
    || options.minimum_spacing >= 1.
    || options.max_atoms == 0
    || options.work_budget == 0
    || options.stereo_budget == 0
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
  Ok(())
}
