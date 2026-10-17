//! Encode input attributes, not inferred chemical equivalence or source spelling.

use crate::{Atom, Element, ImplicitHydrogenPolicy};

use super::{super::profile, SmilesAtomWriteError as Error};

/// Chooses a token only when rereading it preserves every supplied atom attribute.
///
/// # Parameters
/// - `atom`: source facts, independent of neighbors or reader provenance.
///
/// # Returns
/// A bare symbol for an exact match to organic-subset reader defaults; otherwise
/// a bracket token with known charge/declared H, fixed zero additional implicit
/// H, and unknown radical state. H0, H1, and charge spellings may be shortened
/// without changing their parsed values. No valence or H inference is performed.
///
/// # Errors
/// Rejects unknown required fields, unsupported inference/fixed-H combinations,
/// bracket radical knowledge that would be lost (including known zero), H-as-host,
/// and numeric fields outside the shared lexical profile. It never merges H
/// sources, clears an isotope, or replaces unknown values with neutral/zero.
pub(super) fn encode(atom: &Atom) -> Result<String, Error> {
  let charge = atom.formal_charge().ok_or(Error::UnknownFormalCharge)?;
  let hydrogens = atom.declared_hydrogens().ok_or(Error::UnknownDeclaredHydrogens)?;
  match atom.implicit_hydrogen_policy() {
    ImplicitHydrogenPolicy::Unspecified => return Err(Error::UnknownHydrogenPolicy),
    ImplicitHydrogenPolicy::Infer => {
      if profile::organic_subset(atom.element())
        && atom.isotope().is_none()
        && charge == 0
        && hydrogens == 0
        && atom.unpaired_electrons() == Some(0)
      {
        return Ok(atom.element().symbol().into());
      }
      return Err(Error::ImplicitInference);
    }
    ImplicitHydrogenPolicy::Fixed(0) => {}
    ImplicitHydrogenPolicy::Fixed(count) => return Err(Error::FixedImplicitHydrogens(count)),
  }
  // Choosing brackets is not permission to erase an explicit zero radical
  // count. The reader currently returns None for that fact on bracket atoms.
  if let Some(count) = atom.unpaired_electrons() {
    return Err(Error::ExplicitRadicalState(count));
  }
  if u16::from(hydrogens) > profile::MAX_HYDROGENS {
    return Err(Error::HydrogenCountOutOfRange(hydrogens));
  }
  if charge.unsigned_abs() > profile::MAX_CHARGE {
    return Err(Error::ChargeOutOfRange(charge));
  }
  if atom.element() == Element::Hydrogen && hydrogens != 0 {
    return Err(Error::HydrogenAsHost);
  }
  let mut token = String::from("[");
  if let Some(isotope) = atom.isotope() {
    let mass = isotope.mass_number();
    let digits = mass.to_string();
    if digits.len() > profile::ISOTOPE_DIGITS {
      return Err(Error::IsotopeOutOfRange(mass));
    }
    token.push_str(&digits);
  }
  token.push_str(atom.element().symbol());
  if hydrogens > 0 {
    token.push('H');
    if hydrogens != 1 {
      token.push_str(&hydrogens.to_string());
    }
  }
  if charge != 0 {
    token.push(if charge > 0 { '+' } else { '-' });
    if charge.unsigned_abs() != 1 {
      token.push_str(&charge.unsigned_abs().to_string());
    }
  }
  token.push(']');
  Ok(token)
}
