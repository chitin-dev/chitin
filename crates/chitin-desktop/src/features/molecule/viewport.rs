//! Experimental WGPU document panel component.

use std::{
  cell::Cell,
  rc::Rc,
  time::{Duration, Instant},
};

use crate::workbench::documents::state::SelectionInputHandler;
use chitin_bio::structure::selection::AtomSelection;
use chitin_bio::surface::MolecularSurfaceArtifact;
use chitin_command::{SelectionCommand, SelectionMode};
use chitin_molecule_renderer::picking::MolecularPick;
use chitin_molecule_renderer::{DragMode, RepresentationLayers, ViewerCamera, ViewportDrag};
use chitin_ui::workbench::{WorkbenchStyle, viewport_container, viewport_position};
use chitin_wgpu::RenderTargetSize;
use gpui::{
  Bounds, Context, IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Render,
  ScrollWheelEvent, WgpuSurfaceHandle, Window, div, prelude::*, px, wgpu_surface,
};
use gpui_kit::component::{ElementExt, Theme};
use std::sync::Arc;

/// Default fps info postfix used in wgpu panel
pub(crate) const DEFAULT_FPS_POSTFIX: &str = "fps";
/// Default unavailable message show when wgpu backend is not supported
pub(crate) const DEFAULT_UNAVAILABLE_MESSAGE: &str = "WGPU surface is not supported by this GPUI backend";

/// CPU-only selection work prepared from immutable scene geometry.
pub type BoxSelectionTask = Box<dyn FnOnce() -> AtomSelection + Send>;

/// Per-frame data passed from the GPUI panel host into a WGPU scene.
pub struct WgpuPanelFrame<'a> {
  /// Logical-to-physical pixel scale for viewport overlays.
  pub scale_factor: f32,
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
  /// Prepares background selection work without copying the displayed mesh.
  fn box_selection_task(&self, _min: [f32; 2], _max: [f32; 2], _camera: &ViewerCamera) -> Option<BoxSelectionTask> {
    None
  }
  /// Selects visible residues in a viewport-relative box's foreground depth range.
  fn select_in_rect(&self, _min: [f32; 2], _max: [f32; 2], _camera: &ViewerCamera) -> AtomSelection {
    AtomSelection::new()
  }
  /// Applies canonical atom identities without changing representation appearance.
  fn set_selection(&mut self, _atoms: Arc<AtomSelection>) -> bool {
    false
  }

  /// Picks the visible residue using viewport-relative normalized coordinates.
  fn pick(&self, _ndc: [f32; 2], _camera: &ViewerCamera) -> MolecularPick {
    MolecularPick::Miss
  }

  /// Expands a picked residue to coordinate-bearing atoms in this scene.
  fn residue_atoms(&self, _residue: chitin_bio::structure::ResidueId) -> AtomSelection {
    AtomSelection::new()
  }
  /// Renders one frame into the provided target.
  ///
  /// # Parameters
  ///
  /// * `frame` contains the GPUI surface resources and current camera state.
  ///
  /// # Returns
  ///
  /// The queue submission index used by the panel for synchronized presentation.
  fn render_frame(&mut self, frame: WgpuPanelFrame<'_>) -> wgpu::SubmissionIndex;

  /// Applies molecule representation layers when the hosted scene is molecular.
  fn set_representation_layers(&mut self, _representation: RepresentationLayers) -> bool {
    false
  }

  /// Installs a computed molecular surface when the hosted scene supports it.
  fn set_molecular_surface(&mut self, _surface: MolecularSurfaceArtifact) -> bool {
    false
  }

  /// Removes molecular-surface geometry when the hosted scene supports it.
  fn clear_molecular_surface(&mut self) -> bool {
    false
  }
}

/// Experimental interactive WGPU panel suitable for a document-area tab.
pub struct ChitinWgpuDocumentPanel {
  selected: Arc<AtomSelection>,
  selection_input: Option<SelectionInputHandler>,
  selection_gesture: Option<SelectionGesture>,
  selection_commit: Option<SelectionGesture>,
  selection_generation: u64,
  selection_query_running: bool,
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
  /// Actual inset viewport bounds, updated during layout rather than rendering.
  viewport_bounds: Rc<Cell<Bounds<Pixels>>>,
}

struct SelectionGesture {
  origin: gpui::Point<Pixels>,
  mode: SelectionMode,
  moved: bool,
  current: gpui::Point<Pixels>,
  baseline: Arc<AtomSelection>,
  preview_dirty: bool,
}

impl ChitinWgpuDocumentPanel {
  /// Creates an interactive WGPU document panel with a custom scene.
  ///
  /// # Parameters
  ///
  /// * `surface` is the GPUI-owned WGPU surface created by the desktop window.
  /// * `scene` renders each frame using the surface resources supplied by this
  ///   panel.
  ///
  /// # Returns
  ///
  /// A panel ready to host the supplied scene in the document area.
  pub fn new_with_scene(surface: Option<WgpuSurfaceHandle>, scene: impl WgpuPanelScene + 'static) -> Self {
    Self {
      selected: Arc::default(),
      selection_input: None,
      selection_gesture: None,
      selection_commit: None,
      selection_generation: 0,
      selection_query_running: false,
      surface,
      scene: Box::new(scene),
      frame_count: 0,
      last_fps_update: Instant::now(),
      display_fps: 0.0,
      camera: ViewerCamera::default(),
      active_drag: None,
      started_at: Instant::now(),
      viewport_bounds: Rc::new(Cell::new(Bounds::default())),
    }
  }

  pub(crate) fn bind_selection_input(&mut self, handler: SelectionInputHandler) {
    self.selection_input = Some(handler);
  }

  pub(crate) fn set_selection(&mut self, atoms: Arc<AtomSelection>) -> bool {
    self.cancel_selection_query();
    self.selected = Arc::clone(&atoms);
    self.scene.set_selection(atoms)
  }

  /// Changes the hosted molecular scene representation when supported.
  pub fn set_representation_layers(&mut self, representation: RepresentationLayers) -> bool {
    self.cancel_selection_query();
    self.scene.set_representation_layers(representation)
  }

  /// Installs a background-computed molecular surface when supported.
  pub fn set_molecular_surface(&mut self, surface: MolecularSurfaceArtifact) -> bool {
    self.cancel_selection_query();
    self.scene.set_molecular_surface(surface)
  }

  /// Removes the currently installed molecular surface when supported.
  pub fn clear_molecular_surface(&mut self) -> bool {
    self.cancel_selection_query();
    self.scene.clear_molecular_surface()
  }

  /// Renders one frame into the surface back buffer when available.
  fn render_surface(&mut self, scale_factor: f32) {
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
      scale_factor,
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
  /// * `event` is the GPUI mouse-down event over the WGPU panel.
  ///
  /// # Returns
  ///
  /// This function returns `()` after recording the active drag, or resetting
  /// the view when the user double-clicks.
  fn on_mouse_down(&mut self, event: &MouseDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
    let bounds = self.viewport_bounds.get();
    let Some(position) = viewport_position(bounds, WorkbenchStyle::global(cx).viewport_radius, event.position) else {
      return;
    };
    if self.selection_commit.is_some() {
      self.cancel_selection_query();
    }
    if event.button == MouseButton::Left && event.modifiers.control && self.selection_input.is_some() {
      self.cancel_selection_query();
      self.selection_gesture = Some(SelectionGesture {
        origin: event.position,
        mode: if event.modifiers.shift {
          SelectionMode::Toggle
        } else {
          SelectionMode::Replace
        },
        moved: false,
        current: event.position,
        baseline: Arc::clone(&self.selected),
        preview_dirty: false,
      });
      self.active_drag = None;
      cx.notify();
      return;
    }
    if event.click_count >= 2 && event.button == MouseButton::Left && event.modifiers == gpui::Modifiers::default() {
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
      last_x: pixels_to_f32(position.x),
      last_y: pixels_to_f32(position.y),
    });
    cx.notify();
  }

  /// Updates the camera from the current drag gesture.
  ///
  /// # Parameters
  ///
  /// * `event` is the GPUI mouse-move event over the WGPU panel.
  ///
  /// # Returns
  ///
  /// This function returns `()` after applying any camera delta.
  fn on_mouse_move(&mut self, event: &MouseMoveEvent, _window: &mut Window, cx: &mut Context<Self>) {
    if let Some(gesture) = &mut self.selection_gesture {
      let delta = event.position - gesture.origin;
      gesture.moved |= pixels_to_f32(delta.x).hypot(pixels_to_f32(delta.y)) > 4.0;
      let changed = gesture.current != event.position;
      gesture.current = event.position;
      if changed {
        self.selection_generation = self.selection_generation.wrapping_add(1);
      }
      if event.pressed_button.is_none() {
        self.cancel_selection_query();
        self.scene.set_selection(Arc::clone(&self.selected));
      } else if gesture.moved && changed {
        gesture.preview_dirty = true;
      }
      cx.notify();
      return;
    }
    let Some(mut drag) = self.active_drag else {
      return;
    };
    if event.pressed_button.is_none() {
      self.active_drag = None;
      cx.notify();
      return;
    }

    let position = event.position - self.viewport_bounds.get().origin;
    let x = pixels_to_f32(position.x);
    let y = pixels_to_f32(position.y);
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

  fn cancel_selection_query(&mut self) {
    self.selection_generation = self.selection_generation.wrapping_add(1);
    self.selection_gesture = None;
    self.selection_commit = None;
    self.scene.set_selection(Arc::clone(&self.selected));
  }

  fn update_selection_preview(&mut self, cx: &mut Context<Self>) {
    if self.selection_query_running {
      return;
    }
    let Some(gesture) = self
      .selection_commit
      .as_mut()
      .or(self.selection_gesture.as_mut())
      .filter(|gesture| gesture.preview_dirty)
    else {
      return;
    };
    gesture.preview_dirty = false;
    let (origin, current, mode, baseline) = (
      gesture.origin,
      gesture.current,
      gesture.mode,
      Arc::clone(&gesture.baseline),
    );
    let bounds = self.viewport_bounds.get();
    if bounds.size.width <= px(0.) || bounds.size.height <= px(0.) {
      self.apply_selection_result(AtomSelection::new(), mode, baseline, cx);
      return;
    }
    let (min, max) = self.box_coordinates(origin, current);
    let Some(task) = self.scene.box_selection_task(min, max, &self.camera) else {
      let matched = self.scene.select_in_rect(min, max, &self.camera);
      self.apply_selection_result(matched, mode, baseline, cx);
      return;
    };
    self.selection_query_running = true;
    let generation = self.selection_generation;
    let camera = self.camera.view_matrix();
    let worker = cx.background_executor().spawn(async move { task() });
    cx.spawn(async move |panel, cx| {
      let matched = worker.await;
      let _ = panel.update(cx, |panel, cx| {
        panel.selection_query_running = false;
        if generation == panel.selection_generation
          && bounds == panel.viewport_bounds.get()
          && camera == panel.camera.view_matrix()
        {
          panel.apply_selection_result(matched, mode, baseline, cx);
        } else if let Some(gesture) = panel.selection_commit.as_mut().or(panel.selection_gesture.as_mut()) {
          gesture.preview_dirty = true;
        }
        panel.update_selection_preview(cx);
        cx.notify();
      });
    })
    .detach();
  }

  fn apply_selection_result(
    &mut self,
    matched: AtomSelection,
    mode: SelectionMode,
    baseline: Arc<AtomSelection>,
    cx: &mut Context<Self>,
  ) {
    if self.selection_commit.take().is_some() {
      self.scene.set_selection(Arc::clone(&self.selected));
      if let Some(handler) = &self.selection_input {
        handler(SelectionCommand::Atoms { atoms: matched, mode }, cx);
      }
    } else if self.selection_gesture.is_some() {
      self.scene.set_selection(Arc::new(mode.apply(&baseline, matched)));
    }
  }

  /// Ends the current viewport drag gesture.
  ///
  /// # Parameters
  ///
  /// * `_event` is the GPUI mouse-up event ending the gesture.
  ///
  /// # Returns
  ///
  /// This function returns `()` after clearing drag state.
  fn on_mouse_up(&mut self, event: &MouseUpEvent, _window: &mut Window, cx: &mut Context<Self>) {
    if let Some(gesture) = self.selection_gesture.take() {
      let bounds = self.viewport_bounds.get();
      let delta = event.position - gesture.origin;
      let moved = gesture.moved || pixels_to_f32(delta.x).hypot(pixels_to_f32(delta.y)) > 4.0;
      // Keep the last preview visible while the final background query finishes.
      // Preview never enters history; the final bus commit owns the undo entry.
      if !moved || event.button != MouseButton::Left {
        self.scene.set_selection(Arc::clone(&self.selected));
      }
      if event.button == MouseButton::Left && moved {
        let mut gesture = gesture;
        gesture.current = event.position;
        gesture.preview_dirty = true;
        self.selection_generation = self.selection_generation.wrapping_add(1);
        self.selection_commit = Some(gesture);
        self.update_selection_preview(cx);
      } else if event.button == MouseButton::Left
        && !gesture.moved
        && pixels_to_f32(delta.x).hypot(pixels_to_f32(delta.y)) <= 4.0
        && let Some(position) = viewport_position(bounds, WorkbenchStyle::global(cx).viewport_radius, event.position)
        && bounds.size.width > px(0.)
        && bounds.size.height > px(0.)
      {
        let ndc = [
          2. * pixels_to_f32(position.x) / pixels_to_f32(bounds.size.width) - 1.,
          1. - 2. * pixels_to_f32(position.y) / pixels_to_f32(bounds.size.height),
        ];
        let command = match self.scene.pick(ndc, &self.camera) {
          MolecularPick::Residue(id) => Some(SelectionCommand::Atoms {
            atoms: self.scene.residue_atoms(id),
            mode: gesture.mode,
          }),
          MolecularPick::Miss if gesture.mode == SelectionMode::Replace => Some(SelectionCommand::Clear),
          MolecularPick::Miss | MolecularPick::UnmappedSurface => None,
        };
        if let Some(command) = command
          && let Some(handler) = &self.selection_input
        {
          handler(command, cx);
        }
      }
    }
    self.active_drag = None;
    cx.notify();
  }

  fn box_coordinates(&self, origin: gpui::Point<Pixels>, current: gpui::Point<Pixels>) -> ([f32; 2], [f32; 2]) {
    let bounds = self.viewport_bounds.get();
    if bounds.size.width <= px(0.) || bounds.size.height <= px(0.) {
      return ([0.; 2], [0.; 2]);
    }
    let ndc = |point: gpui::Point<Pixels>| {
      [
        (2. * f32::from(point.x - bounds.origin.x) / f32::from(bounds.size.width) - 1.).clamp(-1., 1.),
        (1. - 2. * f32::from(point.y - bounds.origin.y) / f32::from(bounds.size.height)).clamp(-1., 1.),
      ]
    };
    let a = ndc(origin);
    let b = ndc(current);
    ([a[0].min(b[0]), a[1].min(b[1])], [a[0].max(b[0]), a[1].max(b[1])])
  }

  /// Applies scroll-wheel zoom to the viewport camera.
  ///
  /// # Parameters
  ///
  /// * `event` is the GPUI scroll-wheel event over the WGPU panel.
  ///
  /// # Returns
  ///
  /// This function returns `()` after updating the camera distance.
  fn on_scroll_wheel(&mut self, event: &ScrollWheelEvent, cx: &gpui::App) {
    if viewport_position(
      self.viewport_bounds.get(),
      WorkbenchStyle::global(cx).viewport_radius,
      event.position,
    )
    .is_none()
    {
      return;
    }
    let delta = event.delta.pixel_delta(px(20.0));
    if self.selection_gesture.is_some() || self.selection_commit.is_some() {
      self.cancel_selection_query();
    }
    self.camera.zoom_pixels(pixels_to_f32(delta.y));
  }
}

impl Render for ChitinWgpuDocumentPanel {
  /// Renders the WGPU viewport and schedules the next frame.
  ///
  /// # Parameters
  ///
  /// * `window` is used to request animation frames while the panel is visible.
  /// * `cx` is notified so GPUI keeps repainting the panel.
  ///
  /// # Returns
  ///
  /// A GPUI element tree containing the WGPU surface and a small FPS overlay.
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = Theme::global(cx).colors;
    let style = WorkbenchStyle::new(theme);
    let viewport_bounds = self.viewport_bounds.clone();
    if self.selection_gesture.is_some() {
      let panel = cx.weak_entity();
      // Element mouse-move listeners only fire while hovered. Continue a box
      // gesture outside the viewport, clamping its visual/query bounds inside.
      window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
        if phase == gpui::DispatchPhase::Bubble {
          let _ = panel.update(cx, |panel, cx| {
            if panel.selection_gesture.is_some() && !panel.viewport_bounds.get().contains(&event.position) {
              panel.on_mouse_move(event, window, cx);
            }
          });
        }
      });
    }
    if self.surface.is_some() {
      self.render_surface(window.scale_factor());
      self.update_selection_preview(cx);
      window.request_animation_frame();
      cx.notify();
    }

    let viewport = match self.surface.clone() {
      Some(surface) => viewport_container(style)
        .id("chitin-wgpu-document-viewport")
        .on_prepaint(move |bounds, _, _| viewport_bounds.set(bounds))
        .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
        .on_mouse_down(MouseButton::Middle, cx.listener(Self::on_mouse_down))
        .on_mouse_down(MouseButton::Right, cx.listener(Self::on_mouse_down))
        .on_mouse_move(cx.listener(Self::on_mouse_move))
        .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
        .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
        .on_mouse_up(MouseButton::Middle, cx.listener(Self::on_mouse_up))
        .on_mouse_up(MouseButton::Right, cx.listener(Self::on_mouse_up))
        .on_scroll_wheel({
          let entity = cx.entity().clone();
          move |event, _, cx| {
            entity.update(cx, |this, cx| {
              this.on_scroll_wheel(event, cx);
              cx.notify();
            });
          }
        })
        .child(
          wgpu_surface(surface)
            .absolute()
            .inset_0()
            .rounded(style.viewport_radius),
        )
        .child(
          div()
            .absolute()
            .top(px(10.0))
            .left(px(12.0))
            .px_2()
            .py_1()
            .rounded_sm()
            .bg(theme.popover)
            .text_xs()
            .text_color(theme.popover_foreground)
            .child(format!("{:.0} {}", self.display_fps, DEFAULT_FPS_POSTFIX)),
        )
        .when_some(
          self.selection_gesture.as_ref().filter(|gesture| gesture.moved),
          |viewport, gesture| {
            let bounds = self.viewport_bounds.get();
            let a = gesture.origin - bounds.origin;
            let b = gesture.current - bounds.origin;
            let left = a.x.min(b.x).max(px(0.));
            let top = a.y.min(b.y).max(px(0.));
            let right = a.x.max(b.x).min(bounds.size.width);
            let bottom = a.y.max(b.y).min(bounds.size.height);
            viewport.child(
              div()
                .absolute()
                .left(left)
                .top(top)
                .w((right - left).max(px(0.)))
                .h((bottom - top).max(px(0.)))
                .border_1()
                .border_color(gpui::rgb(0x40d980))
                .bg(gpui::rgba(0x40d98018)),
            )
          },
        ),
      None => div()
        .id("chitin-wgpu-document-unavailable")
        .flex()
        .flex_1()
        .items_center()
        .justify_center()
        .bg(theme.background)
        .text_color(theme.danger)
        .child(DEFAULT_UNAVAILABLE_MESSAGE),
    };
    div()
      .flex()
      .flex_col()
      .size_full()
      .min_w_0()
      .min_h_0()
      .p(style.panel_inset)
      .child(viewport)
  }
}

/// Converts GPUI pixels into scalar camera deltas.
fn pixels_to_f32(pixels: Pixels) -> f32 {
  f32::from(pixels)
}

#[cfg(test)]
mod tests {
  use super::*;
  use chitin_bio::structure::{PdbParser, ResidueId, StructureScene};
  use std::cell::RefCell;

  struct PickingProbe(Rc<Cell<[f32; 2]>>, StructureScene);

  struct AsyncBoxProbe(Rc<RefCell<Vec<[f32; 2]>>>, AtomSelection);
  impl WgpuPanelScene for AsyncBoxProbe {
    fn render_frame(&mut self, _: WgpuPanelFrame<'_>) -> wgpu::SubmissionIndex {
      panic!("probe has no GPU surface");
    }
    fn box_selection_task(&self, _: [f32; 2], max: [f32; 2], _: &ViewerCamera) -> Option<BoxSelectionTask> {
      self.0.borrow_mut().push(max);
      let atoms = self.1.clone();
      Some(Box::new(move || atoms))
    }
    fn select_in_rect(&self, _: [f32; 2], _: [f32; 2], _: &ViewerCamera) -> AtomSelection {
      panic!("async scene must never query on the UI thread");
    }
  }

  #[gpui::test]
  fn background_box_queries_should_coalesce_and_commit_only_the_final_rectangle(cx: &mut gpui::TestAppContext) {
    cx.update(chitin_ui::init);
    let queries = Rc::new(RefCell::new(Vec::new()));
    let commands = Rc::new(RefCell::new(Vec::new()));
    let (panel, cx) = cx.add_window_view(|_, _| {
      let mut panel =
        ChitinWgpuDocumentPanel::new_with_scene(None, AsyncBoxProbe(queries.clone(), AtomSelection::new()));
      panel.viewport_bounds.set(Bounds {
        origin: gpui::point(px(80.), px(100.)),
        size: gpui::size(px(200.), px(100.)),
      });
      let commands = commands.clone();
      panel.bind_selection_input(Rc::new(move |command, _| commands.borrow_mut().push(command)));
      panel
    });
    cx.update(|window, cx| {
      panel.update(cx, |panel, cx| {
        let origin = gpui::point(px(130.), px(125.));
        panel.on_mouse_down(
          &MouseDownEvent {
            position: origin,
            modifiers: gpui::Modifiers {
              control: true,
              ..Default::default()
            },
            ..Default::default()
          },
          window,
          cx,
        );
        for offset in [10., 20., 30.] {
          panel.on_mouse_move(
            &MouseMoveEvent {
              position: origin + gpui::point(px(offset), px(10.)),
              pressed_button: Some(MouseButton::Left),
              ..Default::default()
            },
            window,
            cx,
          );
          panel.update_selection_preview(cx);
        }
        assert_eq!(queries.borrow().len(), 1);
        assert!(panel.selection_query_running);
        panel.on_mouse_up(
          &MouseUpEvent {
            position: origin + gpui::point(px(40.), px(10.)),
            ..Default::default()
          },
          window,
          cx,
        );
        assert!(commands.borrow().is_empty());
      })
    });
    cx.run_until_parked();
    assert_eq!(commands.borrow().len(), 1);
    assert_eq!(queries.borrow().len(), 2);
    for (actual, expected) in queries.borrow().iter().zip([[-0.4, 0.5], [-0.1, 0.5]]) {
      assert!(actual.iter().zip(expected).all(|(a, b)| (a - b).abs() < 1.0e-6));
    }
    cx.update(|_, cx| {
      panel.update(cx, |panel, _| {
        assert!(!panel.selection_query_running);
        assert!(panel.selection_commit.is_none());
      })
    });
  }

  #[gpui::test]
  fn external_selection_should_invalidate_pending_box_commit(cx: &mut gpui::TestAppContext) {
    cx.update(chitin_ui::init);
    let commands = Rc::new(RefCell::new(Vec::new()));
    let (panel, cx) = cx.add_window_view(|_, _| {
      let mut panel = ChitinWgpuDocumentPanel::new_with_scene(None, AsyncBoxProbe(Rc::default(), AtomSelection::new()));
      panel.viewport_bounds.set(Bounds {
        origin: gpui::point(px(80.), px(100.)),
        size: gpui::size(px(200.), px(100.)),
      });
      let commands = commands.clone();
      panel.bind_selection_input(Rc::new(move |command, _| commands.borrow_mut().push(command)));
      panel
    });
    cx.update(|window, cx| {
      panel.update(cx, |panel, cx| {
        let origin = gpui::point(px(130.), px(125.));
        panel.on_mouse_down(
          &MouseDownEvent {
            position: origin,
            modifiers: gpui::Modifiers {
              control: true,
              ..Default::default()
            },
            ..Default::default()
          },
          window,
          cx,
        );
        panel.on_mouse_up(
          &MouseUpEvent {
            position: origin + gpui::point(px(20.), px(10.)),
            ..Default::default()
          },
          window,
          cx,
        );
        assert!(panel.selection_query_running);
        panel.set_selection(Arc::default());
      })
    });
    cx.run_until_parked();
    assert!(commands.borrow().is_empty());
  }
  impl WgpuPanelScene for PickingProbe {
    fn render_frame(&mut self, _: WgpuPanelFrame<'_>) -> wgpu::SubmissionIndex {
      panic!("probe has no GPU surface");
    }
    fn pick(&self, ndc: [f32; 2], _: &ViewerCamera) -> MolecularPick {
      self.0.set(ndc);
      self
        .1
        .atoms
        .first()
        .map_or(MolecularPick::Miss, |atom| MolecularPick::Residue(atom.residue_id))
    }
    fn residue_atoms(&self, id: ResidueId) -> AtomSelection {
      self.1.selection.residue_atoms(&self.1, id)
    }
  }

  struct BoxProbe(Rc<RefCell<Arc<AtomSelection>>>, AtomSelection, Rc<Cell<usize>>);
  impl WgpuPanelScene for BoxProbe {
    fn render_frame(&mut self, _: WgpuPanelFrame<'_>) -> wgpu::SubmissionIndex {
      panic!("no GPU in probe");
    }
    fn set_selection(&mut self, atoms: Arc<AtomSelection>) -> bool {
      if *self.0.borrow() == atoms {
        return false;
      }
      *self.0.borrow_mut() = atoms;
      true
    }
    fn select_in_rect(&self, min: [f32; 2], max: [f32; 2], _: &ViewerCamera) -> AtomSelection {
      self.2.set(self.2.get() + 1);
      assert!(min[0] <= max[0] && min[1] <= max[1]);
      self.1.clone()
    }
  }

  #[test]
  fn canonical_selection_should_report_changes_after_invalidating_queries() -> Result<(), Box<dyn std::error::Error>> {
    let parsed = PdbParser::new()
      .parse_bytes(b"ATOM      1  CA  GLY A  42       1.000   2.000   3.000  1.00 10.00           C  \nEND\n")?;
    let scene = StructureScene::from_first_model(&parsed.structure)?;
    let atoms = Arc::new(scene.atoms.iter().map(|atom| atom.atom_id).collect::<AtomSelection>());
    let mut panel =
      ChitinWgpuDocumentPanel::new_with_scene(None, BoxProbe(Rc::default(), AtomSelection::new(), Rc::default()));
    assert!(panel.set_selection(Arc::clone(&atoms)));
    assert!(!panel.set_selection(atoms));
    Ok(())
  }

  #[gpui::test]
  fn box_drag_should_preview_against_baseline_and_commit_only_once(cx: &mut gpui::TestAppContext) {
    cx.update(chitin_ui::init);
    let Ok(parsed) = PdbParser::new()
      .parse_bytes(b"ATOM      1  CA  GLY A  42       1.000   2.000   3.000  1.00 10.00           C  \nEND\n")
    else {
      panic!("fixture");
    };
    let Ok(scene) = StructureScene::from_first_model(&parsed.structure) else {
      panic!("scene");
    };
    let baseline: AtomSelection = scene.atoms.iter().map(|atom| atom.atom_id).collect();
    let displayed = Rc::new(RefCell::new(Arc::new(baseline.clone())));
    let requests = Rc::new(RefCell::new(Vec::new()));
    let queries = Rc::new(Cell::new(0));
    let (panel, cx) = cx.add_window_view(|_, _| {
      let mut panel =
        ChitinWgpuDocumentPanel::new_with_scene(None, BoxProbe(displayed.clone(), baseline.clone(), queries.clone()));
      panel.selected = Arc::new(baseline.clone());
      panel.viewport_bounds.set(Bounds {
        origin: gpui::point(px(80.), px(100.)),
        size: gpui::size(px(200.), px(100.)),
      });
      let requests = requests.clone();
      panel.bind_selection_input(Rc::new(move |command, _| requests.borrow_mut().push(command)));
      panel
    });
    cx.update(|window,cx| panel.update(cx, |panel,cx| {
      let origin = gpui::point(px(130.),px(125.));
      panel.on_mouse_down(&MouseDownEvent { position: origin, modifiers: gpui::Modifiers { control: true, shift: true, ..Default::default() }, ..Default::default() },window,cx);
      for offset in [20.,40.,30.] {
        panel.on_mouse_move(&MouseMoveEvent { position: origin + gpui::point(px(offset),px(20.)), pressed_button: Some(MouseButton::Left), ..Default::default() },window,cx);
        assert_eq!(**displayed.borrow(),baseline);
        assert!(requests.borrow().is_empty());
        assert!(panel.selection_gesture.as_ref().is_some_and(|gesture| gesture.moved));
      }
      assert_eq!(queries.get(),0);
      panel.update_selection_preview(cx);
      assert!(displayed.borrow().is_empty());
      assert_eq!(queries.get(),1);
      panel.update_selection_preview(cx);
      assert_eq!(queries.get(),1);
      panel.on_mouse_up(&MouseUpEvent { position: origin + gpui::point(px(30.),px(20.)), ..Default::default() },window,cx);
      assert_eq!(requests.borrow().len(),1);
      assert_eq!(**displayed.borrow(),baseline);
      assert!(panel.selection_gesture.is_none());
      assert!(matches!(&requests.borrow()[0],SelectionCommand::Atoms { atoms, mode: SelectionMode::Toggle } if *atoms == baseline));
      // A lost button cancels a preview rather than leaving stale GPU flags.
      panel.on_mouse_down(&MouseDownEvent { position: origin, modifiers: gpui::Modifiers { control: true, ..Default::default() }, ..Default::default() },window,cx);
      panel.on_mouse_move(&MouseMoveEvent { position: origin + gpui::point(px(30.),px(20.)), ..Default::default() },window,cx);
      assert!(panel.selection_gesture.is_none());
      assert_eq!(**displayed.borrow(),baseline);
      assert_eq!(requests.borrow().len(),1);
    }));
  }

  #[gpui::test]
  fn selection_clicks_should_use_inset_bounds_and_lock_modifiers_on_press(cx: &mut gpui::TestAppContext) {
    cx.update(chitin_ui::init);
    let ndc = Rc::new(Cell::new([0.; 2]));
    let requests = Rc::new(RefCell::new(Vec::new()));
    let output = requests.clone();
    let Ok(parsed) = PdbParser::new()
      .parse_bytes(b"ATOM      1  CA  GLY A  42       1.000   2.000   3.000  1.00 10.00           C  \nEND\n")
    else {
      panic!("fixture parses");
    };
    let Ok(scene) = StructureScene::from_first_model(&parsed.structure) else {
      panic!("fixture scene");
    };
    let (panel, cx) = cx.add_window_view(|_, _| {
      let mut panel = ChitinWgpuDocumentPanel::new_with_scene(None, PickingProbe(ndc.clone(), scene));
      panel.viewport_bounds.set(Bounds {
        origin: gpui::point(px(80.), px(100.)),
        size: gpui::size(px(200.), px(100.)),
      });
      panel.bind_selection_input(Rc::new(move |command, _| output.borrow_mut().push(command)));
      panel
    });
    cx.update(|window, cx| {
      panel.update(cx, |panel, cx| {
        let original = panel.camera.view_matrix();
        let position = gpui::point(px(130.), px(125.));
        panel.on_mouse_down(
          &MouseDownEvent {
            position,
            modifiers: gpui::Modifiers {
              control: true,
              shift: true,
              ..Default::default()
            },
            click_count: 2,
            ..Default::default()
          },
          window,
          cx,
        );
        panel.on_mouse_up(
          &MouseUpEvent {
            position,
            ..Default::default()
          },
          window,
          cx,
        );
        assert_eq!(panel.camera.view_matrix(), original);
        assert_eq!(ndc.get(), [-0.5, 0.5]);
        assert!(matches!(
          requests.borrow().as_slice(),
          [SelectionCommand::Atoms {
            mode: SelectionMode::Toggle,
            ..
          }]
        ));
        // Crossing the threshold then returning to the start is still a drag.
        panel.on_mouse_down(
          &MouseDownEvent {
            position,
            modifiers: gpui::Modifiers {
              control: true,
              ..Default::default()
            },
            ..Default::default()
          },
          window,
          cx,
        );
        panel.on_mouse_move(
          &MouseMoveEvent {
            position: position + gpui::point(px(10.), px(0.)),
            pressed_button: Some(MouseButton::Left),
            ..Default::default()
          },
          window,
          cx,
        );
        panel.on_mouse_up(
          &MouseUpEvent {
            position,
            ..Default::default()
          },
          window,
          cx,
        );
        assert_eq!(requests.borrow().len(), 2);
        assert_eq!(panel.camera.view_matrix(), original);
        // Ctrl pressed after an ordinary drag began does not change its purpose.
        panel.on_mouse_down(
          &MouseDownEvent {
            position,
            ..Default::default()
          },
          window,
          cx,
        );
        panel.on_mouse_move(
          &MouseMoveEvent {
            position: position + gpui::point(px(10.), px(0.)),
            modifiers: gpui::Modifiers {
              control: true,
              ..Default::default()
            },
            pressed_button: Some(MouseButton::Left),
          },
          window,
          cx,
        );
        panel.on_mouse_up(
          &MouseUpEvent {
            position,
            ..Default::default()
          },
          window,
          cx,
        );
        assert_ne!(panel.camera.view_matrix(), original);
        assert_eq!(requests.borrow().len(), 2);
      })
    });
  }
}
