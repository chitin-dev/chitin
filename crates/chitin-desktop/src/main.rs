#![forbid(unsafe_code)]
//! Chitin desktop binary entry point.

use std::{borrow::Cow, collections::BTreeSet, path::PathBuf};

use chitin_desktop::{app::ChitinApp, commands::default_key_bindings};
use gpui::{
  App, AppContext, Application, AssetSource, Bounds, Result, SharedString, WindowBounds, WindowOptions, px, size,
};
use rust_embed::RustEmbed;

/// Compile-time asset bundle for the desktop application.
#[derive(RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/../../assets"]
struct EmbeddedAssets;

/// GPUI asset source backed by the embedded desktop assets.
struct DesktopAssets;

impl AssetSource for DesktopAssets {
  /// Loads one embedded asset file.
  ///
  /// # Parameters
  ///
  /// * `path` is the asset-relative path requested by GPUI, such as an icon path
  ///   under `assets/icons`.
  ///
  /// # Returns
  ///
  /// `Ok(Some(bytes))` when the asset exists, or `Ok(None)` when it does not.
  fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
    Ok(EmbeddedAssets::get(path).map(|asset| asset.data))
  }

  /// Lists direct child asset names inside an embedded asset directory.
  ///
  /// # Parameters
  ///
  /// * `path` is the asset-relative directory path requested by GPUI.
  ///
  /// # Returns
  ///
  /// `Ok(Vec<SharedString>)` containing unique UTF-8 child names in sorted
  /// order.
  fn list(&self, path: &str) -> Result<Vec<SharedString>> {
    let path = path.trim_matches('/');
    let prefix = (!path.is_empty()).then(|| format!("{path}/"));
    let mut children = BTreeSet::new();

    for asset_path in EmbeddedAssets::iter() {
      let relative_path = match &prefix {
        Some(prefix) => asset_path.strip_prefix(prefix),
        None => Some(asset_path.as_ref()),
      };

      if let Some(name) = relative_path.and_then(|path| path.split('/').next()) {
        children.insert(name.to_owned());
      }
    }

    Ok(children.into_iter().map(SharedString::from).collect())
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
/// * `std::env::args_os`.
///
/// # Returns
///
/// This function returns `()`. On window creation failure it logs the error to
/// stderr and quits the GPUI application.
fn main() {
  env_logger::init();
  let project_path = std::env::args_os().nth(1).map(PathBuf::from);

  Application::new().with_assets(DesktopAssets).run(|cx: &mut App| {
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
        window.focus(&project_sidebar_focus, cx);
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
