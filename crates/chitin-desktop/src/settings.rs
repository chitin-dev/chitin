//! Desktop user settings loaded before the first window is created.

use std::{
  io::{Error, ErrorKind},
  path::PathBuf,
};

use chitin_ui::theme::ThemePreference;
use gpui::Context;

use crate::app::ChitinApp;

/// File name used for the persisted theme preference.
const THEME_FILE: &str = "theme";

/// Loads the saved theme preference, falling back to following the system.
///
/// Missing, unreadable, or unrecognized values intentionally fall back to
/// [`ThemePreference::System`] rather than failing application startup.
pub fn load_theme_preference() -> ThemePreference {
  theme_path()
    .map(|path| load_theme_preference_at(&path))
    .unwrap_or_default()
}

/// Persists the selected theme preference for the next application launch.
///
/// # Errors
///
/// Returns an I/O error when no configuration directory can be resolved or the
/// settings file cannot be written. Callers should log the error without
/// interrupting the current interaction.
pub fn save_theme_preference(preference: ThemePreference) -> std::io::Result<()> {
  let path = theme_path().ok_or_else(|| Error::new(ErrorKind::NotFound, "no configuration directory"))?;
  save_theme_preference_at(&path, preference)
}

/// Loads a preference from an explicit path, falling back to following the system.
fn load_theme_preference_at(path: &std::path::Path) -> ThemePreference {
  std::fs::read_to_string(path)
    .ok()
    .and_then(|value| value.trim().parse().ok())
    .unwrap_or_default()
}

/// Writes a preference atomically to an explicit path.
fn save_theme_preference_at(path: &std::path::Path, preference: ThemePreference) -> std::io::Result<()> {
  if let Some(parent) = path.parent() {
    std::fs::create_dir_all(parent)?;
  }
  let temporary = path.with_extension("tmp");
  std::fs::write(&temporary, preference.as_str())?;
  std::fs::rename(temporary, path)
}

impl ChitinApp {
  /// Applies and persists a theme selected from the Settings view.
  pub(crate) fn set_theme_preference(&mut self, preference: ThemePreference, cx: &mut Context<Self>) {
    chitin_ui::theme::set_preference(preference, cx);
    if let Err(error) = save_theme_preference(preference) {
      log::warn!("failed to persist theme preference: {error}");
    }
    cx.notify();
  }
}

/// Resolves the per-user configuration file that stores the theme preference.
fn theme_path() -> Option<PathBuf> {
  config_directory().map(|directory| directory.join(THEME_FILE))
}

/// Resolves the per-user configuration directory on supported desktop systems.
fn config_directory() -> Option<PathBuf> {
  if let Some(path) = std::env::var_os("XDG_CONFIG_HOME") {
    return Some(PathBuf::from(path).join("chitin"));
  }
  if cfg!(target_os = "windows")
    && let Some(path) = std::env::var_os("APPDATA")
  {
    return Some(PathBuf::from(path).join("chitin"));
  }
  let home = std::env::var_os("HOME")?;
  if cfg!(target_os = "macos") {
    Some(
      PathBuf::from(home)
        .join("Library")
        .join("Application Support")
        .join("chitin"),
    )
  } else {
    Some(PathBuf::from(home).join(".config").join("chitin"))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn theme_preference_should_round_trip_through_the_settings_file() {
    let unique = std::time::SystemTime::now()
      .duration_since(std::time::UNIX_EPOCH)
      .map_or(0, |duration| duration.as_nanos());
    let directory = std::env::temp_dir().join(format!("chitin-theme-{}-{unique}", std::process::id()));
    let path = directory.join(THEME_FILE);
    assert_eq!(load_theme_preference_at(&path), ThemePreference::System);
    if let Err(error) = save_theme_preference_at(&path, ThemePreference::Dark) {
      panic!("settings directory should be writable: {error}");
    }
    assert_eq!(load_theme_preference_at(&path), ThemePreference::Dark);
    if let Err(error) = std::fs::write(&path, "sepia") {
      panic!("settings file should be writable: {error}");
    }
    assert_eq!(load_theme_preference_at(&path), ThemePreference::System);
    let _ = std::fs::remove_dir_all(&directory);
  }
}
