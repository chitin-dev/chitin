#![forbid(unsafe_code)]
//! Chitin desktop shell with an interactive WGPU document-area panel.
//!
//! Run with `cargo run --example chitin-wgpu-desktop -- .`.
//!
//! This example validates the integration path where GPUI owns the app shell,
//! document tabs, splits, and side panels while WGPU renders an interactive
//! viewport inside one document tab.

#[path = "./chitin-wgpu/cube.rs"]
mod cube;

use std::{borrow::Cow, fs, path::PathBuf};

use chitin_desktop::{
  app::{ChitinApp, WgpuDocumentViewFactory},
  commands::default_key_bindings,
  wgpu_panel::ChitinWgpuDocumentPanel,
};
use cube::ExampleCubeScene;
use gpui::{
  App, AppContext, Application, AssetSource, Bounds, Result, SharedString, WindowBounds, WindowOptions, px, size,
};

/// GPUI asset source backed by the repository's `assets/` directory.
struct DesktopAssets {
  /// Filesystem directory containing desktop assets.
  base: PathBuf,
}

impl AssetSource for DesktopAssets {
  /// Loads one asset file from the configured desktop asset directory.
  ///
  /// # Parameters
  ///
  /// `path` is the asset-relative path requested by GPUI.
  ///
  /// # Returns
  ///
  /// `Ok(Some(bytes))` when the asset exists and is readable.
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
  /// `Ok(Vec<SharedString>)` with UTF-8 child asset names.
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

/// Starts the desktop WGPU integration example.
///
/// # Parameters
///
/// This function reads an optional project path from `std::env::args_os`.
///
/// # Returns
///
/// This function returns `()` after handing control to GPUI's event loop.
fn main() {
  env_logger::init();
  let project_path = std::env::args_os().nth(1).map(PathBuf::from);

  Application::new()
    .with_assets(DesktopAssets {
      base: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
    })
    .run(|cx: &mut App| {
      cx.bind_keys(default_key_bindings());

      let bounds = Bounds::centered(None, size(px(1180.0), px(800.0)), cx);
      let result = cx.open_window(
        WindowOptions {
          window_bounds: Some(WindowBounds::Windowed(bounds)),
          app_id: Some("dev.chitin.ChitinWgpuDesktop".to_string()),
          ..Default::default()
        },
        |window, cx| {
          let project_sidebar_focus = cx.focus_handle();
          window.focus(&project_sidebar_focus, cx);
          window.activate_window();

          let wgpu_panel_factory = WgpuDocumentViewFactory::new(|window, cx| {
            let surface = window.create_wgpu_surface(960, 540, wgpu::TextureFormat::Rgba8UnormSrgb);
            cx.new(|_| ChitinWgpuDocumentPanel::new_with_scene(surface, ExampleCubeScene::new()))
              .into()
          });
          let wgpu_panel = wgpu_panel_factory.build(window, cx);

          cx.new(|_| {
            ChitinApp::new_with_wgpu_document_panel(
              project_path,
              project_sidebar_focus,
              "WGPU example cube",
              wgpu_panel,
              wgpu_panel_factory,
            )
          })
        },
      );

      if let Err(error) = result {
        eprintln!("failed to open Chitin WGPU desktop window: {error}");
        cx.quit();
        return;
      }

      cx.activate(true);
    });
}
