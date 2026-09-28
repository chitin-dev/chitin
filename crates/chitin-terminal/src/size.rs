use alacritty_terminal::grid::Dimensions;

/// Character-grid and pixel dimensions reported to a terminal process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalSize {
  /// Number of visible character columns.
  pub columns: usize,
  /// Number of visible character rows.
  pub rows: usize,
  /// Width of one terminal cell in device pixels.
  pub cell_width: u16,
  /// Height of one terminal cell in device pixels.
  pub cell_height: u16,
}

impl TerminalSize {
  /// Creates a terminal size with explicit grid and cell dimensions.
  pub const fn new(columns: usize, rows: usize, cell_width: u16, cell_height: u16) -> Self {
    Self {
      columns,
      rows,
      cell_width,
      cell_height,
    }
  }

  /// Converts this size into the PTY representation.
  pub(crate) fn pty_size(self) -> portable_pty::PtySize {
    let rows = self.rows.min(u16::MAX as usize) as u16;
    let columns = self.columns.min(u16::MAX as usize) as u16;

    portable_pty::PtySize {
      rows,
      cols: columns,
      pixel_width: self.cell_width.saturating_mul(columns),
      pixel_height: self.cell_height.saturating_mul(rows),
    }
  }
}

impl Dimensions for TerminalSize {
  fn total_lines(&self) -> usize {
    self.rows
  }

  fn screen_lines(&self) -> usize {
    self.rows
  }

  fn columns(&self) -> usize {
    self.columns
  }
}

impl Default for TerminalSize {
  fn default() -> Self {
    Self::new(80, 24, 8, 16)
  }
}
