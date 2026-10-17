//! Positioned lexical facts. This layer does not allocate domain graph handles.

use std::ops::Range;

use crate::{Atom, BondOrder, Element, ImplicitHydrogenPolicy, Isotope};

use super::{SmilesError, SmilesErrorKind, error};

#[derive(Debug)]
pub(super) struct Token {
  pub span: Range<usize>,
  pub kind: Kind,
}

#[derive(Debug)]
pub(super) enum Kind {
  Atom(Atom),
  Bond(BondOrder),
  Ring(u8),
  Open,
  Close,
  Dot,
}

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

struct Scanner<'a> {
  source: &'a str,
  offset: usize,
}

impl Scanner<'_> {
  fn peek(&self) -> Option<u8> {
    self.source.as_bytes().get(self.offset).copied()
  }

  fn here(&self) -> Range<usize> {
    let width = self.source[self.offset..].chars().next().map_or(0, char::len_utf8);
    self.offset..self.offset + width
  }

  fn fail(&self, kind: SmilesErrorKind) -> SmilesError {
    error(self.here(), kind)
  }

  fn token(&mut self) -> Result<Kind, SmilesError> {
    let byte = self
      .peek()
      .ok_or_else(|| self.fail(SmilesErrorKind::Syntax("expected token")))?;
    let kind = match byte {
      b'[' => return self.bracket().map(Kind::Atom),
      b'A'..=b'Z' => return self.bare().map(Kind::Atom),
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
      b'(' => Kind::Open,
      b')' => Kind::Close,
      b'.' => Kind::Dot,
      _ => return Err(self.unsupported_or_syntax()),
    };
    self.offset += 1;
    Ok(kind)
  }

  fn unsupported_or_syntax(&self) -> SmilesError {
    let feature = match self.peek() {
      Some(b'a' | b'b' | b'c' | b'n' | b'o' | b'p' | b's' | b':') => "aromatic atoms/bonds or atom classes",
      Some(b'@' | b'/' | b'\\') => "stereochemistry",
      Some(b'*' | b'?' | b'!' | b'&' | b';' | b'~' | b',') => "wildcard/query syntax",
      Some(b'>' | b'|') => "reaction or CXSMILES extension",
      Some(b'$') => "quadruple bonds",
      _ => return self.fail(SmilesErrorKind::Syntax("unexpected character")),
    };
    self.fail(SmilesErrorKind::Unsupported(feature))
  }

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
    if !matches!(symbol, "B" | "C" | "N" | "O" | "P" | "S" | "F" | "Cl" | "Br" | "I") {
      return Err(error(
        start..self.offset,
        SmilesErrorKind::Syntax("this element requires brackets"),
      ));
    }
    let element = Element::from_symbol(symbol).map_err(|e| error(start..self.offset, e.into()))?;
    Ok(
      Atom::new(element)
        .with_formal_charge(Some(0))
        .with_declared_hydrogens(Some(0))
        .with_unpaired_electrons(Some(0))
        .with_implicit_hydrogen_policy(ImplicitHydrogenPolicy::Infer),
    )
  }

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

  fn bracket(&mut self) -> Result<Atom, SmilesError> {
    self.offset += 1;
    let isotope_start = self.offset;
    let isotope = self.number("isotope mass number (at most three digits)", 3)?;
    let symbol_start = self.offset;
    if !self.peek().is_some_and(|b| b.is_ascii_uppercase()) {
      return Err(self.unsupported_or_syntax());
    }
    self.offset += 1;
    if self.peek().is_some_and(|b| b.is_ascii_lowercase()) {
      self.offset += 1;
    }
    let symbol_span = symbol_start..self.offset;
    let element = Element::from_symbol(&self.source[symbol_span.clone()]).map_err(|e| error(symbol_span, e.into()))?;
    let atom = match isotope {
      Some(mass) => {
        Atom::from_isotope(Isotope::new(element, mass).map_err(|e| error(isotope_start..symbol_start, e.into()))?)
      }
      None => Atom::new(element),
    };
    let hydrogens = if self.peek() == Some(b'H') {
      if element == Element::Hydrogen {
        return Err(self.fail(SmilesErrorKind::Syntax("a hydrogen atom cannot declare attached H")));
      }
      self.offset += 1;
      let start = self.offset;
      let count = self.number("declared hydrogen count (one digit)", 1)?.unwrap_or(1);
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
    Ok(
      atom
        .with_formal_charge(Some(charge))
        .with_declared_hydrogens(Some(hydrogens))
        .with_implicit_hydrogen_policy(ImplicitHydrogenPolicy::Fixed(0)),
    )
  }

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
      self.number("formal charge (at most two digits)", 2)?.unwrap_or(1)
    };
    i16::try_from(sign * i32::from(magnitude))
      .map_err(|_| error(start..self.offset, SmilesErrorKind::NumberOutOfRange("formal charge")))
  }
}
