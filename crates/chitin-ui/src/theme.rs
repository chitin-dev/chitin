//! Shared GPUI Kit initialization and Chitin theme policy.

use gpui::App;
use gpui_kit::component::{Theme, ThemeMode};

/// Initializes Kit controls and selects its built-in dark theme.
///
/// # Parameters
///
/// * `cx` is the application context, before creating any Kit windows or controls.
///
/// # Returns
///
/// Nothing. Kit's global theme and base theme tokens are ready for window creation.
pub fn init(cx: &mut App) {
  gpui_kit::init(cx);
  Theme::change(ThemeMode::Dark, None, cx);
  // Document chrome shares the inset workbench panels' gray surface, while
  // the window title and outer workbench retain Kit's background color.
  Theme::update(cx, |theme| {
    theme.colors.tab_bar = theme.colors.muted;
    theme.colors.tab_bar_segmented = theme.colors.muted;
    theme.colors.tab = theme.colors.muted;
    theme.colors.tab_active = theme.colors.muted;
  });
}
