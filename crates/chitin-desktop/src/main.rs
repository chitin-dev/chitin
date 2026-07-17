#![forbid(unsafe_code)]
//! Chitin desktop binary entry point.

mod app;
mod components;

use std::{borrow::Cow, fs, path::PathBuf};

use app::ChitinApp;
use gpui::{
  App, AppContext, Application, AssetSource, Bounds, Result, SharedString, WindowBounds,
  WindowOptions, px, size,
};

/// GPUI asset source backed by the repository's `assets/` directory.
struct DesktopAssets {
  base: PathBuf,
}

impl AssetSource for DesktopAssets {
  fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
    fs::read(self.base.join(path))
      .map(|data| Some(Cow::Owned(data)))
      .map_err(Into::into)
  }

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

fn main() {
  let project_path = std::env::args_os().nth(1).map(PathBuf::from);

  Application::new()
    .with_assets(DesktopAssets {
      base: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
    })
    .run(|cx: &mut App| {
      let bounds = Bounds::centered(None, size(px(1100.0), px(760.0)), cx);
      let result = cx.open_window(
        WindowOptions {
          window_bounds: Some(WindowBounds::Windowed(bounds)),
          app_id: Some("dev.chitin.Chitin".to_string()),
          ..Default::default()
        },
        |window, cx| {
          window.activate_window();
          cx.new(|_| ChitinApp::new(project_path))
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
