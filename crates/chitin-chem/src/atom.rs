//! Atomic facts without coordinates, biological membership, or inferred defaults.

use crate::{Element, ImplicitHydrogenPolicy, Isotope};

/// Chemical attributes of a graph vertex, independent of its graph-local ID.
///
/// `None` means unspecified, not zero. Hydrogen vertices, declared attached H,
/// and the implicit-H policy are separate facts; none is a computed cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Atom {
  element: Element,
  isotope: Option<Isotope>,
  formal_charge: Option<i16>,
  declared_hydrogens: Option<u8>,
  implicit_hydrogen_policy: ImplicitHydrogenPolicy,
  unpaired_electrons: Option<u8>,
}

impl Atom {
  /// Creates an elemental atom with unspecified isotope, charge, H count, and radical state.
  pub const fn new(element: Element) -> Self {
    Self {
      element,
      isotope: None,
      formal_charge: None,
      declared_hydrogens: None,
      implicit_hydrogen_policy: ImplicitHydrogenPolicy::Unspecified,
      unpaired_electrons: None,
    }
  }

  /// Creates an isotope-labelled atom with otherwise unspecified chemical attributes.
  pub const fn from_isotope(isotope: Isotope) -> Self {
    Self {
      isotope: Some(isotope),
      ..Self::new(isotope.element())
    }
  }

  /// Returns the atom's element.
  pub const fn element(&self) -> Element {
    self.element
  }
  /// Returns an explicit isotope label, if supplied.
  pub const fn isotope(&self) -> Option<Isotope> {
    self.isotope
  }
  /// Returns the supplied formal charge, distinguishing unknown from neutral.
  pub const fn formal_charge(&self) -> Option<i16> {
    self.formal_charge
  }
  /// Returns a fixed input count, or `None` for unspecified/inferred implicit H.
  pub const fn implicit_hydrogens(&self) -> Option<u8> {
    match self.implicit_hydrogen_policy {
      ImplicitHydrogenPolicy::Fixed(count) => Some(count),
      ImplicitHydrogenPolicy::Unspecified | ImplicitHydrogenPolicy::Infer => None,
    }
  }
  /// Returns the supplied attached-H declaration, distinct from H vertices.
  pub const fn declared_hydrogens(&self) -> Option<u8> {
    self.declared_hydrogens
  }
  /// Returns the instruction for obtaining additional implicit H.
  pub const fn implicit_hydrogen_policy(&self) -> ImplicitHydrogenPolicy {
    self.implicit_hydrogen_policy
  }
  /// Returns a supplied unpaired-electron count, not a spin multiplicity.
  pub const fn unpaired_electrons(&self) -> Option<u8> {
    self.unpaired_electrons
  }

  /// Sets a supplied formal charge, or clears it to unknown.
  pub const fn with_formal_charge(mut self, charge: Option<i16>) -> Self {
    self.formal_charge = charge;
    self
  }
  /// Sets a fixed implicit-H count, or resets the policy to unspecified for `None`.
  pub const fn with_implicit_hydrogens(mut self, count: Option<u8>) -> Self {
    self.implicit_hydrogen_policy = match count {
      Some(count) => ImplicitHydrogenPolicy::Fixed(count),
      None => ImplicitHydrogenPolicy::Unspecified,
    };
    self
  }
  /// Sets the attached-H declaration without adding vertices or implicit H.
  pub const fn with_declared_hydrogens(mut self, count: Option<u8>) -> Self {
    self.declared_hydrogens = count;
    self
  }
  /// Sets the implicit-H policy, replacing any previously fixed count.
  pub const fn with_implicit_hydrogen_policy(mut self, policy: ImplicitHydrogenPolicy) -> Self {
    self.implicit_hydrogen_policy = policy;
    self
  }
  /// Sets a supplied unpaired-electron count without assigning molecular spin.
  pub const fn with_unpaired_electrons(mut self, count: Option<u8>) -> Self {
    self.unpaired_electrons = count;
    self
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn new_atoms_should_not_invent_missing_chemical_facts() {
    let atom = Atom::new(Element::Carbon);
    assert_eq!(
      (
        atom.isotope(),
        atom.formal_charge(),
        atom.implicit_hydrogens(),
        atom.unpaired_electrons()
      ),
      (None, None, None, None)
    );
  }

  #[test]
  fn known_zero_values_should_remain_distinct_from_unknown() {
    let unknown = Atom::new(Element::Carbon);
    let known = unknown
      .clone()
      .with_formal_charge(Some(0))
      .with_implicit_hydrogens(Some(0))
      .with_unpaired_electrons(Some(0));
    assert_ne!(unknown, known);
    assert_eq!(known.clone().with_formal_charge(None).formal_charge(), None);
    assert_eq!(
      (
        known.formal_charge(),
        known.implicit_hydrogens(),
        known.unpaired_electrons()
      ),
      (Some(0), Some(0), Some(0))
    );
  }

  #[test]
  fn isotope_atom_should_keep_the_same_element() -> Result<(), crate::IsotopeError> {
    let isotope = Isotope::new(Element::Carbon, 13)?;
    let atom = Atom::from_isotope(isotope).with_formal_charge(Some(-1));
    assert_eq!(
      (atom.element(), atom.isotope(), atom.formal_charge()),
      (Element::Carbon, Some(isotope), Some(-1))
    );
    Ok(())
  }
}
