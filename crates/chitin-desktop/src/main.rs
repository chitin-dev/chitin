#![forbid(unsafe_code)]
//! Chitin desktop binary entry point.

mod app;
mod commands;
mod components;

use std::{borrow::Cow, fs, path::PathBuf};

use app::ChitinApp;
use commands::default_key_bindings;
use gpui::{
  App, AppContext, Application, AssetSource, Bounds, Result, SharedString, WindowBounds,
  WindowOptions, px, size,
};

/// GPUI asset source backed by the repository's `assets/` directory.
struct DesktopAssets {
  base: PathBuf,
}

impl AssetSource for DesktopAssets {
  /// Loads one asset file from the configured desktop asset directory.
  ///
  /// # Parameters
  ///
  /// `path` is the asset-relative path requested by GPUI, such as an icon path
  /// under `assets/icons`.
  ///
  /// # Returns
  ///
  /// `Ok(Some(bytes))` when the asset exists and is readable. Filesystem errors
  /// are converted into GPUI errors.
  fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
    fs::read(self.base.join(path))
      .map(|data| Some(Cow::Owned(data)))
      .map_err(Into::into)
  }

  /// Lists child asset names inside an asset directory.
  ///
  /// # Parameters
  ///
  /// `path` is the asset-relative directory path requested by GPUI.
  ///
  /// # Returns
  ///
  /// `Ok(Vec<SharedString>)` containing UTF-8 child names in filesystem
  /// iteration order. Filesystem errors are converted into GPUI errors.
  fn list(&self, path: &str) -> Result<Vec<SharedString>> {
    fs::read_dir(self.base.join(path))
      .map(|entries| {
        entries
          .filter_map(|entry| {
            entry
              .ok()
              .and_then(|entry| entry.file_name().into_string().ok())
              .map(SharedString::from)
          })
          .collect()
      })
      .map_err(Into::into)
  }
}

/// Starts the Chitin desktop application.
///
/// The binary reads an optional project path from the first CLI argument,
/// registers default command keybindings, opens the GPUI window, and assigns
/// initial focus to the project sidebar so tree navigation works immediately.
///
/// # Parameters
///
/// This function takes no Rust parameters. It reads process arguments through
/// `std::env::args_os`.
///
/// # Returns
///
/// This function returns `()`. On window creation failure it logs the error to
/// stderr and quits the GPUI application.
fn main() {
  env_logger::init();
  let project_path = std::env::args_os().nth(1).map(PathBuf::from);

  Application::new()
    .with_assets(DesktopAssets {
      base: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
    })
    .run(|cx: &mut App| {
      cx.bind_keys(default_key_bindings());

      let bounds = Bounds::centered(None, size(px(1100.0), px(760.0)), cx);
      let result = cx.open_window(
        WindowOptions {
          window_bounds: Some(WindowBounds::Windowed(bounds)),
          app_id: Some("dev.chitin.Chitin".to_string()),
          ..Default::default()
        },
        |window, cx| {
          let project_sidebar_focus = cx.focus_handle();
          window.focus(&project_sidebar_focus);
          window.activate_window();
          cx.new(|_| ChitinApp::new_with_project_sidebar_focus(project_path, project_sidebar_focus))
        },
      );

      if let Err(error) = result {
        eprintln!("failed to open Chitin desktop window: {error}");
        cx.quit();
        return;
      }

      cx.activate(true);
    });
}
