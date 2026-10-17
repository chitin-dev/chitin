//! Shared lexical capability boundary, not a chemical valence model.

use crate::Element;

/// Maximum isotope-field width, independent of whether the nuclide exists.
pub(super) const ISOTOPE_DIGITS: usize = 3;
/// Attached H uses one digit in the current molecular SMILES grammar.
pub(super) const HYDROGEN_DIGITS: usize = 1;
/// Numeric charge width excludes its sign; repeated signs are parsed separately.
pub(super) const CHARGE_DIGITS: usize = 2;
/// Largest count that can survive the shared attached-H spelling.
pub(super) const MAX_HYDROGENS: u16 = 10_u16.pow(HYDROGEN_DIGITS as u32) - 1;
/// Largest charge magnitude, not a chemically supported charge-state claim.
pub(super) const MAX_CHARGE: u16 = 10_u16.pow(CHARGE_DIGITS as u32) - 1;
/// Labels 0..9 and %00..%99 share 100 numeric identities, reusable after closure.
pub(super) const RING_LABEL_COUNT: usize = 100;

/// Tests bare-symbol eligibility only; this is not a valence/aromaticity rule.
pub(super) fn organic_subset(element: Element) -> bool {
  matches!(
    element,
    Element::Boron
      | Element::Carbon
      | Element::Nitrogen
      | Element::Oxygen
      | Element::Phosphorus
      | Element::Sulfur
      | Element::Fluorine
      | Element::Chlorine
      | Element::Bromine
      | Element::Iodine
  )
}
