//! Bounded projected refinement of complete 2D coordinates, not molecular energy.
//!
//! Callers keep graph ownership and explicitly pin source atom handles. Search
//! never constructs missing rings, edits chemistry, packs fragments, or silently
//! weakens unit-bond constraints. This explicit optimizer is independent of
//! automatic coordinate generation and requires caller-supplied unit geometry.

use super::{
  Layout2d, LayoutDiagnostic,
  quality::{self, Budget},
};
use crate::geometry::{Bounds, GeometryError, Point};
use chitin_chem::{AtomId, BondId, BondOrder, MolecularGraph, perception::analyze_rings};
use std::collections::{BTreeMap, BTreeSet};

mod constraints;
mod objective;
mod solver;
use constraints::Model;

/// Absolute normalized bond-length tolerance at input and commit boundaries.
pub const BOND_TOLERANCE: f64 = 1e-9;

/// Search limits independent of ring discovery and discrete branch refinement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptimizationOptions {
  /// Maximum projected gradient steps per connected component; zero disables search.
  pub iterations: usize,
  /// Maximum constraint projection sweeps per line-search trial; must be positive.
  pub projection_passes: usize,
  /// Shared search work across components; zero disables search, never validation.
  pub work_budget: usize,
  /// Maximum input atoms for the independent entry point.
  pub max_atoms: usize,
}

impl Default for OptimizationOptions {
  /// Uses bounded descent, strict projections, and a finite shared work allowance.
  fn default() -> Self {
    Self {
      iterations: 64,
      projection_passes: 256,
      work_budget: 1_000_000,
      max_atoms: 4096,
    }
  }
}

impl OptimizationOptions {
  /// Returns whether the iteration-independent resource limits are valid.
  pub(super) fn valid(&self) -> bool {
    self.projection_passes > 0 && self.max_atoms > 0
  }
}

/// Search telemetry; exhaustion is not a claim of convergence or chemical failure.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OptimizationReport {
  /// Shared atom/atom-edge/crossing score before search, excluding interfragment pairs.
  pub initial_geometry_cost: f64,
  /// Same score after search; never exceeds the initial componentwise score.
  pub final_geometry_cost: f64,
  /// Number of connected components whose complete geometry was improved.
  pub improved_components: usize,
  /// Projected gradient iterations attempted across all components.
  pub iterations: usize,
  /// Work units spent on model preparation, proposals, projections and search scoring.
  pub work_used: usize,
  /// Whether work, rather than just the iteration limit, ended the search.
  pub budget_exhausted: bool,
}

/// Detached validated geometry and its optimization telemetry.
#[derive(Debug, Clone, PartialEq)]
pub struct OptimizationResult {
  /// Complete coordinates, with no automatic reorientation or fragment repacking.
  pub layout: Layout2d,
  /// Search outcome independent of unresolved geometry diagnostics.
  pub report: OptimizationReport,
}

/// Invalid input to the coordinate optimizer, not a general chemistry diagnosis.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OptimizationError {
  /// Projection or input resource limits are invalid.
  #[error("invalid coordinate optimization options")]
  Options,
  /// The snapshot does not cover exactly the graph's live atom handles.
  #[error("optimization coordinates must cover exactly the live graph atoms")]
  Coverage,
  /// A fixed handle is absent from the current graph.
  #[error("unknown optimization anchor {0}")]
  Anchor(AtomId),
  /// Coordinates exceed the guarded spatial range or contain nonfinite values.
  #[error("optimization coordinates exceed the finite spatial range")]
  CoordinateRange,
  /// The input contains a bond outside the unit-length contract.
  #[error("optimization requires a unit-length bond at {0}")]
  NonUnitBond(BondId),
  /// An unknown bond order cannot define protected drawing geometry.
  #[error("unsupported bond in coordinate optimization: {0}")]
  Bond(BondId),
  /// Initial coordinates already flatten or invert a supplied double configuration.
  #[error("optimization input violates double-bond configuration at {0}")]
  DoubleBondStereo(BondId),
  /// The standalone caller exceeded its explicit input-size limit.
  #[error("optimization atom limit exceeded: {count} > {limit}")]
  AtomBudget {
    /// Actual graph vertex count.
    count: usize,
    /// Requested limit.
    limit: usize,
  },
  /// Input graph references are inconsistent.
  #[error(transparent)]
  Topology(#[from] chitin_chem::GraphError),
  /// Aromatic facts cannot be represented by the shared restricted assignment model.
  #[error(transparent)]
  Kekule(#[from] chitin_chem::perception::KekuleError),
  /// Aggregate coordinate bounds are not finite.
  #[error(transparent)]
  Geometry(#[from] GeometryError),
}

/// Improves a complete unit-bond snapshot while retaining caller-pinned positions.
///
/// # Parameters
/// - `graph`, `initial`: identical live atom namespace; graph facts remain immutable.
/// - `anchors`: atoms that must retain their original coordinates bit-for-bit.
/// - `options`: explicit atom, iteration, projection and work limits.
///
/// # Returns
/// A detached result. If no strictly improved, fully verified candidate exists,
/// the original coordinates are retained. Fragments are scored independently;
/// their poses are not repacked. Budget exhaustion retains best-so-far geometry
/// and is reported separately from unresolved crowding or crossings. Mandatory
/// input/final inspection is outside the search budget and bounded by `max_atoms`.
///
/// # Errors
/// Rejects missing/extra handles, unknown anchors, invalid initial bond lengths,
/// configurations or coordinate ranges, and unsupported supplied chemistry.
/// Search never repairs an invalid initial stereochemical annotation silently.
///
/// # Examples
/// ```
/// use std::collections::BTreeSet;
/// use chitin_chem::io::smiles::read_smiles;
/// use chitin_chemdraw::layout::{self, optimization};
/// let graph = read_smiles("CC")?.into_graph();
/// let initial = layout::Layout2d::from_positions(graph.atoms().enumerate().map(|(i,(id,_))|
///   (id, chitin_chemdraw::geometry::Point::new(i as f64, 0.))).collect())?;
/// let anchor = graph.atoms().next().map(|(id, _)| id).ok_or("empty graph")?;
/// let result = optimization::optimize(
///   &graph, &initial, &BTreeSet::from([anchor]), &Default::default(),
/// )?;
/// assert_eq!(result.layout.position(anchor), initial.position(anchor));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn optimize(
  graph: &MolecularGraph,
  initial: &Layout2d,
  anchors: &BTreeSet<AtomId>,
  options: &OptimizationOptions,
) -> Result<OptimizationResult, OptimizationError> {
  if !options.valid() {
    return Err(OptimizationError::Options);
  }
  if graph.atom_count() > options.max_atoms {
    return Err(OptimizationError::AtomBudget {
      count: graph.atom_count(),
      limit: options.max_atoms,
    });
  }
  if initial.positions.len() != graph.atom_count() || graph.atoms().any(|(id, _)| !initial.positions.contains_key(&id))
  {
    return Err(OptimizationError::Coverage);
  }
  for &id in anchors {
    if graph.atom(id).is_none() {
      return Err(OptimizationError::Anchor(id));
    }
  }
  let resolved = crate::chemical::integer_bonds(graph)?;
  validate(&resolved, &initial.positions)?;
  let rings = analyze_rings(&resolved)?;
  let mut budget = Budget(options.work_budget);
  let mut result = initial.clone();
  let mut report = OptimizationReport::default();
  result
    .diagnostics
    .retain(|d| matches!(d, LayoutDiagnostic::RefinementBudgetExhausted));
  for ids in resolved.connected_components() {
    let mut points: Vec<_> = ids.iter().map(|id| initial.positions[id]).collect();
    let outcome = component(
      Scope {
        graph: &resolved,
        anchors,
        bridges: rings.bridges(),
      },
      &ids,
      &mut points,
      options,
      &mut budget,
    )?;
    report.initial_geometry_cost += outcome.before;
    report.final_geometry_cost += outcome.after;
    report.improved_components += usize::from(outcome.improved);
    report.iterations += outcome.iterations;
    report.budget_exhausted |= outcome.exhausted;
    for (&id, point) in ids.iter().zip(points) {
      result.positions.insert(id, point);
    }
    let model = Model::topology(&resolved, &ids, anchors)?;
    let quality = quality::measure(
      &model.initial(&result.positions),
      &model.edges,
      0.55,
      &mut Budget(usize::MAX),
    )
    .ok_or(OptimizationError::CoordinateRange)?;
    for (a, b) in quality.crowds {
      result
        .diagnostics
        .push(LayoutDiagnostic::CrowdedAtoms { a: ids[a], b: ids[b] });
    }
    for (a, b) in quality.crossings {
      result.diagnostics.push(LayoutDiagnostic::CrossingBonds {
        a: model.bonds[a],
        b: model.bonds[b],
      });
    }
  }
  report.work_used = options.work_budget - budget.0;
  if report.budget_exhausted {
    result.diagnostics.push(LayoutDiagnostic::OptimizationBudgetExhausted);
  }
  result.bounds = Bounds::from_points(result.positions.values().copied())?;
  Ok(OptimizationResult { layout: result, report })
}

/// Verifies exact IDs separately from finite unit geometry and source relations.
fn validate(graph: &MolecularGraph, points: &BTreeMap<AtomId, Point>) -> Result<(), OptimizationError> {
  if points
    .values()
    .any(|p| !p.is_finite() || p.x.abs() > 1e9 || p.y.abs() > 1e9)
  {
    return Err(OptimizationError::CoordinateRange);
  }
  for (id, bond) in graph.bonds() {
    if !matches!(bond.order(), BondOrder::Single | BondOrder::Double | BondOrder::Triple) {
      return Err(OptimizationError::Bond(id));
    }
    let [a, b] = bond.atoms();
    if (points[&a].distance(points[&b]) - 1.).abs() > BOND_TOLERANCE {
      return Err(OptimizationError::NonUnitBond(id));
    }
  }
  for (id, stereo) in graph.double_bond_stereos() {
    let [a, b] = stereo.ends();
    let [ra, rb] = stereo.references();
    if crate::stereo_geometry::relation([a, b, ra, rb].map(|id| points[&id])) != Some(stereo.relation()) {
      return Err(OptimizationError::DoubleBondStereo(id));
    }
  }
  Ok(())
}

/// Component-local outcome, reused by standalone and automatic callers.
pub(super) struct Outcome {
  /// Geometry-only score at entry.
  pub(super) before: f64,
  /// Verified geometry-only score at exit.
  pub(super) after: f64,
  /// Whether a strictly improving validated candidate was committed.
  pub(super) improved: bool,
  /// Number of projected search iterations attempted.
  pub(super) iterations: usize,
  /// Whether preparation or search depleted the shared allowance.
  pub(super) exhausted: bool,
}

/// Shared immutable facts and caller anchors for a component search.
pub(super) struct Scope<'a> {
  /// Immutable graph facts in the same namespace as component atom IDs.
  pub(super) graph: &'a MolecularGraph,
  /// Explicit coordinates that are not allowed to move.
  pub(super) anchors: &'a BTreeSet<AtomId>,
  /// Detached bridge classification, shared by all components.
  pub(super) bridges: &'a BTreeSet<BondId>,
}

/// Runs one component without packing, retaining its original result on failure.
pub(super) fn component(
  scope: Scope<'_>,
  ids: &[AtomId],
  points: &mut Vec<Point>,
  options: &OptimizationOptions,
  budget: &mut Budget,
) -> Result<Outcome, OptimizationError> {
  let mut model = Model::topology(scope.graph, ids, scope.anchors)?;
  let quality =
    quality::measure(points, &model.edges, 0.55, &mut Budget(usize::MAX)).ok_or(OptimizationError::CoordinateRange)?;
  let mut outcome = Outcome {
    before: quality.cost,
    after: quality.cost,
    improved: false,
    iterations: 0,
    exhausted: false,
  };
  if options.iterations == 0 || options.work_budget == 0 {
    return Ok(outcome);
  }
  if !model.prepare(scope.graph, points, scope.bridges, budget) {
    outcome.exhausted = true;
    return Ok(outcome);
  }
  solver::improve(&model, points, quality, options, budget, &mut outcome);
  Ok(outcome)
}
