//! Element identity is independent of isotope selection and atomic mass data.
//!
//! Names and symbols follow the IUPAC periodic table:
//! <https://iupac.org/what-we-do/periodic-table-of-elements/>.
//! This table deliberately contains no guessed masses, abundances, or valences.

use std::{fmt, str::FromStr};

/// An invalid atomic number or noncanonical element symbol.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ElementError {
  /// No element has the supplied atomic number in the supported table.
  #[error("atomic number {0} is outside 1..=118")]
  AtomicNumber(u8),
  /// The supplied string is not an exact, case-sensitive element symbol.
  #[error("unknown element symbol {0:?}")]
  Symbol(String),
}

// A single ordered declaration drives identities, symbols, and numeric lookup.
// Keeping separate tables would let a new symbol silently acquire the wrong Z.
macro_rules! elements {
  ($($name:ident = $number:literal => $symbol:literal),+ $(,)?) => {
    /// One of the 118 named elements, identified by nuclear charge.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    #[repr(u8)]
    pub enum Element {
      $(#[doc = concat!($symbol, ": ", stringify!($name), ".")]
      $name = $number,)+
    }

    impl Element {
      /// All supported elements in increasing atomic-number order.
      pub const ALL: [Self; 118] = [$(Self::$name,)+];

      /// Returns the canonical, case-sensitive chemical symbol.
      pub const fn symbol(self) -> &'static str {
        match self { $(Self::$name => $symbol,)+ }
      }

      /// Resolves an exact element symbol, without trimming or case conversion.
      ///
      /// # Errors
      /// Returns [`ElementError::Symbol`] for aliases, pseudoatoms, or unknown
      /// symbols. `D` and `T` are isotope aliases, not separate elements.
      pub fn from_symbol(symbol: &str) -> Result<Self, ElementError> {
        match symbol {
          $($symbol => Ok(Self::$name),)+
          _ => Err(ElementError::Symbol(symbol.to_owned())),
        }
      }
    }
  };
}

#[rustfmt::skip]
elements! {
  Hydrogen = 1 => "H",
  Helium = 2 => "He",
  Lithium = 3 => "Li",
  Beryllium = 4 => "Be",
  Boron = 5 => "B",
  Carbon = 6 => "C",
  Nitrogen = 7 => "N",
  Oxygen = 8 => "O",
  Fluorine = 9 => "F",
  Neon = 10 => "Ne",
  Sodium = 11 => "Na",
  Magnesium = 12 => "Mg",
  Aluminium = 13 => "Al",
  Silicon = 14 => "Si",
  Phosphorus = 15 => "P",
  Sulfur = 16 => "S",
  Chlorine = 17 => "Cl",
  Argon = 18 => "Ar",
  Potassium = 19 => "K",
  Calcium = 20 => "Ca",
  Scandium = 21 => "Sc",
  Titanium = 22 => "Ti",
  Vanadium = 23 => "V",
  Chromium = 24 => "Cr",
  Manganese = 25 => "Mn",
  Iron = 26 => "Fe",
  Cobalt = 27 => "Co",
  Nickel = 28 => "Ni",
  Copper = 29 => "Cu",
  Zinc = 30 => "Zn",
  Gallium = 31 => "Ga",
  Germanium = 32 => "Ge",
  Arsenic = 33 => "As",
  Selenium = 34 => "Se",
  Bromine = 35 => "Br",
  Krypton = 36 => "Kr",
  Rubidium = 37 => "Rb",
  Strontium = 38 => "Sr",
  Yttrium = 39 => "Y",
  Zirconium = 40 => "Zr",
  Niobium = 41 => "Nb",
  Molybdenum = 42 => "Mo",
  Technetium = 43 => "Tc",
  Ruthenium = 44 => "Ru",
  Rhodium = 45 => "Rh",
  Palladium = 46 => "Pd",
  Silver = 47 => "Ag",
  Cadmium = 48 => "Cd",
  Indium = 49 => "In",
  Tin = 50 => "Sn",
  Antimony = 51 => "Sb",
  Tellurium = 52 => "Te",
  Iodine = 53 => "I",
  Xenon = 54 => "Xe",
  Caesium = 55 => "Cs",
  Barium = 56 => "Ba",
  Lanthanum = 57 => "La",
  Cerium = 58 => "Ce",
  Praseodymium = 59 => "Pr",
  Neodymium = 60 => "Nd",
  Promethium = 61 => "Pm",
  Samarium = 62 => "Sm",
  Europium = 63 => "Eu",
  Gadolinium = 64 => "Gd",
  Terbium = 65 => "Tb",
  Dysprosium = 66 => "Dy",
  Holmium = 67 => "Ho",
  Erbium = 68 => "Er",
  Thulium = 69 => "Tm",
  Ytterbium = 70 => "Yb",
  Lutetium = 71 => "Lu",
  Hafnium = 72 => "Hf",
  Tantalum = 73 => "Ta",
  Tungsten = 74 => "W",
  Rhenium = 75 => "Re",
  Osmium = 76 => "Os",
  Iridium = 77 => "Ir",
  Platinum = 78 => "Pt",
  Gold = 79 => "Au",
  Mercury = 80 => "Hg",
  Thallium = 81 => "Tl",
  Lead = 82 => "Pb",
  Bismuth = 83 => "Bi",
  Polonium = 84 => "Po",
  Astatine = 85 => "At",
  Radon = 86 => "Rn",
  Francium = 87 => "Fr",
  Radium = 88 => "Ra",
  Actinium = 89 => "Ac",
  Thorium = 90 => "Th",
  Protactinium = 91 => "Pa",
  Uranium = 92 => "U",
  Neptunium = 93 => "Np",
  Plutonium = 94 => "Pu",
  Americium = 95 => "Am",
  Curium = 96 => "Cm",
  Berkelium = 97 => "Bk",
  Californium = 98 => "Cf",
  Einsteinium = 99 => "Es",
  Fermium = 100 => "Fm",
  Mendelevium = 101 => "Md",
  Nobelium = 102 => "No",
  Lawrencium = 103 => "Lr",
  Rutherfordium = 104 => "Rf",
  Dubnium = 105 => "Db",
  Seaborgium = 106 => "Sg",
  Bohrium = 107 => "Bh",
  Hassium = 108 => "Hs",
  Meitnerium = 109 => "Mt",
  Darmstadtium = 110 => "Ds",
  Roentgenium = 111 => "Rg",
  Copernicium = 112 => "Cn",
  Nihonium = 113 => "Nh",
  Flerovium = 114 => "Fl",
  Moscovium = 115 => "Mc",
  Livermorium = 116 => "Lv",
  Tennessine = 117 => "Ts",
  Oganesson = 118 => "Og",
}

impl Element {
  /// Returns the atomic number (nuclear charge in elementary-charge units).
  pub const fn atomic_number(self) -> u8 {
    self as u8
  }

  /// Resolves an atomic number, rejecting zero and values beyond the table.
  ///
  /// # Errors
  /// Returns [`ElementError::AtomicNumber`] unless `number` is within 1..=118.
  pub fn from_atomic_number(number: u8) -> Result<Self, ElementError> {
    number
      .checked_sub(1)
      .and_then(|index| Self::ALL.get(usize::from(index)))
      .copied()
      .ok_or(ElementError::AtomicNumber(number))
  }
}

impl fmt::Display for Element {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(self.symbol())
  }
}

impl FromStr for Element {
  type Err = ElementError;
  fn from_str(symbol: &str) -> Result<Self, Self::Err> {
    Self::from_symbol(symbol)
  }
}

impl TryFrom<u8> for Element {
  type Error = ElementError;
  fn try_from(number: u8) -> Result<Self, Self::Error> {
    Self::from_atomic_number(number)
  }
}

/// A mass number that cannot contain the element's proton count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("mass number {mass_number} must be at least the atomic number of {element}")]
pub struct IsotopeError {
  /// Element whose nuclear identity was requested.
  pub element: Element,
  /// Supplied proton-plus-neutron count; not an atomic mass.
  pub mass_number: u16,
}

/// An explicit isotope label, distinct from natural isotopic composition.
///
/// Construction checks A ≥ Z only. It does not certify that a nuclide has been
/// observed, is bound, or is stable; that requires a separately sourced dataset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Isotope {
  element: Element,
  mass_number: u16,
}

impl Isotope {
  /// Creates an isotope label from element and proton-plus-neutron count.
  ///
  /// # Errors
  /// Returns [`IsotopeError`] when the mass number is smaller than Z.
  pub fn new(element: Element, mass_number: u16) -> Result<Self, IsotopeError> {
    if mass_number < u16::from(element.atomic_number()) {
      return Err(IsotopeError { element, mass_number });
    }
    Ok(Self { element, mass_number })
  }

  /// Returns the isotope's element.
  pub const fn element(self) -> Element {
    self.element
  }
  /// Returns the proton-plus-neutron count, not a mass in daltons.
  pub const fn mass_number(self) -> u16 {
    self.mass_number
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn every_element_should_round_trip_through_number_and_symbol() -> Result<(), ElementError> {
    let mut symbols = std::collections::BTreeSet::new();
    for (index, element) in Element::ALL.into_iter().enumerate() {
      assert_eq!(usize::from(element.atomic_number()), index + 1);
      assert_eq!(Element::from_atomic_number(element.atomic_number())?, element);
      assert_eq!(element.symbol().parse::<Element>()?, element);
      assert!(symbols.insert(element.symbol()));
    }
    Ok(())
  }

  #[test]
  fn symbols_should_be_canonical_without_implicit_normalization() {
    for symbol in ["", "CL", "cl", "c", " C", "C ", "D", "T", "*", "Uut", "Ｃ"] {
      assert!(Element::from_symbol(symbol).is_err(), "accepted {symbol:?}");
    }
  }

  #[test]
  fn atomic_numbers_outside_table_should_be_rejected() {
    for number in [0, 119, 255] {
      assert_eq!(Element::try_from(number), Err(ElementError::AtomicNumber(number)));
    }
  }

  #[test]
  fn isotope_labels_should_preserve_element_and_mass_number() -> Result<(), IsotopeError> {
    let carbon = Isotope::new(Element::Carbon, 13)?;
    assert_eq!((carbon.element(), carbon.mass_number()), (Element::Carbon, 13));
    assert_eq!(Isotope::new(Element::Hydrogen, 2)?.element(), Element::Hydrogen);
    Ok(())
  }

  #[test]
  fn mass_numbers_smaller_than_proton_count_should_be_rejected() {
    for (element, mass) in [(Element::Hydrogen, 0), (Element::Carbon, 5), (Element::Oganesson, 117)] {
      assert!(Isotope::new(element, mass).is_err());
    }
  }
}
