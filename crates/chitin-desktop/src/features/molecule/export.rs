//! Protein viewport image export: destination naming and user feedback.

use std::{
  path::{Path, PathBuf},
  time::{SystemTime, UNIX_EPOCH},
};

use chitin_ui::widgets::toast::{Toast, ToastVariant};
use gpui::Context;

use crate::app::ChitinApp;

impl ChitinApp {
  /// Exports one protein viewport to a timestamped PNG under the workspace.
  ///
  /// The file is written to `<workspace>/.chitin/exports/<title>-<unix-seconds>.png`.
  /// When no workspace is open, the current working directory is used. A toast
  /// reports the final path or the failure reason.
  ///
  /// # Parameters
  ///
  /// * `panel_id` identifies the stable rendering tab to capture.
  /// * `output` optionally supplies a destination file or directory. Relative
  ///   paths resolve inside the workspace; non-PNG extensions become `.png`.
  /// * `cx` updates the toast viewport after the capture completes.
  ///
  /// # Returns
  ///
  /// The written image path on success.
  ///
  /// # Errors
  ///
  /// Returns a user-facing message when the tab is unavailable, the panel does
  /// not support export, or the GPU/PNG write fails.
  pub(crate) fn export_document_image(
    &mut self,
    panel_id: u64,
    output: Option<&Path>,
    cx: &mut Context<Self>,
  ) -> Result<PathBuf, String> {
    let Some(title) = self.document_panels.tab_title(panel_id).map(str::to_owned) else {
      return Err("the requested rendering tab is not open".into());
    };
    let path = self.export_image_path(&title, output);
    match self.document_panels.export_image(panel_id, &path, cx) {
      Some(Ok(())) => {
        self.show_toast(
          Toast::new("Protein image exported")
            .description(path.display().to_string())
            .variant(ToastVariant::Success),
          cx,
        );
        Ok(path)
      }
      Some(Err(error)) => {
        self.show_toast(
          Toast::new("Failed to export protein image")
            .description(error.clone())
            .variant(ToastVariant::Error),
          cx,
        );
        Err(error)
      }
      None => {
        let error = "This panel does not support image export".to_string();
        self.show_toast(Toast::new(error.clone()).variant(ToastVariant::Warning), cx);
        Err(error)
      }
    }
  }

  /// Resolves the export destination for a viewport title.
  fn export_image_path(&self, title: &str, output: Option<&Path>) -> PathBuf {
    match output {
      Some(output) => {
        let mut path = if output.is_absolute() {
          output.to_path_buf()
        } else {
          self.workspace_root().join(output)
        };
        if path.is_dir() || output.to_string_lossy().ends_with(['/', '\\']) {
          path.push(self.default_export_name(title));
        } else if !path
          .extension()
          .and_then(|extension| extension.to_str())
          .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
        {
          path.set_extension("png");
        }
        path
      }
      None => self.export_directory().join(self.default_export_name(title)),
    }
  }

  /// Returns the default directory and filename shown by the save dialog.
  pub(crate) fn export_dialog_default(&self, panel_id: u64) -> Option<(PathBuf, String)> {
    let title = self.document_panels.tab_title(panel_id)?;
    Some((self.export_directory(), self.default_export_name(title)))
  }

  /// Returns the default workspace export directory.
  fn export_directory(&self) -> PathBuf {
    self.workspace_root().join(".chitin").join("exports")
  }

  /// Returns a timestamped PNG filename derived from a tab title.
  fn default_export_name(&self, title: &str) -> String {
    let timestamp = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .map_or(0, |duration| duration.as_secs());
    format!("{}-{timestamp}.png", export_stem(title))
  }

  /// Returns the workspace root, or the process directory when none is open.
  fn workspace_root(&self) -> PathBuf {
    self.workspace.as_ref().map_or_else(
      || std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
      |workspace| workspace.root.clone(),
    )
  }
}

/// Converts a tab title into a filesystem-safe filename stem.
fn export_stem(title: &str) -> String {
  let mut stem = String::new();
  let mut separator = false;
  for character in title.chars() {
    if character.is_ascii_alphanumeric() || character == '_' {
      stem.push(character);
      separator = false;
    } else if !separator {
      stem.push('-');
      separator = true;
    }
  }
  let stem = stem.trim_matches('-');
  if stem.is_empty() || stem == "png" {
    "protein".to_string()
  } else {
    stem
      .strip_suffix(".png")
      .unwrap_or(stem)
      .trim_end_matches('-')
      .to_string()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn export_stem_should_sanitize_titles_and_keep_them_readable() {
    assert_eq!(export_stem("1ABC.pdb"), "1ABC-pdb");
    assert_eq!(export_stem("my protein / chain A"), "my-protein-chain-A");
    assert_eq!(export_stem("..."), "protein");
    assert_eq!(export_stem(".png"), "protein");
  }

  #[test]
  fn export_paths_should_resolve_relative_destinations_and_force_png() {
    let app = ChitinApp::new(None);
    let absolute = std::env::temp_dir().join("chitin-export.png");
    assert_eq!(app.export_image_path("1ABC.pdb", Some(&absolute)), absolute);
    let relative = app.export_image_path("1ABC.pdb", Some(Path::new("images/view.jpg")));
    assert!(relative.is_absolute());
    assert!(relative.ends_with("images/view.png"));
    let directory = app.export_image_path("1ABC.pdb", Some(Path::new("images/")));
    assert_eq!(
      directory.parent().and_then(|parent| parent.file_name()),
      Some(std::ffi::OsStr::new("images"))
    );
    assert_eq!(
      directory.extension().and_then(|extension| extension.to_str()),
      Some("png")
    );
  }
}
