//! Render-neutral terminal row content.

use gpui::SharedString;

use crate::themes::UIThemes;

/// Semantic foreground tone for one terminal span.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TerminalTone {
  /// Primary terminal content.
  #[default]
  Primary,
  /// De-emphasized paths, metadata, and hints.
  Secondary,
  /// Prompt and other interactive accents.
  Accent,
  /// Successful result content.
  Success,
  /// Recoverable warning content.
  Warning,
  /// Failed or rejected command content.
  Error,
  /// Informational and progress content.
  Info,
}

impl TerminalTone {
  /// Resolves this semantic tone through a UI theme.
  pub(crate) const fn color(self, theme: UIThemes) -> gpui::Rgba {
    match self {
      Self::Primary => theme.text.primary,
      Self::Secondary => theme.text.secondary,
      Self::Accent => theme.accent.primary,
      Self::Success => theme.text.success,
      Self::Warning => theme.text.warning,
      Self::Error => theme.text.error,
      Self::Info => theme.text.info,
    }
  }
}

/// One styled, indivisible text span inside a terminal line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalSpan {
  /// Text rendered by this span.
  pub text: SharedString,
  /// Semantic foreground tone.
  pub tone: TerminalTone,
}

impl TerminalSpan {
  /// Creates a span with a semantic foreground tone.
  pub fn new(text: impl Into<SharedString>, tone: TerminalTone) -> Self {
    Self {
      text: text.into(),
      tone,
    }
  }

  /// Creates primary terminal content.
  pub fn primary(text: impl Into<SharedString>) -> Self {
    Self::new(text, TerminalTone::Primary)
  }

  /// Creates de-emphasized terminal content.
  pub fn secondary(text: impl Into<SharedString>) -> Self {
    Self::new(text, TerminalTone::Secondary)
  }

  /// Creates prompt or interactive accent content.
  pub fn accent(text: impl Into<SharedString>) -> Self {
    Self::new(text, TerminalTone::Accent)
  }

  /// Creates successful terminal content.
  pub fn success(text: impl Into<SharedString>) -> Self {
    Self::new(text, TerminalTone::Success)
  }

  /// Creates warning terminal content.
  pub fn warning(text: impl Into<SharedString>) -> Self {
    Self::new(text, TerminalTone::Warning)
  }

  /// Creates failed terminal content.
  pub fn error(text: impl Into<SharedString>) -> Self {
    Self::new(text, TerminalTone::Error)
  }

  /// Creates informational terminal content.
  pub fn info(text: impl Into<SharedString>) -> Self {
    Self::new(text, TerminalTone::Info)
  }
}

/// One soft-wrapping row of terminal spans.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TerminalLine {
  spans: Vec<TerminalSpan>,
}

impl TerminalLine {
  /// Creates a line from ordered styled spans.
  pub fn new(spans: impl IntoIterator<Item = TerminalSpan>) -> Self {
    Self {
      spans: spans.into_iter().collect(),
    }
  }

  /// Creates a line containing primary text.
  pub fn plain(text: impl Into<SharedString>) -> Self {
    Self::new([TerminalSpan::primary(text)])
  }

  /// Returns the ordered styled spans.
  pub fn spans(&self) -> &[TerminalSpan] {
    &self.spans
  }

  /// Appends one styled span.
  pub fn push(&mut self, span: TerminalSpan) {
    self.spans.push(span);
  }

  /// Returns whether the line contains no text spans.
  pub fn is_empty(&self) -> bool {
    self.spans.is_empty()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn line_should_preserve_span_order() {
    let line = TerminalLine::new([TerminalSpan::accent("❯ "), TerminalSpan::primary("help")]);

    assert_eq!(line.spans()[1].text, "help");
  }
}
