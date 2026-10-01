#![forbid(unsafe_code)]
//! Chitin desktop binary entry point.

use std::path::PathBuf;

use chitin_desktop::{app::ChitinApp, fonts::register_terminal_fonts, keybindings::default_key_bindings};
use chitin_ui::assets::ChitinAssets;
use gpui::{App, AppContext, Application, Bounds, WindowBounds, WindowOptions, px, size};

/// Starts the Chitin desktop application.
fn main() {
  env_logger::init();
  let project_path = std::env::args_os().nth(1).map(PathBuf::from);

  Application::new().with_assets(ChitinAssets).run(|cx: &mut App| {
    // Every layer Chitin draws from, and the theme global each of them reads.
    // It has to run before anything else is constructed.
    chitin_ui::init(cx);
    // The compositor owns outer shadows; keep Kit's controls and resize band
    // without reserving an empty client-side shadow gutter.
    cx.set_global(gpui_kit::component::WindowBorderOptions::default().with_shadow_size(px(0.0)));

    if let Err(error) = register_terminal_fonts(cx) {
      eprintln!("failed to register bundled terminal fonts: {error}");
      cx.quit();
      return;
    }
    cx.bind_keys(default_key_bindings());

    let bounds = Bounds::centered(None, size(px(1100.0), px(760.0)), cx);
    let result = gpui_kit::open_window(
      WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        app_id: Some("dev.chitin.Chitin".to_string()),
        // Kit draws our title bar controls only for client-side decorations.
        window_decorations: Some(gpui::WindowDecorations::Client),
        ..gpui_kit::component::TitleBar::window_options()
      },
      cx,
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
