//! Non-ring branch angle allocation with explicit incoming-bond constraints.
//!
//! This module assigns angular slots only. Subtree ordering, stereochemistry,
//! and ring traversal belong to their respective layout stages.

use std::f64::consts::{PI, TAU};

/// Standard non-linear chain turn, giving a 120-degree interior angle.
pub(super) const CHAIN_BEND: f64 = PI / 3.;

/// Allocates outgoing directions relative to the incoming bond's forward heading.
///
/// # Parameters
/// - `count`: number of unplaced outgoing neighbors, already in caller order.
/// - `has_parent`: whether the incoming neighbor occupies the backward direction.
/// - `bend`: signed 60-degree turn chosen by the caller's zigzag traversal.
/// - `linear`: whether two incident bonds require straight continuation.
///
/// # Returns
/// One angular offset per child. The one-child case bends by 60°,
/// two-child case uses opposite signed bends (three 120° sectors with the parent),
/// and larger groups divide the available directions evenly, pairing ± offsets.
/// Linear continuation is handled before branch allocation, without rotation.
pub(super) fn offsets(count: usize, has_parent: bool, bend: f64, linear: bool) -> Vec<f64> {
  if linear {
    return vec![0.; count];
  }
  match count {
    0 => Vec::new(),
    1 => vec![bend],
    2 => vec![bend, -bend],
    _ => {
      let delta = TAU / (count + usize::from(has_parent)) as f64;
      let mut result = Vec::with_capacity(count);
      let mut angle = delta;
      if !count.is_multiple_of(2) {
        result.push(0.);
      } else {
        angle /= 2.;
      }
      while result.len() < count {
        result.push(angle);
        result.push(-angle);
        angle += delta;
      }
      result
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn two_children_should_leave_three_equal_sectors_with_the_incoming_neighbor() {
    for bend in [CHAIN_BEND, -CHAIN_BEND] {
      let mut directions = offsets(2, true, bend, false);
      directions.push(PI);
      for i in 0..3 {
        for j in i + 1..3 {
          assert!(((directions[i] - directions[j]).cos() + 0.5).abs() < 1e-10);
        }
      }
    }
  }
}
