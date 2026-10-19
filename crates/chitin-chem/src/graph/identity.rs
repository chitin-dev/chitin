//! Runtime provenance and monotonic allocation shared by graph snapshots.

use std::{
  fmt,
  sync::{
    OnceLock,
    atomic::{AtomicU64, Ordering},
  },
};

use super::GraphError;

/// Never reset or recycled, even after every graph in a lineage is dropped.
static NEXT_NAMESPACE: AtomicU64 = AtomicU64::new(0);

/// Full identity; serial ordering remains deterministic within one lineage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct Handle {
  /// Process-local provenance, unrelated to serialized atom-map numbers.
  namespace: u64,
  /// Monotonic atom or bond serial, shared by all snapshots of this lineage.
  pub(super) serial: u64,
}

impl fmt::Display for Handle {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(formatter, "{}:{}", self.namespace, self.serial)
  }
}

/// Shared allocation only; topology and chemical facts remain snapshot-owned.
#[derive(Debug, Default)]
pub(super) struct IdentityAllocator {
  // Lazy initialization keeps new/default infallible. Exhaustion is reported by
  // the first insertion rather than panicking or wrapping into a live namespace.
  namespace: OnceLock<Result<u64, GraphError>>,
  /// Next atom serial; exposed to the parent only for overflow regression tests.
  pub(super) next_atom: AtomicU64,
  /// Next bond serial; independent from atom allocation.
  pub(super) next_bond: AtomicU64,
}

impl IdentityAllocator {
  /// Reserves a fresh atom identity across all snapshots of this lineage.
  pub(super) fn atom(&self) -> Result<Handle, GraphError> {
    self.allocate(&self.next_atom)
  }

  /// Reserves a fresh bond identity across all snapshots of this lineage.
  pub(super) fn bond(&self) -> Result<Handle, GraphError> {
    self.allocate(&self.next_bond)
  }

  /// Combines stable provenance with a serial that cannot wrap or be reused.
  ///
  /// # Parameters
  /// - `counter`: the atom or bond allocation cursor shared by this lineage.
  ///
  /// # Returns
  /// A fresh handle, initializing this lineage's namespace once if needed.
  ///
  /// # Errors
  /// Returns `IdentifierExhausted` if namespace or serial allocation is exhausted.
  fn allocate(&self, counter: &AtomicU64) -> Result<Handle, GraphError> {
    let namespace = (*self.namespace.get_or_init(|| reserve(&NEXT_NAMESPACE)))?;
    Ok(Handle {
      namespace,
      serial: reserve(counter)?,
    })
  }
}

/// Reserves the previous value; MAX is an exhausted sentinel, never an ID.
///
/// # Parameters
/// - `counter`: a namespace or entity cursor; concurrent reservations are unique.
///
/// # Returns
/// The previous cursor value after atomically advancing it by one.
///
/// # Errors
/// Returns `IdentifierExhausted` without modifying the cursor on overflow.
fn reserve(counter: &AtomicU64) -> Result<u64, GraphError> {
  // Only uniqueness is synchronized here; no molecular data is published through
  // these counters, so acquiring/releasing unrelated memory is unnecessary.
  counter
    .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| value.checked_add(1))
    .map_err(|_| GraphError::IdentifierExhausted)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn exhausted_counters_should_never_wrap() {
    let counter = AtomicU64::new(u64::MAX - 1);
    assert_eq!(reserve(&counter), Ok(u64::MAX - 1));
    assert_eq!(reserve(&counter), Err(GraphError::IdentifierExhausted));
    assert_eq!(counter.load(Ordering::Relaxed), u64::MAX);
  }

  #[test]
  fn namespace_exhaustion_should_reject_insertion_without_allocating_serials() {
    let allocator = IdentityAllocator {
      namespace: OnceLock::from(Err(GraphError::IdentifierExhausted)),
      ..Default::default()
    };
    assert_eq!(allocator.atom(), Err(GraphError::IdentifierExhausted));
    assert_eq!(allocator.bond(), Err(GraphError::IdentifierExhausted));
    assert_eq!(allocator.next_atom.load(Ordering::Relaxed), 0);
    assert_eq!(allocator.next_bond.load(Ordering::Relaxed), 0);
  }
}
