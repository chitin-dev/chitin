//! Strongly typed RCSB identifiers.

use std::fmt;

use super::error::{PdbIdError, PdbIdListError};

/// Canonical four-character RCSB PDB identifier.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PdbId(String);

impl PdbId {
  /// Parses and canonicalizes an RCSB PDB identifier.
  ///
  /// # Parameters
  ///
  /// * `value` is the raw identifier string, such as `"4hhb"` or `"6VXX"`.
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

  /// Parses a comma-separated list of PDB identifiers.
  ///
  /// # Parameters
  ///
  /// * `value` contains comma-separated identifiers. Whitespace around each
  ///   item is ignored.
  ///
  /// # Returns
  ///
  /// Canonical uppercase identifiers in their original input order.
  ///
  /// # Errors
  ///
  /// Returns [`PdbIdListError`] for the first invalid item, including its
  /// one-based position and original substring.
  pub fn parse_many(value: &str) -> Result<Vec<Self>, PdbIdListError> {
    value
      .split(',')
      .map(str::trim)
      .enumerate()
      .map(|(index, value)| {
        Self::new(value).map_err(|source| PdbIdListError {
          index: index + 1,
          value: value.to_string(),
          source,
        })
      })
      .collect()
  }

  /// Returns the canonical uppercase PDB identifier.
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

#[cfg(test)]
mod tests {
  use super::PdbId;

  #[test]
  fn parse_many_should_report_invalid_position_and_value() {
    let error = match PdbId::parse_many("4hhb,invalid,1yth") {
      Ok(_) => panic!("the second ID is invalid"),
      Err(error) => error,
    };

    assert_eq!(error.index, 2);
    assert_eq!(error.value, "invalid");
  }

  #[test]
  fn parse_many_should_reject_empty_elements() {
    let error = match PdbId::parse_many("4hhb,") {
      Ok(_) => panic!("the trailing element is empty"),
      Err(error) => error,
    };

    assert_eq!(error.index, 2);
    assert_eq!(error.value, "");
  }
}
