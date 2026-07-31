//! Strongly typed RCSB identifiers.

use std::fmt;

use super::error::PdbIdError;

/// Canonical four-character RCSB PDB identifier.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PdbId(String);

impl PdbId {
  /// Parses and canonicalizes an RCSB PDB identifier.
  ///
  /// # Parameters
  ///
  /// `value` is the raw identifier string, such as `"4hhb"` or `"6VXX"`.
  ///
  /// # Returns
  ///
  /// `Ok(PdbId)` with an uppercase identifier when the input is valid.
  ///
  /// # Errors
  ///
  /// Returns [`PdbIdError`] when the value is not exactly four ASCII
  /// alphanumeric characters.
  pub fn new(value: &str) -> Result<Self, PdbIdError> {
    let actual = value.chars().count();
    if actual != 4 {
      return Err(PdbIdError::InvalidLength { actual });
    }

    for character in value.chars() {
      if !character.is_ascii_alphanumeric() {
        return Err(PdbIdError::InvalidCharacter { character });
      }
    }

    Ok(Self(value.to_ascii_uppercase()))
  }

  /// Returns the canonical uppercase PDB identifier.
  ///
  /// # Parameters
  ///
  /// This method reads `self`.
  ///
  /// # Returns
  ///
  /// The four-character identifier as a string slice.
  pub fn as_str(&self) -> &str {
    self.0.as_str()
  }
}

impl AsRef<str> for PdbId {
  /// Returns the canonical uppercase PDB identifier.
  fn as_ref(&self) -> &str {
    self.as_str()
  }
}

impl fmt::Display for PdbId {
  /// Formats the canonical PDB identifier.
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(self.as_str())
  }
}

impl TryFrom<&str> for PdbId {
  type Error = PdbIdError;

  /// Converts a string slice into a validated PDB identifier.
  fn try_from(value: &str) -> Result<Self, Self::Error> {
    Self::new(value)
  }
}
