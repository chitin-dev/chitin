//! Separate hydrogen input policy from counts derived from a molecular graph.

/// How an atom's additional implicit hydrogens should be obtained.
///
/// This policy never creates H vertices or changes the declared attached-H
/// count. An unspecified policy must not silently turn into automatic inference.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum ImplicitHydrogenPolicy {
  /// No instruction has been supplied; even zero implicit H is not established.
  #[default]
  Unspecified,
  /// A named chemistry model may infer additional H if sufficient facts exist.
  Infer,
  /// Use exactly this many implicit H, including zero to disable inference.
  Fixed(u8),
}

/// Distinct hydrogen contributions in a successful atom-local analysis.
///
/// Declared H are attached-H attributes, such as the four in `[NH4+]`, not
/// independently represented vertices. A reader must not also create vertices
/// for those same H without reducing the declaration accordingly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HydrogenCounts {
  pub(crate) neighbors: u32,
  pub(crate) declared: u8,
  pub(crate) implicit: u8,
}

impl HydrogenCounts {
  /// Returns the number of adjacent H vertices, including isotope-labelled H.
  pub const fn neighbors(self) -> u32 {
    self.neighbors
  }
  /// Returns attached H explicitly declared as an atomic attribute.
  pub const fn declared(self) -> u8 {
    self.declared
  }
  /// Returns additional implicit H, fixed by input or inferred by the model.
  pub const fn implicit(self) -> u8 {
    self.implicit
  }
  /// Returns the sum of all three contributions without narrowing the count.
  pub fn total(self) -> u64 {
    u64::from(self.neighbors) + u64::from(self.declared) + u64::from(self.implicit)
  }
}
