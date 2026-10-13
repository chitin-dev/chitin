//! Shared GPUI Kit initialization and Chitin theme policy.

use gpui::App;
use gpui_kit::component::{Theme, ThemeMode, theme::ThemeColor};

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
  Theme::update(cx, |theme| apply_workbench_colors(&mut theme.colors));
}

/// Applies surface hierarchy without changing the scientific viewport palette.
fn apply_workbench_colors(colors: &mut ThemeColor) {
  let surfaces = crate::workbench::WorkbenchStyle::new(*colors);
  // Shared Kit consumers include Select menus, popovers and the command palette.
  colors.popover = surfaces.raised_background;
  colors.accent = surfaces.raised_hover_background;
  colors.title_bar = colors.muted;
  colors.sidebar = colors.muted;
  colors.tab_bar = colors.muted;
  colors.tab_bar_segmented = colors.muted;
  colors.tab = colors.muted;
  colors.tab_active = colors.muted;
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn raised_popups_should_not_change_document_or_viewport_background() {
    let mut colors = *ThemeColor::dark();
    apply_workbench_colors(&mut colors);
    let surfaces = crate::workbench::WorkbenchStyle::new(colors);
    assert_eq!(colors.popover, surfaces.raised_background);
    assert_eq!(colors.accent, surfaces.raised_hover_background);
    assert_eq!(
      colors.background,
      gpui_kit::component::theme::ThemeColor::dark().background
    );
    assert_eq!(colors.tab_bar_segmented, colors.muted);
  }
}
