#![forbid(unsafe_code)]
//! Interactive GPUI panel that renders directly into a WGPU surface.
//!
//! Run with `cargo run --example chitin-wgpu-interactive`.
//!
//! This example layers PyMOL/ChimeraX-style viewport controls on top of the
//! minimal WGPUI panel. GPUI handles input and panel chrome; raw `wgpu` keeps
//! the scene resources resident and updates only the camera uniform per frame.

#[path = "chitin-wgpu/common.rs"]
mod common;

use std::{sync::Arc, time};

use common::{
  CubeRenderer, DragMode, INITIAL_SURFACE_HEIGHT, INITIAL_SURFACE_WIDTH, ViewerCamera,
  ViewportDrag, WINDOW_HEIGHT, WINDOW_WIDTH, pixels_to_f32,
};
use gpui::{
  App, Application, Bounds, Context, IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent,
  MouseUpEvent, Render, ScrollWheelEvent, WgpuSurfaceHandle, Window, WindowBounds, WindowOptions,
  div, prelude::*, px, rgb, size, wgpu_surface,
};

/// GPUI view state for the interactive WGPU panel.
struct ChitinWgpuInteractivePanel {
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
  /// Orbit/pan/zoom camera state controlled by GPUI mouse input.
  camera: ViewerCamera,
  /// Active viewport drag, if any.
  active_drag: Option<ViewportDrag>,
}

impl ChitinWgpuInteractivePanel {
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
    let mvp =
      (self.camera.view_projection(renderer.aspect()) * glam::Mat4::IDENTITY).to_cols_array_2d();
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

  /// Starts a molecular-viewer drag gesture.
  ///
  /// # Parameters
  ///
  /// `event` is the GPUI mouse-down event over the WGPU panel.
  ///
  /// # Returns
  ///
  /// This function returns `()` after recording the active drag, or resetting
  /// the view when the user double-clicks.
  fn on_mouse_down(
    &mut self,
    event: &MouseDownEvent,
    _window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    if event.click_count >= 2 && event.button == MouseButton::Left {
      self.camera.reset();
      self.active_drag = None;
      cx.notify();
      return;
    }

    let mode = match event.button {
      MouseButton::Left if event.modifiers.shift => DragMode::Pan,
      MouseButton::Left => DragMode::Rotate,
      MouseButton::Middle => DragMode::Pan,
      MouseButton::Right => DragMode::Zoom,
      _ => return,
    };

    self.active_drag = Some(ViewportDrag {
      mode,
      last_position: event.position,
    });
    cx.notify();
  }

  /// Updates the camera from the current drag gesture.
  ///
  /// # Parameters
  ///
  /// `event` is the GPUI mouse-move event over the WGPU panel.
  ///
  /// # Returns
  ///
  /// This function returns `()` after applying any camera delta.
  fn on_mouse_move(
    &mut self,
    event: &MouseMoveEvent,
    _window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    let Some(mut drag) = self.active_drag else {
      return;
    };
    if event.pressed_button.is_none() {
      self.active_drag = None;
      cx.notify();
      return;
    }

    let delta_x = pixels_to_f32(event.position.x - drag.last_position.x);
    let delta_y = -pixels_to_f32(event.position.y - drag.last_position.y);
    match drag.mode {
      DragMode::Rotate => self.camera.rotate_pixels(delta_x, delta_y),
      DragMode::Pan => self.camera.pan_pixels(delta_x, delta_y),
      DragMode::Zoom => self.camera.zoom_pixels(delta_y),
    }
    drag.last_position = event.position;
    self.active_drag = Some(drag);
    cx.notify();
  }

  /// Ends the current molecular-viewer drag gesture.
  ///
  /// # Parameters
  ///
  /// `_event` is the GPUI mouse-up event ending the gesture.
  ///
  /// # Returns
  ///
  /// This function returns `()` after clearing drag state.
  fn on_mouse_up(&mut self, _event: &MouseUpEvent, _window: &mut Window, cx: &mut Context<Self>) {
    self.active_drag = None;
    cx.notify();
  }

  /// Applies scroll-wheel zoom to the molecular-viewer camera.
  ///
  /// # Parameters
  ///
  /// `event` is the GPUI scroll-wheel event over the WGPU panel.
  ///
  /// # Returns
  ///
  /// This function returns `()` after updating the camera distance.
  fn on_scroll_wheel(&mut self, event: &ScrollWheelEvent) {
    let delta = event.delta.pixel_delta(px(20.0));
    self.camera.zoom_pixels(pixels_to_f32(delta.y));
  }
}

impl Render for ChitinWgpuInteractivePanel {
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
        .id("chitin-wgpu-interactive-viewport")
        .relative()
        .flex_1()
        .min_h_0()
        .rounded_sm()
        .overflow_hidden()
        .border_1()
        .border_color(rgb(0x2a3140))
        .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
        .on_mouse_down(MouseButton::Middle, cx.listener(Self::on_mouse_down))
        .on_mouse_down(MouseButton::Right, cx.listener(Self::on_mouse_down))
        .on_mouse_move(cx.listener(Self::on_mouse_move))
        .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
        .on_mouse_up(MouseButton::Middle, cx.listener(Self::on_mouse_up))
        .on_mouse_up(MouseButton::Right, cx.listener(Self::on_mouse_up))
        .on_scroll_wheel({
          let entity = cx.entity().clone();
          move |event, _, cx| {
            entity.update(cx, |this, cx| {
              this.on_scroll_wheel(event);
              cx.notify();
            });
          }
        })
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
        )
        .child(
          div()
            .absolute()
            .right(px(12.0))
            .bottom(px(10.0))
            .px_2()
            .py_1()
            .rounded_sm()
            .bg(gpui::rgba(0x000000a8))
            .text_xs()
            .text_color(rgb(0xd7e0f2))
            .child("L-drag rotate | Shift-L/M-drag pan | R-drag/wheel zoom | Double-click reset"),
        ),
      None => div()
        .id("chitin-wgpu-interactive-unavailable")
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
                  .child("Chitin WGPU Interactive Panel"),
              )
              .child(
                div()
                  .text_sm()
                  .text_color(rgb(0x93a4ba))
                  .child("Molecular viewer controls"),
              ),
          )
          .child(panel_body),
      )
  }
}

/// Starts the standalone interactive Chitin WGPU example.
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
        app_id: Some("dev.chitin.ChitinWgpuInteractiveExample".to_string()),
        ..WindowOptions::default()
      },
      |window: &mut Window, cx: &mut App| {
        let surface = window.create_wgpu_surface(
          INITIAL_SURFACE_WIDTH,
          INITIAL_SURFACE_HEIGHT,
          wgpu::TextureFormat::Rgba8UnormSrgb,
        );

        cx.new(|_| ChitinWgpuInteractivePanel {
          surface,
          renderer: None,
          frame_count: 0,
          last_fps_update: time::Instant::now(),
          display_fps: 0.0,
          camera: ViewerCamera::default(),
          active_drag: None,
        })
      },
    );

    if let Err(error) = result {
      eprintln!("failed to open Chitin WGPU interactive example window: {error}");
      cx.quit();
    }
  });
}
