//! Bounded work accounting for native layout.

use super::{LayoutError as Error, quality};
use crate::geometry::Point;

/// Shared work allowance for ring discovery, assembly, and geometric trials.
pub(super) struct Work(usize);

impl Work {
  /// Measures one complete trial while charging the same shared ring allowance.
  pub(super) fn measure(
    &mut self,
    points: &[Point],
    edges: &[[usize; 2]],
    spacing: f64,
  ) -> Result<quality::Quality, Error> {
    let mut budget = quality::Budget(self.0);
    let result = quality::measure(points, edges, spacing, &mut budget);
    self.0 = budget.0;
    result.ok_or(Error::RingSearchBudgetExceeded)
  }
  /// Creates one allowance for all ring systems in an automatic layout.
  pub(super) const fn new(limit: usize) -> Self {
    Self(limit)
  }

  /// Returns the still-unspent allowance for optional bounded trials.
  pub(super) const fn remaining(&self) -> usize {
    self.0
  }

  /// Runs an optional search without lending it the required-placement reserve.
  ///
  /// # Parameters
  /// - `reserve`: allowance unavailable to this trial, even if the trial fails.
  /// - `search`: isolated work using a child allowance; commit geometry only
  ///   after all fallible work succeeds, or return a detached candidate.
  ///
  /// # Returns
  /// The completed value, or `None` if no work is available or the child runs
  /// out. Only work actually charged to the child is deducted from the parent.
  ///
  /// # Errors
  /// Non-budget errors propagate after accounting for the consumed work.
  pub(super) fn optional<T>(
    &mut self,
    reserve: usize,
    search: impl FnOnce(&mut Self) -> Result<T, Error>,
  ) -> Result<Option<T>, Error> {
    let allowance = self.0.saturating_sub(reserve);
    if allowance == 0 {
      return Ok(None);
    }
    let mut child = Self::new(allowance);
    let result = search(&mut child);
    self.0 -= allowance - child.0;
    match result {
      Ok(value) => Ok(Some(value)),
      Err(Error::RingSearchBudgetExceeded) => Ok(None),
      Err(error) => Err(error),
    }
  }

  /// Charges before work starts; exhaustion never permits partial geometry.
  pub(super) fn charge(&mut self, units: usize) -> Result<(), Error> {
    self.0 = self.0.checked_sub(units).ok_or(Error::RingSearchBudgetExceeded)?;
    Ok(())
  }
}
