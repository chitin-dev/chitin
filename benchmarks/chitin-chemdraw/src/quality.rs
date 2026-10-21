//! Versioned measurements and explicit applicability for diagram quality.
//!
//! Geometry is evaluated independently of optimizer scores. Numeric thresholds
//! are benchmark policy, not an assertion of complete graphical-standard compliance.

use anyhow::Result;
use chitin_chem::{MolecularGraph, perception::analyze_rings};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Bump whenever metric domains, formulas, thresholds or aggregation change.
pub const VERSION: &str = "diagram-quality-v2";
/// Embedded normative definitions; its checksum binds reports to this document.
pub const SPECIFICATION: &str = include_str!("../QUALITY_SPEC.md");

/// Versioned distances relative to the median positive skeleton bond length.
pub const ATOM_DISTANCE: f64 = 0.55;
pub const ATOM_EDGE_DISTANCE: f64 = 0.15;
pub const LENGTH_DEVIATION: f64 = 0.20;
pub const ANGLE_DEVIATION_DEG: f64 = 15.;

/// Absence of an applicable object is distinct from an unimplemented check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
  Measured,
  NotApplicable,
  NotImplemented,
}

/// One scalar with its exact domain size and optional threshold violation count.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Observation {
  pub state: State,
  pub value: Option<f64>,
  pub eligible: usize,
  pub violations: Option<usize>,
  pub reason: Option<String>,
}

impl Observation {
  /// A rate in [0,1]; zero eligible objects produces null rather than a false zero.
  pub fn rate(violations: usize, eligible: usize) -> Self {
    if eligible == 0 {
      return Self::unavailable(State::NotApplicable, 0, "no eligible objects");
    }
    Self::measured(violations as f64 / eligible as f64, eligible, Some(violations))
  }

  /// A finite descriptive value; absence of a threshold is not zero violations.
  pub fn measured(value: f64, eligible: usize, violations: Option<usize>) -> Self {
    Self {
      state: State::Measured,
      value: Some(value),
      eligible,
      violations,
      reason: None,
    }
  }

  /// A reasoned exclusion or an explicitly missing evaluator.
  pub fn unavailable(state: State, eligible: usize, reason: &str) -> Self {
    Self {
      state,
      value: None,
      eligible,
      violations: None,
      reason: Some(reason.into()),
    }
  }

  /// RMSE and per-object violations for a documented zero-centered error series.
  pub fn rmse(errors: &[f64], threshold: f64) -> Self {
    if errors.is_empty() {
      return Self::unavailable(State::NotApplicable, 0, "no eligible objects");
    }
    Self::measured(
      (errors.iter().map(|error| error * error).sum::<f64>() / errors.len() as f64).sqrt(),
      errors.len(),
      Some(errors.iter().filter(|error| error.abs() > threshold).count()),
    )
  }
}

/// Source-derived strata, computed even when subsequent coordinate generation fails.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Features {
  pub atoms: usize,
  pub bonds: usize,
  pub cycle_rank: usize,
  pub maximum_block_rank: usize,
  pub largest_cyclic_block_atoms: usize,
  pub tetrahedral_centers: usize,
  pub double_bond_descriptors: usize,
}

/// Characterizes graph blocks, never presenting block size as a chemical ring size.
pub fn characterize(graph: &MolecularGraph) -> Result<Features> {
  let topology = analyze_rings(graph)?;
  Ok(Features {
    atoms: graph.atom_count(),
    bonds: graph.bond_count(),
    cycle_rank: topology.blocks().iter().map(|block| block.cycle_rank()).sum(),
    maximum_block_rank: topology
      .blocks()
      .iter()
      .map(|block| block.cycle_rank())
      .max()
      .unwrap_or(0),
    largest_cyclic_block_atoms: topology
      .blocks()
      .iter()
      .filter(|block| block.cycle_rank() > 0)
      .map(|block| block.atoms().len())
      .max()
      .unwrap_or(0),
    tetrahedral_centers: graph.tetrahedral_centers().len(),
    double_bond_descriptors: graph.double_bond_stereos().len(),
  })
}

/// A quality vector, not an arbitrarily weighted overall aesthetic score.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Metrics {
  pub version: String,
  pub profile: String,
  pub features: Features,
  pub measurements: BTreeMap<String, Observation>,
  /// Contact screens plus required source coverage/nonzero bonds; null is unassessed.
  pub readability_clear: Option<bool>,
  /// Auditable bounded-face contours; absent in historical v1 records.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub ring_geometry: Option<crate::ring_geometry::RingGeometry>,
}

/// Metric keys making up the explicitly limited basic readability screen.
pub const READABILITY: [&str; 5] = [
  "nonbonded_atom_proximity",
  "atom_edge_proximity",
  "unannotated_edge_contact",
  "label_label_overlap",
  "label_bond_overlap",
];

impl Metrics {
  /// Checks the named screens only; no-eligible pairs are vacuously clear.
  pub fn finish(&mut self) {
    let checks: Vec<_> = READABILITY
      .iter()
      .copied()
      .chain([
        "source_bond_coverage",
        "required_atom_label_coverage",
        "zero_length_bonds",
      ])
      .map(|name| self.measurements.get(name))
      .collect();
    self.readability_clear = if checks
      .iter()
      .any(|metric| metric.is_some_and(|metric| metric.violations.is_some_and(|n| n > 0)))
    {
      Some(false)
    } else if checks.iter().all(|metric| {
      metric.is_some_and(|metric| {
        metric.state == State::NotApplicable && metric.eligible == 0 || metric.violations == Some(0)
      })
    }) {
      Some(true)
    } else {
      None
    };
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn empty_domains_are_not_reported_as_measured_zeroes() {
    let observation = Observation::rate(0, 0);
    assert_eq!(observation.state, State::NotApplicable);
    assert_eq!(observation.value, None);
    assert_eq!(observation.violations, None);
  }

  #[test]
  fn threshold_boundary_is_not_a_violation() {
    assert_eq!(Observation::rmse(&[0.2, -0.2, 0.21], 0.2).violations, Some(1));
  }
}
