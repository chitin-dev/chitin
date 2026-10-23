//! Detached normalized coordinates: x-right, y-up, with nominal unit bonds.
//! Symmetric bridges and supported cage cores allow unequal projected bond lengths.
//! Complex ring fallback may use bounded local single-bond length exceptions
//! when exact unit closure fails; source connectivity and stereo remain unchanged.
//!
//! Domain ring blocks constrain rigid polygon seeds; branch expansion is followed
//! by bounded overlap refinement. Layout constraints never assign aromaticity.

use crate::geometry::{Bounds, GeometryError, Point};
use chitin_chem::{AtomId, BondId, GraphError, MolecularGraph};
use std::collections::BTreeMap;

mod alkenes;
mod angles;
mod closure;
pub mod coordination;
pub mod optimization;
mod placement;
mod quality;
mod refinement;
mod rings;
mod walks;
pub(crate) use refinement::{clear_labels, contact_counts, terminal_label_trials};

/// Resource, spacing, and bounded refinement policy for deterministic layout.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutOptions {
  /// Horizontal separation between component atom-center bounds, in bond units.
  pub component_gap: f64,
  /// Distance below which unresolved atom crowding is reported; between 0 and 1.
  pub minimum_spacing: f64,
  /// Maximum input vertex count; exceeding this returns an error, never truncation.
  pub max_atoms: usize,
  /// Work units for preferred ring discovery, assembly, embedding and folding.
  /// Exhaustion invokes the separately bounded fallback when enabled; neither
  /// stage publishes a partially drawn ring system.
  pub ring_search_budget: usize,
  /// Maximum discrete passes; zero disables all optional refinement stages.
  pub refinement_passes: usize,
  /// Work units for candidate traversal/indexing/geometric tests, shared by fragments.
  /// Initial and final quality inspection are outside this optimization budget.
  pub refinement_budget: usize,
  /// Shared work for optional long-backbone routing; zero disables this stage.
  /// Independent of ordinary collision repair so failed guides cannot starve it.
  pub backbone_budget: usize,
  /// Work units for required double-bond stereo constraints, independent of refinement.
  pub stereo_budget: usize,
  /// Separate whole-graph closure allowance after preferred embedding fails.
  /// Zero disables this coverage fallback; successful fallback retains diagnostics.
  pub fallback_budget: usize,
  /// Projected coordinate search after discrete refinement; zero iterations disables it.
  pub optimization: optimization::OptimizationOptions,
}

impl Default for LayoutOptions {
  /// Uses nominal unit geometry, 4096 atoms, and bounded ring/refinement work.
  fn default() -> Self {
    Self {
      component_gap: 3.,
      minimum_spacing: 0.55,
      max_atoms: 4096,
      ring_search_budget: 4_000_000,
      refinement_passes: 4,
      refinement_budget: 1_000_000,
      backbone_budget: 2_000_000,
      stereo_budget: 1_000_000,
      fallback_budget: 16_000_000,
      optimization: optimization::OptimizationOptions::default(),
    }
  }
}

/// A layout-quality limitation, not a claim that the molecule is invalid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutDiagnostic {
  /// Coupled graph closure repaired a failed preferred embedding or alkene pose.
  /// This is not a readability pass or an intentional perspective crossing.
  ConstraintClosure,
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
  /// No fully closed, stereo-valid candidate was found within the fallback allowance.
  #[error("constrained fallback layout work budget exhausted")]
  FallbackBudgetExceeded,
  /// A complete coordinate snapshot failed the optional optimizer's input contract.
  #[error(transparent)]
  Optimization(#[from] optimization::OptimizationError),
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
  /// An unknown order cannot provide integer-bond geometry.
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

/// Generates nominal-unit coordinates without changing topology or chemical facts.
///
/// # Parameters
/// - `graph`: possibly disconnected input with integer or supported aromatic bonds.
/// - `options`: fragment spacing, crowding threshold, and explicit resource budget.
///
/// # Returns
/// Deterministic positions for every live atom. Chains use zigzag placement,
/// triple/cumulated-double degree-two centers are linear, and supported ring
/// blocks use polygon, folded macrocycle/polycycle paths, bridge-path or supported
/// cage constraints. Whole-branch
/// transforms, terminal hydroxyl adjustments and bounded projected refinement
/// improve overlap with unit-bond constraints. Unresolved crowding/crossings remain diagnostic;
/// this is not a global optimum or label-collision-free guarantee.
/// Symmetric bridges retain unit outer arcs but may shorten the interior path;
/// saturated adamantane/cubane cores use exact topology templates with
/// mean core length one, not unit individual edges. These components skip the
/// optional unit-distance projected optimizer. Discrete
/// rigid refinement still preserves their initial lengths and ring geometry.
/// Aromatic input uses a detached, bounded C/N/O/S Kekulé representation shared
/// with depiction. This does not perceive aromaticity or modify input facts.
/// Failed preferred ring/alkene placement can use the separately bounded global
/// distance-closure fallback. Its diagnostic remains visible; successful closure
/// guarantees complete geometry and supplied alkene sides, not readability.
///
/// # Errors
/// Rejects invalid options, exceeded budgets, unsupported bond labels, and any
/// ring blocks incompatible with both preferred models and constrained fallback.
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
    || options.stereo_budget == 0
    || (options.refinement_passes > 0 && options.refinement_budget == 0)
    || !options.optimization.valid()
  {
    return Err(LayoutError::Options);
  }
  if graph.atom_count() > options.max_atoms {
    return Err(LayoutError::AtomBudget {
      count: graph.atom_count(),
      limit: options.max_atoms,
    });
  }
  let resolved = crate::chemical::integer_bonds(graph)?;
  let mut preferred = rings::Work::new(options.ring_search_budget);
  match placement::generate(&resolved, graph, options, &mut preferred, rings::RingMode::Preferred) {
    Err(LayoutError::RingSystem { .. } | LayoutError::DoubleBondStereo(_) | LayoutError::RingSearchBudgetExceeded)
      if options.fallback_budget > 0 =>
    {
      let mut fallback = rings::Work::new(options.fallback_budget);
      // Preserve established successful drawings. Expanded ring candidates get
      // only a quarter of the separate allowance; generic closure keeps the rest.
      let retry = fallback
        .optional(
          options.fallback_budget - options.fallback_budget / 4,
          |work| match placement::generate(&resolved, graph, options, work, rings::RingMode::Coupled) {
            Ok(candidate) => Ok(Some(candidate)),
            Err(LayoutError::RingSystem { .. } | LayoutError::DoubleBondStereo(_)) => Ok(None),
            Err(error) => Err(error),
          },
        )?
        .flatten();
      if let Some(mut candidate) = retry {
        if !candidate.diagnostics.contains(&LayoutDiagnostic::ConstraintClosure) {
          candidate.diagnostics.push(LayoutDiagnostic::ConstraintClosure);
        }
        return Ok(candidate);
      }
      let options = LayoutOptions {
        fallback_budget: fallback.remaining(),
        ..options.clone()
      };
      closure::generate(&resolved, &options)
    }
    result => result,
  }
}

/// Generates one optional alternate assembly under its own finite allowance.
///
/// # Errors
/// Unsupported topology or exhausted search rejects only this optional proposal.
pub(crate) fn exploratory(graph: &MolecularGraph, options: &LayoutOptions) -> Result<Layout2d, LayoutError> {
  alternate(graph, options, rings::RingMode::Exploratory)
}

/// Searches exact shared paths and preserves scored chain angles independently.
///
/// # Errors
/// Failure or work exhaustion rejects only this additional automatic candidate.
pub(crate) fn constrained(graph: &MolecularGraph, options: &LayoutOptions) -> Result<Layout2d, LayoutError> {
  alternate(graph, options, rings::RingMode::Constrained)
}

/// Runs one alternate family with a separate finite work allowance.
fn alternate(graph: &MolecularGraph, options: &LayoutOptions, mode: rings::RingMode) -> Result<Layout2d, LayoutError> {
  let resolved = crate::chemical::integer_bonds(graph)?;
  let options = LayoutOptions {
    ring_search_budget: options.fallback_budget / 4,
    refinement_budget: options.refinement_budget / 2,
    ..options.clone()
  };
  let mut work = rings::Work::new(options.ring_search_budget);
  placement::generate(&resolved, graph, &options, &mut work, mode)
}

/// Retries an automatic depiction's degenerate projection without altering chemistry.
///
/// # Errors
/// Enforces the original atom limit, aromatic representation and independent
/// fallback allowance. Explicit `depict` calls never invoke this stage.
pub(crate) fn constrained_fallback(graph: &MolecularGraph, options: &LayoutOptions) -> Result<Layout2d, LayoutError> {
  if graph.atom_count() > options.max_atoms {
    return Err(LayoutError::AtomBudget {
      count: graph.atom_count(),
      limit: options.max_atoms,
    });
  }
  let resolved = crate::chemical::integer_bonds(graph)?;
  closure::generate(&resolved, options)
}
