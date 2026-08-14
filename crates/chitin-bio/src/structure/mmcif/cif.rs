//! Schema-independent mmCIF tokenization and document representation.
//!
//! This layer intentionally does not interpret biological categories. Format
//! adapters such as the atom-site reader can consume its preserved tags and
//! values without duplicating quoting, comments, or missing-value handling.

use std::fmt;

/// A scalar value or loop cell from an mmCIF document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CifValue {
  /// A concrete text value.
  Text(String),
  /// The CIF missing-value marker.
  Missing,
  /// The CIF unknown-value marker.
  Unknown,
}

impl CifValue {
  /// Returns the text value, excluding CIF missing and unknown markers.
  pub fn as_text(&self) -> Option<&str> {
    match self {
      Self::Text(value) => Some(value),
      Self::Missing | Self::Unknown => None,
    }
  }
}

/// A category item or a loop in a CIF data block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CifCategory {
  /// One scalar tag/value pair.
  Item { tag: String, value: CifValue },
  /// Column tags and row-major loop values.
  Loop {
    tags: Vec<String>,
    rows: Vec<Vec<CifValue>>,
  },
}

/// One named mmCIF data block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CifDataBlock {
  /// Name after the data_ prefix.
  pub name: String,
  /// Categories in source order.
  pub categories: Vec<CifCategory>,
}

/// Parsed mmCIF document independent of any biological schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CifDocument {
  /// Data blocks in source order.
  pub blocks: Vec<CifDataBlock>,
}

/// Generic mmCIF tokenizer and document parser.
#[derive(Debug, Clone, Copy, Default)]
pub struct CifParser;

impl CifParser {
  /// Parses a complete UTF-8 mmCIF document into data blocks and categories.
  ///
  /// # Parameters
  ///
  /// * `input` is the complete UTF-8 CIF document.
  ///
  /// # Returns
  ///
  /// A document preserving source order, quoted values, multiline text, and
  /// the distinction between missing and unknown values.
  ///
  /// # Examples
  ///
  /// ```
  /// use chitin_bio::structure::cif::CifParser;
  ///
  /// let document = CifParser::parse("data_demo\n_entry.id 4HHB\n")?;
  /// assert_eq!(document.blocks[0].name, "demo");
  /// # Ok::<(), Box<dyn std::error::Error>>(())
  /// ```
  pub fn parse(input: &str) -> Result<CifDocument, CifParseError> {
    let tokens = tokenize(input)?;
    let mut blocks = Vec::new();
    let mut cursor = 0;

    while cursor < tokens.len() {
      if let Some(name) = tokens[cursor].text.strip_prefix("data_") {
        blocks.push(CifDataBlock {
          name: name.to_owned(),
          categories: Vec::new(),
        });
        cursor += 1;
        continue;
      }
      let Some(block) = blocks.last_mut() else {
        return Err(CifParseError::new(
          tokens[cursor].line,
          "content appears before data_ block",
        ));
      };

      if tokens[cursor].text == "loop_" {
        cursor += 1;
        let tag_start = cursor;
        while cursor < tokens.len() && tokens[cursor].text.starts_with('_') {
          cursor += 1;
        }
        if tag_start == cursor {
          return Err(CifParseError::new(tokens[cursor - 1].line, "loop_ has no tags"));
        }
        let tags = tokens[tag_start..cursor]
          .iter()
          .map(|token| token.text.clone())
          .collect::<Vec<_>>();
        let value_start = cursor;
        while cursor < tokens.len() && !is_control_token(&tokens[cursor].text) {
          cursor += 1;
        }
        let values = &tokens[value_start..cursor];
        if values.is_empty() || !values.len().is_multiple_of(tags.len()) {
          return Err(CifParseError::new(
            tokens[tag_start].line,
            "loop values do not form complete rows",
          ));
        }
        let rows = values
          .chunks(tags.len())
          .map(|row| row.iter().map(|token| token.value.clone()).collect())
          .collect();
        block.categories.push(CifCategory::Loop { tags, rows });
        continue;
      }

      if tokens[cursor].text.starts_with('_') {
        let tag = tokens[cursor].text.clone();
        cursor += 1;
        let Some(value) = tokens.get(cursor) else {
          return Err(CifParseError::new(tokens[cursor - 1].line, "tag has no value"));
        };
        block.categories.push(CifCategory::Item {
          tag,
          value: value.value.clone(),
        });
        cursor += 1;
        continue;
      }

      cursor += 1;
    }

    if blocks.is_empty() {
      return Err(CifParseError::new(0, "document has no data_ block"));
    }
    Ok(CifDocument { blocks })
  }
}

#[derive(Debug, Clone)]
struct Token {
  text: String,
  value: CifValue,
  line: usize,
}

/// A syntax error with the source line where it was detected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CifParseError {
  /// One-based source line, or zero for document-level errors.
  pub line: usize,
  /// Human-readable parser explanation.
  pub message: String,
}

impl CifParseError {
  fn new(line: usize, message: impl Into<String>) -> Self {
    Self {
      line,
      message: message.into(),
    }
  }
}

impl fmt::Display for CifParseError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(formatter, "line {}: {}", self.line, self.message)
  }
}

impl std::error::Error for CifParseError {}

/// Tokenizes CIF whitespace, quoted values, comments, and text fields.
///
/// # Parameters
///
/// * `input` is the complete UTF-8 document.
///
/// # Returns
///
/// Tokens with decoded values and source lines, or a syntax error for an
/// unterminated quoted or semicolon-delimited value.
fn tokenize(input: &str) -> Result<Vec<Token>, CifParseError> {
  // this is a hand-written lexer
  let bytes = input.as_bytes();
  let mut tokens = Vec::new();
  let mut position = 0;
  let mut line = 1;

  while position < bytes.len() {
    while position < bytes.len() && bytes[position].is_ascii_whitespace() {
      // handle the whitespace
      if bytes[position] == b'\n' {
        line += 1;
      }
      position += 1;
    }
    if position >= bytes.len() {
      break;
    }
    if bytes[position] == b'#' {
      // parse comment, comment will be skipped
      while position < bytes.len() && bytes[position] != b'\n' {
        position += 1;
      }
      continue;
    }

    let token_line = line;
    let first = bytes[position];
    if first == b'\'' || first == b'"' {
      let quote = first;
      position += 1;
      let start = position;
      while position < bytes.len() && bytes[position] != quote {
        if bytes[position] == b'\n' {
          line += 1;
        }
        position += 1;
      }
      if position >= bytes.len() {
        return Err(CifParseError::new(token_line, "unterminated quoted value"));
      }
      let text = input[start..position].to_owned();
      tokens.push(Token {
        text: text.clone(),
        value: CifValue::Text(text),
        line: token_line,
      });
      position += 1;
      continue;
    }

    if first == b';' && (position == 0 || bytes[position - 1] == b'\n') {
      position += 1;
      let start = position;
      let mut closed = false;
      while position < bytes.len() {
        if (position == 0 || bytes[position - 1] == b'\n') && bytes[position] == b';' {
          let text = input[start..position].trim_end_matches('\n').to_owned();
          tokens.push(Token {
            text: text.clone(),
            value: CifValue::Text(text),
            line: token_line,
          });
          closed = true;
          while position < bytes.len() && bytes[position] != b'\n' {
            position += 1;
          }
          break;
        }
        if bytes[position] == b'\n' {
          line += 1;
        }
        position += 1;
      }
      if !closed {
        return Err(CifParseError::new(token_line, "unterminated semicolon text value"));
      }
      continue;
    }

    let start = position;
    while position < bytes.len() && !bytes[position].is_ascii_whitespace() && bytes[position] != b'#' {
      position += 1;
    }
    let text = input[start..position].to_owned();
    let value = match text.as_str() {
      "." => CifValue::Missing,
      "?" => CifValue::Unknown,
      _ => CifValue::Text(text.clone()),
    };
    tokens.push(Token {
      text,
      value,
      line: token_line,
    });
  }
  Ok(tokens)
}

/// Reports whether a token begins a new CIF construct rather than a loop cell.
fn is_control_token(value: &str) -> bool {
  value == "loop_" || value.starts_with("data_") || value.starts_with('_')
}

#[cfg(test)]
mod tests {
  use super::{CifCategory, CifParser, CifValue};

  #[test]
  fn parses_scalar_and_loop_categories() {
    let document = CifParser::parse(
      r#"data_demo
_entry.id 4HHB
loop_
_atom_site.id
_atom_site.label_atom_id
1 CA
2 C
"#,
    )
    .unwrap_or_else(|error| panic!("fixture should parse: {error}"));

    assert_eq!(document.blocks[0].name, "demo");
    assert!(matches!(document.blocks[0].categories[0], CifCategory::Item { .. }));
    assert!(matches!(document.blocks[0].categories[1], CifCategory::Loop { .. }));
  }

  #[test]
  fn preserves_missing_and_unknown_values() {
    let document =
      CifParser::parse("data_demo\n_a .\n_b ?\n").unwrap_or_else(|error| panic!("fixture should parse: {error}"));
    assert!(matches!(
      document.blocks[0].categories[0],
      CifCategory::Item {
        value: CifValue::Missing,
        ..
      }
    ));
    assert!(matches!(
      document.blocks[0].categories[1],
      CifCategory::Item {
        value: CifValue::Unknown,
        ..
      }
    ));
  }
}
