//! Application composites and specialized GPUI views for Chitin.
//!
//! `chitin-ui` is intended to stay application- and domain-neutral. It provides
//! composable layouts and specialized views over GPUI Kit. Theme colors and
//! general-purpose controls are provided by Kit, not duplicated here.

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
  // Preserve the requested uniform document/title chrome using colors from
  // Kit's palette, without introducing another application-owned theme.
  Theme::update(cx, |theme| {
    theme.colors.tab_bar = theme.colors.background;
    theme.colors.tab = theme.colors.background;
    theme.colors.tab_active = theme.colors.background;
  });
}

/// Bundled application assets and GPUI Kit asset fallback.
pub mod assets;
/// Composite controls assembled from reusable primitives.
pub mod composite;
/// Low-level, application-neutral controls.
pub mod primitive;
