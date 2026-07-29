//! Experimental WGPU document panel component.

use std::{
  sync::Arc,
  time::{Duration, Instant},
};

use chitin_wgpu::{ClearRenderer, DragMode, RenderTargetSize, ViewerCamera, ViewportDrag};
use gpui::{
  Context, IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Render, ScrollWheelEvent,
  WgpuSurfaceHandle, Window, div, prelude::*, px, rgb, wgpu_surface,
};

/// Default fps info postfix used in wgpu panel
pub(crate) const DEFAULT_FPS_POSTFIX: &'static str = "fps";
/// Default interaction hint showed at the bottom-right corner of wgpu panel
pub(crate) const DEFAULT_INTERACTION_HINT: &'static str =
  "L-drag rotate | Shift-L/M-drag pan | R-drag/wheel zoom | Double-click reset";
/// Default unavailable message show when wgpu backend is not supported
pub(crate) const DEFAULT_UNAVAILABLE_MESSAGE: &'static str = "WGPU surface is not supported by this GPUI backend";

/// Per-frame data passed from the GPUI panel host into a WGPU scene.
pub struct WgpuPanelFrame<'a> {
  /// WGPU device cloned from the GPUI surface owner.
  pub device: &'a wgpu::Device,
  /// WGPU queue cloned from the GPUI surface owner.
  pub queue: &'a wgpu::Queue,
  /// Color format of the GPUI-owned surface.
  pub format: wgpu::TextureFormat,
  /// Current back-buffer color target.
  pub view: &'a wgpu::TextureView,
  /// Current back-buffer size in physical pixels.
  pub size: RenderTargetSize,
  /// Shared camera controlled by GPUI mouse input.
  pub camera: &'a ViewerCamera,
  /// Time elapsed since the panel was created.
  pub elapsed: Duration,
}

/// Minimal drawing contract for content hosted inside `ChitinWgpuDocumentPanel`.
pub trait WgpuPanelScene {
  /// Renders one frame into the provided target.
  ///
  /// # Parameters
  ///
  /// `frame` contains the GPUI surface resources and current camera state.
  ///
  /// # Returns
  ///
  /// The queue submission index used by the panel for synchronized presentation.
  fn render_frame(&mut self, frame: WgpuPanelFrame<'_>) -> wgpu::SubmissionIndex;

  /// Returns the viewport interaction hint displayed by the panel overlay.
  ///
  /// # Parameters
  ///
  /// This function takes no parameters.
  ///
  /// # Returns
  ///
  /// Static UI text describing scene-specific interactions.
  fn interaction_hint(&self) -> &'static str {
    DEFAULT_INTERACTION_HINT
  }
}

/// Default scene used when no specialized renderer is supplied.
struct ClearScene {
  /// Lazy renderer using the surface's device and queue.
  renderer: Option<ClearRenderer>,
}

impl ClearScene {
  /// Creates a scene that clears the WGPU surface each frame.
  ///
  /// # Parameters
  ///
  /// This function takes no parameters.
  ///
  /// # Returns
  ///
  /// A lazy clear renderer scene for backend smoke tests.
  fn new() -> Self {
    Self { renderer: None }
  }
}

impl WgpuPanelScene for ClearScene {
  /// Clears the current frame using the shared WGPU helper.
  ///
  /// # Parameters
  ///
  /// `frame` contains the GPUI surface resources and back-buffer target.
  ///
  /// # Returns
  ///
  /// The queue submission index for synchronized presentation.
  fn render_frame(&mut self, frame: WgpuPanelFrame<'_>) -> wgpu::SubmissionIndex {
    let renderer = self.renderer.get_or_insert_with(|| {
      ClearRenderer::new(
        Arc::new(frame.device.clone()),
        Arc::new(frame.queue.clone()),
        frame.size,
        wgpu::Color {
          r: 0.025,
          g: 0.030,
          b: 0.045,
          a: 1.0,
        },
      )
    });

    renderer.resize_if_needed(frame.size);
    renderer.render(frame.view)
  }
}

/// Experimental interactive WGPU panel suitable for a document-area tab.
pub struct ChitinWgpuDocumentPanel {
  /// GPUI surface handle, absent only if the backend cannot create WGPU surfaces.
  surface: Option<WgpuSurfaceHandle>,
  /// Scene renderer hosted by the GPUI panel.
  scene: Box<dyn WgpuPanelScene>,
  /// Number of frames since the last FPS update.
  frame_count: u32,
  /// Timestamp for coarse FPS reporting.
  last_fps_update: Instant,
  /// Last computed frames-per-second value displayed in the panel.
  display_fps: f64,
  /// Orbit/pan/zoom camera state controlled by GPUI mouse input.
  camera: ViewerCamera,
  /// Active viewport drag, if any.
  active_drag: Option<ViewportDrag>,
  /// Start time shared with hosted scene animations.
  started_at: Instant,
}

impl ChitinWgpuDocumentPanel {
  /// Creates an interactive WGPU document panel.
  ///
  /// # Parameters
  ///
  /// `surface` is the GPUI-owned WGPU surface created by the desktop window.
  ///
  /// # Returns
  ///
  /// A panel ready to render when GPUI schedules its first frame.
  #[allow(dead_code)]
  pub fn new(surface: Option<WgpuSurfaceHandle>) -> Self {
    Self::new_with_scene(surface, ClearScene::new())
  }

  /// Creates an interactive WGPU document panel with a custom scene.
  ///
  /// # Parameters
  ///
  /// `surface` is the GPUI-owned WGPU surface created by the desktop window.
  ///
  /// `scene` renders each frame using the surface resources supplied by this
  /// panel.
  ///
  /// # Returns
  ///
  /// A panel ready to host the supplied scene in the document area.
  pub fn new_with_scene(surface: Option<WgpuSurfaceHandle>, scene: impl WgpuPanelScene + 'static) -> Self {
    Self {
      surface,
      scene: Box::new(scene),
      frame_count: 0,
      last_fps_update: Instant::now(),
      display_fps: 0.0,
      camera: ViewerCamera::default(),
      active_drag: None,
      started_at: Instant::now(),
    }
  }

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
    let size = RenderTargetSize::new(width, height);
    let frame = WgpuPanelFrame {
      device: surface.device(),
      queue: surface.queue(),
      format: surface.format(),
      view: &view,
      size,
      camera: &self.camera,
      elapsed: self.started_at.elapsed(),
    };
    let submission_index = self.scene.render_frame(frame);
    drop(view);
    // GPUI drives the repaint below via request_animation_frame, so use the
    // silent present path and avoid scheduling an extra full-window refresh.
    surface.present_synced_silent(submission_index);

    self.frame_count = self.frame_count.wrapping_add(1);
    let now = Instant::now();
    if now.duration_since(self.last_fps_update) >= Duration::from_secs(1) {
      self.display_fps = f64::from(self.frame_count);
      self.frame_count = 0;
      self.last_fps_update = now;
    }
  }

  /// Starts a viewport drag gesture.
  ///
  /// # Parameters
  ///
  /// `event` is the GPUI mouse-down event over the WGPU panel.
  ///
  /// # Returns
  ///
  /// This function returns `()` after recording the active drag, or resetting
  /// the view when the user double-clicks.
  fn on_mouse_down(&mut self, event: &MouseDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
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
      last_x: pixels_to_f32(event.position.x),
      last_y: pixels_to_f32(event.position.y),
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
  fn on_mouse_move(&mut self, event: &MouseMoveEvent, _window: &mut Window, cx: &mut Context<Self>) {
    let Some(mut drag) = self.active_drag else {
      return;
    };
    if event.pressed_button.is_none() {
      self.active_drag = None;
      cx.notify();
      return;
    }

    let x = pixels_to_f32(event.position.x);
    let y = pixels_to_f32(event.position.y);
    let delta_x = x - drag.last_x;
    let delta_y = -(y - drag.last_y);
    match drag.mode {
      DragMode::Rotate => self.camera.rotate_pixels(delta_x, delta_y),
      DragMode::Pan => self.camera.pan_pixels(delta_x, delta_y),
      DragMode::Zoom => self.camera.zoom_pixels(delta_y),
    }
    drag.last_x = x;
    drag.last_y = y;
    self.active_drag = Some(drag);
    cx.notify();
  }

  /// Ends the current viewport drag gesture.
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

  /// Applies scroll-wheel zoom to the viewport camera.
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

impl Render for ChitinWgpuDocumentPanel {
  /// Renders the WGPU viewport and schedules the next frame.
  ///
  /// # Parameters
  ///
  /// `window` is used to request animation frames while the panel is visible.
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

    match self.surface.clone() {
      Some(surface) => div()
        .id("chitin-wgpu-document-viewport")
        .relative()
        .flex_1()
        .min_h_0()
        .overflow_hidden()
        .bg(rgb(0x0b0e14))
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
            .child(format!("{:.0} {}", self.display_fps, DEFAULT_FPS_POSTFIX)),
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
            .child(self.scene.interaction_hint()),
        ),
      None => div()
        .id("chitin-wgpu-document-unavailable")
        .flex()
        .flex_1()
        .items_center()
        .justify_center()
        .bg(rgb(0x180f14))
        .text_color(rgb(0xffb4c4))
        .child(DEFAULT_UNAVAILABLE_MESSAGE),
    }
  }
}

/// Converts GPUI logical pixels to plain `f32` values.
///
/// # Parameters
///
/// `pixels` is a GPUI pixel value from an input event.
///
/// # Returns
///
/// The underlying scalar value.
fn pixels_to_f32(pixels: Pixels) -> f32 {
  f32::from(pixels)
}
