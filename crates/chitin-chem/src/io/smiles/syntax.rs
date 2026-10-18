//! Positioned lexical facts. This layer does not allocate domain graph handles.

use std::ops::Range;

use crate::{Atom, BondOrder, Element, ImplicitHydrogenPolicy, Isotope, TetrahedralWinding};

use super::{SmilesError, SmilesErrorKind, error, profile};

/// One positioned lexical fact, before any graph-local handles are allocated.
#[derive(Debug)]
pub(super) struct Token {
  /// Half-open UTF-8 byte range covering the entire token.
  pub span: Range<usize>,
  /// Atomic source facts or a topology control symbol, never inferred chemistry.
  pub kind: Kind,
}

/// Supported lexical categories; unsupported annotations never become tokens.
#[derive(Debug)]
pub(super) enum Kind {
  /// Atom facts and optional local winding; graph ligand identities come later.
  Atom(Atom, Option<TetrahedralWinding>),
  /// Explicit order awaiting an atom or ring site in the topology parser.
  Bond(BondOrder),
  /// Numeric ring identity, independent of digit versus percent spelling.
  Ring(u8),
  /// Starts a branch whose parent is resolved by the topology parser.
  Open,
  /// Ends a branch and restores its parent.
  Close,
  /// Stops the current chain without discarding pending ring identities.
  Dot,
}

/// Scans the entire input before exposing tokens to graph construction.
///
/// # Parameters
/// - `source`: exact SMILES input, without trimming or record-name handling.
///
/// # Returns
/// Tokens in source order with byte-aligned spans, or an empty vector for empty
/// input. Bracket fields remain supplied facts rather than perception results.
///
/// # Errors
/// Returns the first lexical, unsupported-feature, or representation error.
/// Local scanner progress on failure is discarded together with all tokens;
/// this layer neither recovers malformed input nor returns a partial graph.
pub(super) fn scan(source: &str) -> Result<Vec<Token>, SmilesError> {
  let mut scanner = Scanner { source, offset: 0 };
  let mut tokens = Vec::new();
  while scanner.peek().is_some() {
    let start = scanner.offset;
    let kind = scanner.token()?;
    tokens.push(Token {
      span: start..scanner.offset,
      kind,
    });
  }
  Ok(tokens)
}

/// Cursor over borrowed UTF-8 input; successful advances consume ASCII only.
///
/// `offset` always lies on a character boundary. Non-ASCII input is reported at
/// its full character range without incrementing the cursor into a code point.
struct Scanner<'a> {
  /// Source storage remains unchanged so token ranges stay valid.
  source: &'a str,
  /// Byte index of the next unconsumed character, including end-of-input.
  offset: usize,
}

impl Scanner<'_> {
  /// Returns the next raw byte, or `None` at end-of-input, without advancing.
  fn peek(&self) -> Option<u8> {
    self.source.as_bytes().get(self.offset).copied()
  }

  /// Covers one full current UTF-8 character, or an empty end-of-input location.
  fn here(&self) -> Range<usize> {
    let width = self.source[self.offset..].chars().next().map_or(0, char::len_utf8);
    self.offset..self.offset + width
  }

  /// Positions a supplied failure at the current cursor without consuming input.
  fn fail(&self, kind: SmilesErrorKind) -> SmilesError {
    error(self.here(), kind)
  }

  /// Consumes one lexical token, including the full spelling of bracket atoms.
  ///
  /// # Returns
  /// One supported category and an advanced byte cursor. Percent ring labels
  /// consume exactly two following digits; they do not close graph edges here.
  ///
  /// # Errors
  /// Rejects malformed, unsupported, or out-of-range tokens. The cursor may have
  /// advanced on failure; `scan` owns it and never resumes a failed scan.
  fn token(&mut self) -> Result<Kind, SmilesError> {
    let byte = self
      .peek()
      .ok_or_else(|| self.fail(SmilesErrorKind::Syntax("expected token")))?;
    let kind = match byte {
      b'[' => return self.bracket().map(|(atom, winding)| Kind::Atom(atom, winding)),
      b'A'..=b'Z' => return self.bare().map(|atom| Kind::Atom(atom, None)),
      b'b' | b'c' | b'n' | b'o' | b'p' | b's' => {
        return self.aromatic(false).map(|atom| {
          Kind::Atom(
            atom
              .with_formal_charge(Some(0))
              .with_declared_hydrogens(Some(0))
              .with_unpaired_electrons(Some(0))
              .with_implicit_hydrogen_policy(ImplicitHydrogenPolicy::Infer),
            None,
          )
        });
      }
      b'0'..=b'9' => Kind::Ring(byte - b'0'),
      b'%' => {
        self.offset += 1;
        let start = self.offset;
        for _ in 0..2 {
          if !self.peek().is_some_and(|b| b.is_ascii_digit()) {
            return Err(self.fail(SmilesErrorKind::Syntax("expected two digits after '%'")));
          }
          self.offset += 1;
        }
        let label = self.source[start..self.offset]
          .parse::<u8>()
          .map_err(|_| error(start..self.offset, SmilesErrorKind::NumberOutOfRange("ring label")))?;
        return Ok(Kind::Ring(label));
      }
      b'-' => Kind::Bond(BondOrder::Single),
      b'=' => Kind::Bond(BondOrder::Double),
      b'#' => Kind::Bond(BondOrder::Triple),
      b':' => Kind::Bond(BondOrder::Aromatic),
      b'(' => Kind::Open,
      b')' => Kind::Close,
      b'.' => Kind::Dot,
      _ => return Err(self.unsupported_or_syntax()),
    };
    self.offset += 1;
    Ok(kind)
  }

  /// Distinguishes recognized missing capabilities from unexpected characters.
  ///
  /// This is a fail-fast diagnostic classification, not a query/stereo parser;
  /// reporting a feature as unsupported does not validate its remaining syntax.
  fn unsupported_or_syntax(&self) -> SmilesError {
    let feature = match self.peek() {
      Some(b':') => "atom classes",
      Some(b'@' | b'/' | b'\\') => "stereochemistry",
      Some(b'*' | b'?' | b'!' | b'&' | b';' | b'~' | b',') => "wildcard/query syntax",
      Some(b'>' | b'|') => "reaction or CXSMILES extension",
      Some(b'$') => "quadruple bonds",
      _ => return self.fail(SmilesErrorKind::Syntax("unexpected character")),
    };
    self.fail(SmilesErrorKind::Unsupported(feature))
  }

  /// Consumes one unbracketed organic-subset atom with explicit reader defaults.
  ///
  /// # Returns
  /// An unlabelled neutral, closed-shell input atom with zero declared H and
  /// permission for later implicit-H inference. No H count is computed here.
  /// Only `Cl`/`Br` consume a second character outside brackets.
  ///
  /// # Errors
  /// Rejects symbols requiring brackets; source ranges refer to the bare token.
  fn bare(&mut self) -> Result<Atom, SmilesError> {
    let start = self.offset;
    self.offset += 1;
    if matches!(&self.source[start..self.offset], "C" | "B") && matches!(self.peek(), Some(b'l' | b'r')) {
      let pair = &self.source[start..self.offset + 1];
      if matches!(pair, "Cl" | "Br") {
        self.offset += 1;
      }
    }
    let symbol = &self.source[start..self.offset];
    let element = match Element::from_symbol(symbol) {
      Ok(element) if profile::organic_subset(element) => element,
      _ => {
        return Err(error(
          start..self.offset,
          SmilesErrorKind::Syntax("this element requires brackets"),
        ));
      }
    };
    Ok(
      Atom::new(element)
        .with_formal_charge(Some(0))
        .with_declared_hydrogens(Some(0))
        .with_unpaired_electrons(Some(0))
        .with_implicit_hydrogen_policy(ImplicitHydrogenPolicy::Infer),
    )
  }

  /// Consumes a lowercase atomic designation without perceiving aromaticity.
  ///
  /// `bracket` permits the two-letter `as`/`se` forms. Unknown lowercase
  /// symbols are syntax errors; the surrounding grammar owns H/charge defaults.
  fn aromatic(&mut self, bracket: bool) -> Result<Atom, SmilesError> {
    let start = self.offset;
    let tail = &self.source[start..];
    let width = if bracket && (tail.starts_with("as") || tail.starts_with("se")) {
      2
    } else {
      1
    };
    let symbol = self
      .source
      .get(start..start + width)
      .ok_or_else(|| self.fail(SmilesErrorKind::Syntax("expected aromatic symbol")))?;
    let element = Element::ALL
      .iter()
      .copied()
      .find(|&element| profile::aromatic_symbol(element) == Some(symbol))
      .ok_or_else(|| self.fail(SmilesErrorKind::Syntax("unknown aromatic symbol")))?;
    self.offset += width;
    Ok(Atom::new(element).with_aromatic(true))
  }

  /// Consumes an optional ASCII digit run under a caller-selected lexical bound.
  ///
  /// # Parameters
  /// - `name`: field identity retained in numeric-range diagnostics.
  /// - `max_digits`: spelling limit, not the width of the domain integer type.
  ///
  /// # Returns
  /// `None` without advancing when no digits occur; otherwise a checked value.
  /// Leading zeros still count toward the lexical width, so they cannot bypass it.
  ///
  /// # Errors
  /// Reports an oversized/unrepresentable run at its complete source range.
  fn number(&mut self, name: &'static str, max_digits: usize) -> Result<Option<u16>, SmilesError> {
    let start = self.offset;
    while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
      self.offset += 1;
    }
    if start == self.offset {
      return Ok(None);
    }
    // OpenSMILES bounds these fields lexically. The wider domain integer
    // types must not silently turn longer numeric spellings into a dialect.
    if self.offset - start > max_digits {
      return Err(error(start..self.offset, SmilesErrorKind::NumberOutOfRange(name)));
    }
    self.source[start..self.offset]
      .parse::<u16>()
      .map(Some)
      .map_err(|_| error(start..self.offset, SmilesErrorKind::NumberOutOfRange(name)))
  }

  /// Consumes `[isotope? element chirality? H-count? charge?]` without perception.
  ///
  /// # Returns
  /// One atom with known charge/H defaults and fixed zero additional implicit H.
  /// Isotope H stays an atom, whereas an attached-H declaration creates no
  /// vertices. Radical state remains unknown even for apparently saturated input.
  ///
  /// # Errors
  /// Rejects unsupported annotations, missing delimiters, invalid element/isotope
  /// labels, out-of-range numbers, and H declarations hosted on a hydrogen atom.
  /// Partial fields never escape the failed scan.
  fn bracket(&mut self) -> Result<(Atom, Option<TetrahedralWinding>), SmilesError> {
    self.offset += 1;
    let isotope_start = self.offset;
    let isotope = self.number("isotope mass number (at most three digits)", profile::ISOTOPE_DIGITS)?;
    let symbol_start = self.offset;
    let base = if self.peek().is_some_and(|b| b.is_ascii_lowercase()) {
      self.aromatic(true)?
    } else {
      if !self.peek().is_some_and(|b| b.is_ascii_uppercase()) {
        return Err(self.unsupported_or_syntax());
      }
      self.offset += 1;
      if self.peek().is_some_and(|b| b.is_ascii_lowercase()) {
        self.offset += 1;
      }
      let span = symbol_start..self.offset;
      Atom::new(Element::from_symbol(&self.source[span.clone()]).map_err(|e| error(span, e.into()))?)
    };
    let element = base.element();
    let atom = match isotope {
      Some(mass) => {
        Atom::from_isotope(Isotope::new(element, mass).map_err(|e| error(isotope_start..symbol_start, e.into()))?)
          .with_aromatic(base.is_aromatic())
      }
      None => base,
    };
    let winding = self.chirality()?;
    let hydrogens = if self.peek() == Some(b'H') {
      if element == Element::Hydrogen {
        return Err(self.fail(SmilesErrorKind::Syntax("a hydrogen atom cannot declare attached H")));
      }
      self.offset += 1;
      let start = self.offset;
      let count = self
        .number("declared hydrogen count (one digit)", profile::HYDROGEN_DIGITS)?
        .unwrap_or(1);
      u8::try_from(count).map_err(|_| {
        error(
          start..self.offset,
          SmilesErrorKind::NumberOutOfRange("declared hydrogen count"),
        )
      })?
    } else {
      0
    };
    let charge = self.charge()?;
    if self.peek() != Some(b']') {
      return Err(self.unsupported_or_syntax());
    }
    self.offset += 1;
    // Brackets specify H exactly but not a radical count. Inferring a zero
    // radical here would make [C] look like a known closed-shell carbon atom.
    Ok((
      atom
        .with_formal_charge(Some(charge))
        .with_declared_hydrogens(Some(hydrogens))
        .with_implicit_hydrogen_policy(ImplicitHydrogenPolicy::Fixed(0)),
      winding,
    ))
  }

  /// Consumes tetrahedral shorthand/TH classes, without accepting other stereo.
  ///
  /// # Errors
  /// Extended coordination classes remain unsupported. TH indices outside
  /// 1/2 and repeated markers are malformed rather than silently shortened.
  fn chirality(&mut self) -> Result<Option<TetrahedralWinding>, SmilesError> {
    if self.peek() != Some(b'@') {
      return Ok(None);
    }
    self.offset += 1;
    let winding = if self.peek() == Some(b'@') {
      self.offset += 1;
      TetrahedralWinding::Clockwise
    } else if self.source[self.offset..].starts_with("TH") {
      self.offset += 2;
      let winding = match self.peek() {
        Some(b'1') => TetrahedralWinding::Anticlockwise,
        Some(b'2') => TetrahedralWinding::Clockwise,
        _ => return Err(self.fail(SmilesErrorKind::Syntax("expected TH1 or TH2"))),
      };
      self.offset += 1;
      winding
    } else {
      TetrahedralWinding::Anticlockwise
    };
    if self
      .peek()
      .is_some_and(|byte| byte.is_ascii_alphabetic() && byte != b'H')
    {
      return Err(self.fail(SmilesErrorKind::Unsupported("non-tetrahedral stereochemistry")));
    }
    Ok(Some(winding))
  }

  /// Consumes an optional signed numeric charge or the legacy `++`/`--` form.
  ///
  /// # Returns
  /// Zero without advancing if the field is absent, as required by bracket
  /// syntax. A lone sign means magnitude one; a repeated sign means two.
  ///
  /// # Errors
  /// Returns a positioned range error for unsupported numeric widths/values.
  /// Trailing or mixed signs are left for the enclosing bracket grammar to reject.
  fn charge(&mut self) -> Result<i16, SmilesError> {
    let sign = match self.peek() {
      Some(b'+') => 1_i32,
      Some(b'-') => -1_i32,
      _ => return Ok(0),
    };
    let start = self.offset;
    let marker = self.peek();
    self.offset += 1;
    let magnitude = if self.peek() == marker {
      self.offset += 1;
      2
    } else {
      self
        .number("formal charge (at most two digits)", profile::CHARGE_DIGITS)?
        .unwrap_or(1)
    };
    i16::try_from(sign * i32::from(magnitude))
      .map_err(|_| error(start..self.offset, SmilesErrorKind::NumberOutOfRange("formal charge")))
  }
}
