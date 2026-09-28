use alacritty_terminal::{
  Term,
  event::EventListener,
  grid::Dimensions,
  term::cell::Flags,
  vte::ansi::{Color, NamedColor},
};

/// A frontend-neutral copy of the visible terminal grid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalSnapshot {
  /// Number of visible columns.
  pub columns: usize,
  /// Number of visible rows.
  pub rows: usize,
  /// Visible cells stored in row-major order.
  pub cells: Vec<TerminalCell>,
  /// Visible cursor location, when the terminal requests a cursor.
  pub cursor: Option<(usize, usize)>,
}

impl TerminalSnapshot {
  /// Copies the visible grid from an Alacritty terminal state machine.
  pub(crate) fn from_term<T: EventListener>(term: &Term<T>) -> Self {
    let content = term.renderable_content();
    let columns = term.columns();
    let rows = term.screen_lines();
    let cursor = (!matches!(content.cursor.shape, alacritty_terminal::vte::ansi::CursorShape::Hidden)).then(|| {
      (
        content.cursor.point.line.0.max(0) as usize,
        content.cursor.point.column.0,
      )
    });
    let mut cells = vec![TerminalCell::default(); rows.saturating_mul(columns)];

    for indexed in content.display_iter {
      let row = indexed.point.line.0.max(0) as usize;
      let column = indexed.point.column.0;
      let Some(cell) = cells.get_mut(row.saturating_mul(columns).saturating_add(column)) else {
        continue;
      };

      *cell = TerminalCell {
        character: indexed.cell.c,
        combining_characters: indexed.cell.zerowidth().unwrap_or_default().to_vec(),
        foreground: indexed.cell.fg.into(),
        background: indexed.cell.bg.into(),
        attributes: TerminalCellAttributes {
          bold: indexed.cell.flags.intersects(Flags::BOLD | Flags::DIM_BOLD),
          dim: indexed.cell.flags.intersects(Flags::DIM | Flags::DIM_BOLD),
          italic: indexed.cell.flags.intersects(Flags::ITALIC | Flags::BOLD_ITALIC),
          underline: indexed.cell.flags.intersects(Flags::ALL_UNDERLINES),
          inverse: indexed.cell.flags.contains(Flags::INVERSE),
          hidden: indexed.cell.flags.contains(Flags::HIDDEN),
          wide_spacer: indexed
            .cell
            .flags
            .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER),
        },
      };
    }

    Self {
      columns,
      rows,
      cells,
      cursor,
    }
  }

  /// Returns the cell at a visible grid coordinate.
  pub fn cell(&self, row: usize, column: usize) -> Option<&TerminalCell> {
    if row >= self.rows || column >= self.columns {
      return None;
    }

    self.cells.get(row * self.columns + column)
  }
}

/// Character and visual attributes for one terminal grid cell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalCell {
  /// Unicode scalar displayed in the cell.
  pub character: char,
  /// Zero-width scalars that complete the cell's displayed grapheme.
  pub combining_characters: Vec<char>,
  /// Requested foreground color.
  pub foreground: TerminalColor,
  /// Requested background color.
  pub background: TerminalColor,
  /// Additional text attributes.
  pub attributes: TerminalCellAttributes,
}

impl Default for TerminalCell {
  fn default() -> Self {
    Self {
      character: ' ',
      combining_characters: Vec::new(),
      foreground: TerminalColor::Named(TerminalNamedColor::Foreground),
      background: TerminalColor::Named(TerminalNamedColor::Background),
      attributes: TerminalCellAttributes::default(),
    }
  }
}

/// Font and compositing attributes attached to one terminal cell.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TerminalCellAttributes {
  pub bold: bool,
  pub dim: bool,
  pub italic: bool,
  pub underline: bool,
  pub inverse: bool,
  pub hidden: bool,
  pub wide_spacer: bool,
}

/// A terminal color before it is resolved against the UI palette.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalColor {
  Named(TerminalNamedColor),
  Indexed(u8),
  Rgb { red: u8, green: u8, blue: u8 },
}

impl From<Color> for TerminalColor {
  fn from(value: Color) -> Self {
    match value {
      Color::Named(color) => Self::Named(color.into()),
      Color::Indexed(index) => Self::Indexed(index),
      Color::Spec(color) => Self::Rgb {
        red: color.r,
        green: color.g,
        blue: color.b,
      },
    }
  }
}

/// Semantic and ANSI named colors understood by the VT state machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalNamedColor {
  Black,
  Red,
  Green,
  Yellow,
  Blue,
  Magenta,
  Cyan,
  White,
  BrightBlack,
  BrightRed,
  BrightGreen,
  BrightYellow,
  BrightBlue,
  BrightMagenta,
  BrightCyan,
  BrightWhite,
  Foreground,
  Background,
  Cursor,
  DimBlack,
  DimRed,
  DimGreen,
  DimYellow,
  DimBlue,
  DimMagenta,
  DimCyan,
  DimWhite,
  BrightForeground,
  DimForeground,
}

impl From<NamedColor> for TerminalNamedColor {
  fn from(value: NamedColor) -> Self {
    match value {
      NamedColor::Black => Self::Black,
      NamedColor::Red => Self::Red,
      NamedColor::Green => Self::Green,
      NamedColor::Yellow => Self::Yellow,
      NamedColor::Blue => Self::Blue,
      NamedColor::Magenta => Self::Magenta,
      NamedColor::Cyan => Self::Cyan,
      NamedColor::White => Self::White,
      NamedColor::BrightBlack => Self::BrightBlack,
      NamedColor::BrightRed => Self::BrightRed,
      NamedColor::BrightGreen => Self::BrightGreen,
      NamedColor::BrightYellow => Self::BrightYellow,
      NamedColor::BrightBlue => Self::BrightBlue,
      NamedColor::BrightMagenta => Self::BrightMagenta,
      NamedColor::BrightCyan => Self::BrightCyan,
      NamedColor::BrightWhite => Self::BrightWhite,
      NamedColor::Foreground => Self::Foreground,
      NamedColor::Background => Self::Background,
      NamedColor::Cursor => Self::Cursor,
      NamedColor::DimBlack => Self::DimBlack,
      NamedColor::DimRed => Self::DimRed,
      NamedColor::DimGreen => Self::DimGreen,
      NamedColor::DimYellow => Self::DimYellow,
      NamedColor::DimBlue => Self::DimBlue,
      NamedColor::DimMagenta => Self::DimMagenta,
      NamedColor::DimCyan => Self::DimCyan,
      NamedColor::DimWhite => Self::DimWhite,
      NamedColor::BrightForeground => Self::BrightForeground,
      NamedColor::DimForeground => Self::DimForeground,
    }
  }
}

#[cfg(test)]
mod tests {
  use alacritty_terminal::{
    Term,
    event::VoidListener,
    term::Config,
    vte::ansi::{Processor, StdSyncHandler},
  };

  use super::*;
  use crate::TerminalSize;

  #[test]
  fn snapshot_should_preserve_visible_text_and_sgr_attributes() {
    let size = TerminalSize::new(12, 3, 8, 16);
    let mut term = Term::new(Config::default(), &size, VoidListener);
    let mut processor = Processor::<StdSyncHandler>::new();

    processor.advance(&mut term, b"plain \x1b[1;31mbold\x1b[0m");
    let snapshot = TerminalSnapshot::from_term(&term);

    assert_eq!(snapshot.cell(0, 0).map(|cell| cell.character), Some('p'));
    assert_eq!(snapshot.cell(0, 6).map(|cell| cell.character), Some('b'));
    assert_eq!(snapshot.cell(0, 6).map(|cell| cell.attributes.bold), Some(true));
    assert_eq!(
      snapshot.cell(0, 6).map(|cell| cell.foreground),
      Some(TerminalColor::Named(TerminalNamedColor::Red)),
    );
  }

  #[test]
  fn snapshot_should_preserve_emoji_variation_selectors() {
    let size = TerminalSize::new(12, 3, 8, 16);
    let mut term = Term::new(Config::default(), &size, VoidListener);
    let mut processor = Processor::<StdSyncHandler>::new();

    processor.advance(&mut term, "☀️".as_bytes());
    let snapshot = TerminalSnapshot::from_term(&term);

    assert_eq!(snapshot.cell(0, 0).map(|cell| cell.character), Some('☀'));
    assert_eq!(
      snapshot.cell(0, 0).map(|cell| cell.combining_characters.as_slice()),
      Some(&['\u{fe0f}'][..])
    );
  }

  #[test]
  fn cell_should_reject_coordinates_outside_the_visible_grid() {
    let snapshot = TerminalSnapshot {
      columns: 2,
      rows: 1,
      cells: vec![TerminalCell::default(); 2],
      cursor: None,
    };

    assert!(snapshot.cell(0, 1).is_some());
    assert!(snapshot.cell(1, 0).is_none());
    assert!(snapshot.cell(0, 2).is_none());
  }
}
