#![forbid(unsafe_code)]
//! Minimal GPUI panel that renders directly into a WGPU surface.
//!
//! Run with `cargo run --example chitin-wgpu`.
//!
//! This example is intentionally non-interactive. It demonstrates the smallest
//! useful WGPUI integration path: GPUI owns layout and composition, while raw
//! `wgpu` renders one animated cube into a `WgpuSurfaceHandle`.

#[path = "chitin-wgpu/common.rs"]
mod common;

use std::{sync::Arc, time};

use common::{
  CubeRenderer, INITIAL_SURFACE_HEIGHT, INITIAL_SURFACE_WIDTH, WINDOW_HEIGHT, WINDOW_WIDTH,
  spinning_cube_mvp,
};
use gpui::{
  App, Application, Bounds, Context, IntoElement, Render, WgpuSurfaceHandle, Window, WindowBounds,
  WindowOptions, div, prelude::*, px, rgb, size, wgpu_surface,
};

/// GPUI view state for the minimal WGPU panel.
struct ChitinWgpuPanel {
  /// GPUI surface handle, absent only if the backend cannot create WGPU surfaces.
  surface: Option<WgpuSurfaceHandle>,
  /// Lazy renderer using the surface's device, queue, and color format.
  renderer: Option<CubeRenderer>,
  /// Number of frames since the last FPS update.
  frame_count: u32,
  /// Timestamp for coarse FPS reporting.
  last_fps_update: time::Instant,
  /// Last computed frames-per-second value displayed in the panel.
  display_fps: f64,
  /// Start time used to animate the cube deterministically.
  started_at: time::Instant,
}

impl ChitinWgpuPanel {
  /// Renders one frame into the surface back buffer when available.
  ///
  /// # Parameters
  ///
  /// This method mutably borrows `self` to update renderer and FPS state.
  ///
  /// # Returns
  ///
  /// This function returns `()`. If the surface is unavailable or has no back
  /// buffer yet, it leaves the previous state unchanged.
  fn render_surface(&mut self) {
    let Some(surface) = self.surface.as_ref() else {
      return;
    };
    // Hold this guard while encoding, submitting, and presenting. It prevents
    // resize/reconfigure work from racing the queue on the shared GPUI device.
    let _submit_guard = surface.submit_guard();
    let Some((view, (width, height))) = surface.back_view_with_size() else {
      return;
    };
    let renderer = self.renderer.get_or_insert_with(|| {
      CubeRenderer::new(
        Arc::new(surface.device().clone()),
        Arc::new(surface.queue().clone()),
        width,
        height,
        surface.format(),
      )
    });

    renderer.resize_if_needed(width, height);
    let mvp = spinning_cube_mvp(self.started_at.elapsed().as_secs_f32(), renderer.aspect());
    let submission_index = renderer.render_mvp(&view, mvp);
    drop(view);
    // GPUI drives the repaint below via request_animation_frame, so use the
    // silent present path and avoid scheduling an extra full-window refresh.
    surface.present_synced_silent(submission_index);

    self.frame_count = self.frame_count.wrapping_add(1);
    let now = time::Instant::now();
    if now.duration_since(self.last_fps_update) >= time::Duration::from_secs(1) {
      self.display_fps = f64::from(self.frame_count);
      self.frame_count = 0;
      self.last_fps_update = now;
    }
  }
}

impl Render for ChitinWgpuPanel {
  /// Renders the panel chrome and schedules the next WGPU frame.
  ///
  /// # Parameters
  ///
  /// `window` is used to request animation frames while the example is open.
  ///
  /// `cx` is notified so GPUI keeps repainting the panel.
  ///
  /// # Returns
  ///
  /// A GPUI element tree containing the WGPU surface and a small FPS overlay.
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    self.render_surface();
    window.request_animation_frame();
    cx.notify();

    let panel_body = match self.surface.clone() {
      Some(surface) => div()
        .relative()
        .flex_1()
        .min_h_0()
        .rounded_sm()
        .overflow_hidden()
        .border_1()
        .border_color(rgb(0x2a3140))
        .child(
          // WgpuSurface is just another GPUI element here. GPUI lays it out,
          // then composites the latest ready texture from the handle.
          wgpu_surface(surface)
            .absolute()
            .inset_0()
            .defer_resize_until_mouse_up(true),
        )
        .child(
          div()
            .absolute()
            .top(px(10.0))
            .left(px(12.0))
            .px_2()
            .py_1()
            .rounded_sm()
            .bg(gpui::rgba(0x000000a8))
            .text_xs()
            .text_color(rgb(0xd7e0f2))
            .child(format!("{:.0} fps", self.display_fps)),
        ),
      None => div()
        .flex()
        .flex_1()
        .items_center()
        .justify_center()
        .rounded_sm()
        .border_1()
        .border_color(rgb(0x3a2330))
        .bg(rgb(0x180f14))
        .text_color(rgb(0xffb4c4))
        .child("WGPU surface is not supported by this GPUI backend"),
    };

    div()
      .size_full()
      .bg(rgb(0x0b0e14))
      .text_color(rgb(0xe6edf7))
      .p_4()
      .child(
        div()
          .flex()
          .flex_col()
          .size_full()
          .gap_3()
          .child(
            div()
              .flex()
              .items_center()
              .justify_between()
              .child(
                div()
                  .text_lg()
                  .font_weight(gpui::FontWeight::SEMIBOLD)
                  .child("Chitin WGPU Panel"),
              )
              .child(
                div()
                  .text_sm()
                  .text_color(rgb(0x93a4ba))
                  .child("Minimal GPUI + WGPU surface"),
              ),
          )
          .child(panel_body),
      )
  }
}

/// Starts the standalone minimal Chitin WGPU example.
///
/// # Parameters
///
/// This function takes no Rust parameters.
///
/// # Returns
///
/// This function returns `()` after the GPUI application exits.
fn main() {
  env_logger::init();

  Application::new().run(|cx: &mut App| {
    let bounds = Bounds::centered(None, size(px(WINDOW_WIDTH), px(WINDOW_HEIGHT)), cx);
    let result = cx.open_window(
      WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        app_id: Some("dev.chitin.ChitinWgpuExample".to_string()),
        ..WindowOptions::default()
      },
      |window: &mut Window, cx: &mut App| {
        let surface = window.create_wgpu_surface(
          INITIAL_SURFACE_WIDTH,
          INITIAL_SURFACE_HEIGHT,
          wgpu::TextureFormat::Rgba8UnormSrgb,
        );

        cx.new(|_| ChitinWgpuPanel {
          surface,
          renderer: None,
          frame_count: 0,
          last_fps_update: time::Instant::now(),
          display_fps: 0.0,
          started_at: time::Instant::now(),
        })
      },
    );

    if let Err(error) = result {
      eprintln!("failed to open Chitin WGPU example window: {error}");
      cx.quit();
    }
  });
}
