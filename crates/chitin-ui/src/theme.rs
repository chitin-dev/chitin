//! Shared GPUI Kit initialization and Chitin theme policy.

use gpui::{App, WindowAppearance};
use gpui_kit::component::{Theme, ThemeMode, theme::ThemeColor};

/// User-selected theme policy for the desktop.
///
/// [`System`](Self::System) follows the operating system and is the default.
/// Explicit choices override the system until the user selects `System` again.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThemePreference {
  /// Follow the window/OS appearance.
  #[default]
  System,
  /// Always use the light theme.
  Light,
  /// Always use the dark theme.
  Dark,
}

impl ThemePreference {
  /// Resolves this preference against the current window appearance.
  pub fn resolve(self, appearance: WindowAppearance) -> ThemeMode {
    match self {
      Self::System => ThemeMode::from(appearance),
      Self::Light => ThemeMode::Light,
      Self::Dark => ThemeMode::Dark,
    }
  }

  /// Returns a stable label for tooltips and accessibility text.
  pub fn label(self) -> &'static str {
    match self {
      Self::System => "System",
      Self::Light => "Light",
      Self::Dark => "Dark",
    }
  }

  /// Returns the stable lowercase name written to settings files.
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::System => "system",
      Self::Light => "light",
      Self::Dark => "dark",
    }
  }
}

impl std::str::FromStr for ThemePreference {
  type Err = ();

  fn from_str(value: &str) -> Result<Self, Self::Err> {
    match value.trim().to_ascii_lowercase().as_str() {
      "system" | "auto" => Ok(Self::System),
      "light" => Ok(Self::Light),
      "dark" => Ok(Self::Dark),
      _ => Err(()),
    }
  }
}

impl gpui::Global for ThemePreference {}

/// Initializes Kit controls and applies the current system theme.
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
  cx.set_global(ThemePreference::default());
  let appearance = cx.window_appearance();
  apply_preference(ThemePreference::System, appearance, cx);
  reapply_workbench_colors(cx);
}

/// Returns the current user theme preference.
pub fn preference(cx: &App) -> ThemePreference {
  cx.try_global::<ThemePreference>().copied().unwrap_or_default()
}

/// Returns the theme mode currently painted, after system resolution.
pub fn effective_mode(cx: &App) -> ThemeMode {
  Theme::global(cx).mode
}

/// Stores a theme preference and applies it immediately.
pub fn set_preference(preference: ThemePreference, cx: &mut App) {
  cx.set_global(preference);
  let appearance = cx.window_appearance();
  apply_preference(preference, appearance, cx);
  reapply_workbench_colors(cx);
}

/// Applies the system theme when the user has not overridden it.
///
/// Window appearance observers call this when the OS switches between light and
/// dark mode. Explicit light or dark preferences are left unchanged.
pub fn apply_system_appearance(appearance: WindowAppearance, cx: &mut App) {
  let preference = preference(cx);
  if preference != ThemePreference::System {
    return;
  }
  apply_preference(preference, appearance, cx);
  reapply_workbench_colors(cx);
}

/// Changes Kit's active mode without duplicating work when it is already active.
fn apply_preference(preference: ThemePreference, appearance: WindowAppearance, cx: &mut App) {
  let mode = preference.resolve(appearance);
  if Theme::global(cx).mode != mode {
    Theme::change(mode, None, cx);
  }
}

/// Rebuilds Chitin's surface overrides for the active Kit colors.
fn reapply_workbench_colors(cx: &mut App) {
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
  fn system_preference_should_resolve_window_appearance() {
    assert_eq!(ThemePreference::System.resolve(WindowAppearance::Dark), ThemeMode::Dark);
    assert_eq!(
      ThemePreference::System.resolve(WindowAppearance::VibrantDark),
      ThemeMode::Dark
    );
    assert_eq!(
      ThemePreference::System.resolve(WindowAppearance::Light),
      ThemeMode::Light
    );
    assert_eq!(
      ThemePreference::System.resolve(WindowAppearance::VibrantLight),
      ThemeMode::Light
    );
  }

  #[test]
  fn explicit_preference_should_ignore_window_appearance() {
    assert_eq!(ThemePreference::Light.resolve(WindowAppearance::Dark), ThemeMode::Light);
    assert_eq!(ThemePreference::Dark.resolve(WindowAppearance::Light), ThemeMode::Dark);
  }

  #[test]
  fn theme_preference_should_round_trip_through_its_storage_name() {
    for preference in [ThemePreference::System, ThemePreference::Light, ThemePreference::Dark] {
      assert_eq!(preference.as_str().parse(), Ok(preference));
    }
    assert_eq!("AUTO".parse(), Ok(ThemePreference::System));
    assert!("sepia".parse::<ThemePreference>().is_err());
  }

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

  #[gpui::test]
  fn system_appearance_should_not_override_explicit_preferences(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
      init(cx);
      assert_eq!(Theme::global(cx).mode, ThemeMode::Light);
      set_preference(ThemePreference::Dark, cx);
      apply_system_appearance(WindowAppearance::Light, cx);
      assert_eq!(Theme::global(cx).mode, ThemeMode::Dark);
      set_preference(ThemePreference::System, cx);
      apply_system_appearance(WindowAppearance::Dark, cx);
      assert_eq!(Theme::global(cx).mode, ThemeMode::Dark);
      apply_system_appearance(WindowAppearance::Light, cx);
      assert_eq!(Theme::global(cx).mode, ThemeMode::Light);
    });
  }
}
