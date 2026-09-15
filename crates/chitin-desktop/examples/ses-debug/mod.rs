//! Stand-alone orchestration for the SES construction visualizer.

mod geometry;
mod scalar_slice;

use std::{
  cell::RefCell,
  collections::HashMap,
  ffi::OsString,
  fs,
  path::{Path, PathBuf},
  rc::Rc,
  sync::Arc,
  time::Duration,
};

use chitin_bio::{
  structure::{MmcifParser, PdbParser, StructureScene},
  surface::{
    MolecularSurfaceArtifact, MolecularSurfaceRequest, ScalarFieldGrid, SesDomainTrace, SesParameters, SurfaceMesh,
    SurfacePartition, trace_molecular_surface,
  },
};
use chitin_desktop::wgpu_panel::{ChitinWgpuDocumentPanel, WgpuPanelFrame, WgpuPanelScene};
use chitin_molecule_renderer::{
  AtomStyle, BallAndStickStyle, MoleculeRenderInput, MoleculeRenderer, RepresentationLayers, SurfaceStyle,
};
use gpui::{
  App, AppContext, Application, AsyncApp, Bounds, ClickEvent, Context, Entity, IntoElement, MouseButton,
  MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Render, Timer, WeakEntity, Window, WindowBounds, WindowOptions,
  div, prelude::*, px, rgb, size,
};

use self::{
  geometry::{grid_mesh, probe_center_stage_mesh, probe_surface_stage_mesh, scene_fit_transform, single_surface},
  scalar_slice::ScalarSliceRenderer,
};

const STAGE_COUNT: usize = 9;
/// Logical-pixel width of the scalar-slice position control.
const SLICE_SLIDER_WIDTH: f32 = 220.0;
/// Delay between automatic scalar-slice updates.
const SLICE_PLAYBACK_FRAME_INTERVAL: Duration = Duration::from_millis(16);
/// Fraction of the Z range traversed by one automatic update.
const SLICE_PLAYBACK_STEP: f32 = 16.0 / 6_000.0;
/// Usage shown when the example receives invalid command-line arguments.
const USAGE: &str = "usage: ses-debug [STRUCTURE] [--max-grid-points POINTS]";

/// Validated inputs used to construct one SES diagnostic run.
#[derive(Debug, PartialEq)]
struct SesDebugOptions {
  path: PathBuf,
  ses: SesParameters,
}

/// Renderable checkpoints in the simplified SES diagnostic pipeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
  Atoms,
  Grid,
  ScalarField,
  ProbeCenters,
  ProbeField,
  RawProbeSurface,
  InnerProbeSurface,
  SmoothedInnerSurface,
  Ses,
}

impl Stage {
  /// Returns the stable ordinal used by the navigation controls.
  fn index(self) -> usize {
    match self {
      Self::Atoms => 0,
      Self::Grid => 1,
      Self::ScalarField => 2,
      Self::ProbeCenters => 3,
      Self::ProbeField => 4,
      Self::RawProbeSurface => 5,
      Self::InnerProbeSurface => 6,
      Self::SmoothedInnerSurface => 7,
      Self::Ses => 8,
    }
  }

  /// Returns the label shown below the viewport.
  fn label(self) -> &'static str {
    match self {
      Self::Atoms => "Input atoms",
      Self::Grid => "Sampling grid",
      Self::ScalarField => "Implicit scalar field",
      Self::ProbeCenters => "Probe centers",
      Self::ProbeField => "Probe distance field",
      Self::RawProbeSurface => "Raw probe isosurface",
      Self::InnerProbeSurface => "Filtered inner surface",
      Self::SmoothedInnerSurface => "Smoothed inner surface",
      Self::Ses => "Final SES",
    }
  }

  /// Returns whether this stage displays a movable scalar-field slice.
  fn has_slice(self) -> bool {
    matches!(
      self,
      Self::ScalarField
        | Self::ProbeCenters
        | Self::ProbeField
        | Self::RawProbeSurface
        | Self::InnerProbeSurface
        | Self::SmoothedInnerSurface
    )
  }
}

/// Shared UI state between the navigation bar and the WGPU scene.
struct DebugState {
  /// Currently selected pipeline checkpoint.
  stage: Stage,
  /// Incremented whenever geometry must be rebuilt for a new checkpoint.
  revision: u64,
  /// Normalized location of the XY slice between the minimum and maximum Z bounds.
  slice_fraction: f32,
  /// Whether the scalar slice is advancing automatically.
  slice_playing: bool,
  /// Current normalized playback direction, either toward maximum or minimum Z.
  slice_playback_direction: f32,
  /// Identity used to invalidate a previously spawned playback loop.
  playback_generation: u64,
}

/// Example-local WGPU scene that turns a selected SES checkpoint into geometry.
struct SesDebugScene {
  /// Renderer-neutral structure loaded from the input file.
  scene: Arc<StructureScene>,
  /// Scientific intermediates captured from the production SES kernel.
  trace: Arc<SesDomainTrace>,
  /// Shared stage selection owned by the parent GPUI view.
  state: Rc<RefCell<DebugState>>,
  /// Lazily initialized molecule renderer using GPUI's WGPU surface.
  renderer: Option<MoleculeRenderer>,
  /// Revision already uploaded to the renderer.
  applied_revision: u64,
  /// Surface artifact containing the selected stage's diagnostic geometry.
  surface: Option<MolecularSurfaceArtifact>,
  /// Unfiltered zero contour containing both probe-field offset sheets.
  raw_probe_surface: MolecularSurfaceArtifact,
  /// Molecular-side contour extracted from the composite inner field.
  inner_probe_surface: MolecularSurfaceArtifact,
  /// Position-smoothed inner contour before field-guided normal correction.
  smoothed_inner_surface: MolecularSurfaceArtifact,
  /// Per-vertex-color slice overlay reused by scalar-field-dependent stages.
  scalar_slice_renderer: Option<ScalarSliceRenderer>,
  /// Slice overlay backed by the probe-sphere distance volume.
  probe_field_renderer: Option<ScalarSliceRenderer>,
}

impl SesDebugScene {
  /// Creates a scene whose CPU geometry is rebuilt only after navigation.
  fn new(scene: Arc<StructureScene>, trace: Arc<SesDomainTrace>, state: Rc<RefCell<DebugState>>) -> Self {
    log::info!(
      "precomputed probe contours: raw_vertices={}, raw_triangles={}, inner_vertices={}, inner_triangles={}, inner_invalid_edges={}, smoothed_vertices={}, smoothed_triangles={}",
      trace.raw_probe_surface.vertices.len(),
      trace.raw_probe_surface.indices.len() / 3,
      trace.inner_probe_surface.vertices.len(),
      trace.inner_probe_surface.indices.len() / 3,
      invalid_edge_count(&trace.inner_probe_surface),
      trace.smoothed_inner_surface.vertices.len(),
      trace.smoothed_inner_surface.indices.len() / 3,
    );
    let raw_probe_stage_mesh = probe_surface_stage_mesh(&trace.probe_field, trace.raw_probe_surface.clone());
    let inner_probe_stage_mesh = probe_surface_stage_mesh(&trace.probe_field, trace.inner_probe_surface.clone());
    let smoothed_inner_stage_mesh = probe_surface_stage_mesh(&trace.probe_field, trace.smoothed_inner_surface.clone());
    Self {
      scene,
      trace,
      state,
      renderer: None,
      applied_revision: 0,
      surface: None,
      raw_probe_surface: single_surface(raw_probe_stage_mesh),
      inner_probe_surface: single_surface(inner_probe_stage_mesh),
      smoothed_inner_surface: single_surface(smoothed_inner_stage_mesh),
      scalar_slice_renderer: None,
      probe_field_renderer: None,
    }
  }

  /// Converts the selected debug stage into a renderer-neutral artifact.
  fn refresh_geometry(&mut self) {
    let state = self.state.borrow();
    if state.revision == self.applied_revision {
      return;
    }
    self.surface = match state.stage {
      Stage::Atoms => None,
      Stage::Grid => Some(single_surface(grid_mesh(&self.trace.sas_field))),
      // The scalar field uses a dedicated per-vertex-color overlay rather than
      // pretending field magnitude is surface displacement. Keep the previous
      // sampling lattice as a surface layer for spatial comparison.
      Stage::ScalarField => Some(single_surface(grid_mesh(&self.trace.sas_field))),
      Stage::ProbeCenters => Some(single_surface(probe_center_stage_mesh(
        &self.trace.sas_field,
        &self.trace.probe_centers,
      ))),
      Stage::ProbeField => Some(single_surface(probe_center_stage_mesh(
        &self.trace.probe_field,
        &self.trace.probe_centers,
      ))),
      Stage::RawProbeSurface | Stage::InnerProbeSurface | Stage::SmoothedInnerSurface => None,
      Stage::Ses => Some(single_surface(self.trace.final_surface.clone())),
    };
    let (vertices, triangles) = self
      .surface_for_stage(state.stage)
      .map(|surface| {
        surface.domains.iter().fold((0, 0), |(vertices, triangles), domain| {
          (
            vertices + domain.mesh.vertices.len(),
            triangles + domain.mesh.indices.len() / 3,
          )
        })
      })
      .unwrap_or((self.scene.atoms.len(), 0));
    log::debug!(
      "SES debug geometry refreshed: stage={:?}, revision={}, vertices={}, triangles={}",
      state.stage,
      state.revision,
      vertices,
      triangles
    );
    self.applied_revision = state.revision;
    self.renderer = None;
  }

  /// Returns the surface artifact displayed by one stage.
  fn surface_for_stage(&self, stage: Stage) -> Option<&MolecularSurfaceArtifact> {
    match stage {
      Stage::RawProbeSurface => Some(&self.raw_probe_surface),
      Stage::InnerProbeSurface => Some(&self.inner_probe_surface),
      Stage::SmoothedInnerSurface => Some(&self.smoothed_inner_surface),
      _ => self.surface.as_ref(),
    }
  }
}

/// Counts open or non-manifold edges in one diagnostic surface mesh.
fn invalid_edge_count(mesh: &SurfaceMesh) -> usize {
  let mut uses = HashMap::new();
  for triangle in mesh.indices.chunks_exact(3) {
    for [first, second] in [
      [triangle[0], triangle[1]],
      [triangle[1], triangle[2]],
      [triangle[2], triangle[0]],
    ] {
      let edge = if first < second {
        (first, second)
      } else {
        (second, first)
      };
      *uses.entry(edge).or_insert(0_u8) += 1;
    }
  }
  uses.values().filter(|count| **count != 2).count()
}

impl WgpuPanelScene for SesDebugScene {
  /// Draws the current stage into the GPUI-owned WGPU target.
  fn render_frame(&mut self, frame: WgpuPanelFrame<'_>) -> wgpu::SubmissionIndex {
    self.refresh_geometry();
    let (stage, slice_fraction) = {
      let state = self.state.borrow();
      (state.stage, state.slice_fraction)
    };
    log::trace!(
      "SES debug render frame: stage={stage:?}, size={}x{}",
      frame.size.width,
      frame.size.height
    );
    let layers = match stage {
      Stage::Atoms => RepresentationLayers::atom(AtomStyle::Stick),
      // Keep the source molecule visible so the lattice has an immediate
      // spatial reference during grid inspection.
      Stage::Grid => RepresentationLayers::atom(AtomStyle::Stick).with_surface(SurfaceStyle::Solid),
      Stage::ScalarField => RepresentationLayers::atom(AtomStyle::Stick).with_surface(SurfaceStyle::Solid),
      // This cumulative stage retains atoms and its combined grid/center mesh;
      // the scalar slice is submitted as the final overlay below.
      Stage::ProbeCenters => RepresentationLayers::atom(AtomStyle::Stick).with_surface(SurfaceStyle::Solid),
      Stage::ProbeField => RepresentationLayers::atom(AtomStyle::Stick).with_surface(SurfaceStyle::Solid),
      Stage::RawProbeSurface | Stage::InnerProbeSurface | Stage::SmoothedInnerSurface => {
        RepresentationLayers::atom(AtomStyle::Stick).with_surface(SurfaceStyle::Solid)
      }
      Stage::Ses => RepresentationLayers::empty().with_surface(SurfaceStyle::Solid),
    };
    let surface = match stage {
      Stage::RawProbeSurface => Some(&self.raw_probe_surface),
      Stage::InnerProbeSurface => Some(&self.inner_probe_surface),
      Stage::SmoothedInnerSurface => Some(&self.smoothed_inner_surface),
      _ => self.surface.as_ref(),
    };
    let renderer = self.renderer.get_or_insert_with(|| {
      MoleculeRenderer::new_with_layers(
        Arc::new(frame.device.clone()),
        Arc::new(frame.queue.clone()),
        frame.size,
        frame.format,
        MoleculeRenderInput {
          scene: &self.scene,
          surface,
        },
        layers,
        &BallAndStickStyle::default(),
      )
    });
    renderer.resize_if_needed(frame.size);
    let molecule_submission = renderer.render(
      frame.view,
      frame.camera.view_matrix(),
      frame.camera.projection_matrix(renderer.aspect()),
    );
    let field = match stage {
      Stage::ScalarField | Stage::ProbeCenters => &self.trace.sas_field,
      Stage::ProbeField | Stage::RawProbeSurface | Stage::InnerProbeSurface | Stage::SmoothedInnerSurface => {
        &self.trace.probe_field
      }
      _ => return molecule_submission,
    };
    let slice_renderer = match stage {
      Stage::ScalarField | Stage::ProbeCenters => &mut self.scalar_slice_renderer,
      Stage::ProbeField | Stage::RawProbeSurface | Stage::InnerProbeSurface | Stage::SmoothedInnerSurface => {
        &mut self.probe_field_renderer
      }
      _ => return molecule_submission,
    };
    let slice = slice_renderer.get_or_insert_with(|| {
      ScalarSliceRenderer::new(
        Arc::new(frame.device.clone()),
        Arc::new(frame.queue.clone()),
        frame.format,
        field,
      )
    });
    let projection = frame.camera.projection_matrix(renderer.aspect());
    slice.render(
      frame.view,
      projection * frame.camera.view_matrix() * scene_fit_transform(&self.scene),
      slice_fraction,
    )
  }

  /// Returns the mouse and keyboard interaction hint for this example.
  fn interaction_hint(&self) -> &'static str {
    "SES debug | L-drag rotate | Shift-L/M-drag pan | R-drag/wheel zoom"
  }
}

/// GPUI view combining the WGPU viewport and stage navigation controls.
struct SesDebugView {
  /// Existing reusable viewport component.
  panel: Entity<ChitinWgpuDocumentPanel>,
  /// Stage state shared with the viewport scene.
  state: Rc<RefCell<DebugState>>,
  /// Last measured bounds of the scalar-slice slider track.
  slider_bounds: Option<Bounds<Pixels>>,
  /// Whether the primary mouse button currently controls the slice position.
  slider_dragging: bool,
  /// Minimum and maximum source-space Z coordinates represented by the slider.
  slice_z_bounds: [f32; 2],
  /// Number of merged centers represented by the probe-center and probe-field stages.
  probe_center_count: usize,
  /// Effective scalar-grid resolution after the SES point-budget adjustment.
  grid_diagnostics: GridDiagnostics,
}

/// Compact description of the scalar lattice used by the traced SES domain.
#[derive(Clone, Copy)]
struct GridDiagnostics {
  spacing: f32,
  dimensions: [usize; 3],
  sample_count: usize,
}

impl GridDiagnostics {
  /// Captures the effective geometry of a sampled scalar field.
  fn from_grid(grid: &ScalarFieldGrid) -> Self {
    Self {
      spacing: grid.spacing,
      dimensions: grid.dimensions,
      sample_count: grid.values.len(),
    }
  }

  /// Formats the diagnostics displayed over the SES viewport.
  fn label(self) -> String {
    let [x, y, z] = self.dimensions;
    format!(
      "Grid {x} × {y} × {z} · Δ {:.3} Å · {} samples",
      self.spacing, self.sample_count
    )
  }
}

impl SesDebugView {
  /// Moves one checkpoint, rebuilds geometry on the next frame, and repaints.
  fn step(&mut self, delta: i32, cx: &mut Context<Self>) {
    let mut state = self.state.borrow_mut();
    let previous_stage = state.stage;
    let index = (state.stage.index() as i32 + delta).clamp(0, (STAGE_COUNT - 1) as i32) as usize;
    state.stage = match index {
      0 => Stage::Atoms,
      1 => Stage::Grid,
      2 => Stage::ScalarField,
      3 => Stage::ProbeCenters,
      4 => Stage::ProbeField,
      5 => Stage::RawProbeSurface,
      6 => Stage::InnerProbeSurface,
      7 => Stage::SmoothedInnerSurface,
      _ => Stage::Ses,
    };
    if state.stage == previous_stage {
      return;
    }
    state.revision = state.revision.wrapping_add(1);
    if !state.stage.has_slice() {
      state.slice_playing = false;
      state.playback_generation = state.playback_generation.wrapping_add(1);
    }
    log::debug!(
      "SES debug stage changed: {:?} -> {:?}, revision={}",
      previous_stage,
      state.stage,
      state.revision
    );
    drop(state);
    self.panel.update(cx, |_, cx| cx.notify());
    cx.notify();
  }

  /// Starts slider tracking and immediately applies the pointer position.
  fn on_slider_mouse_down(&mut self, event: &MouseDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
    self.stop_slice_playback();
    self.slider_dragging = true;
    self.set_slice_from_pointer(event.position.x, cx);
  }

  /// Toggles automatic traversal of the current scalar-field slice.
  fn toggle_slice_playback(&mut self, cx: &mut Context<Self>) {
    let generation = {
      let mut state = self.state.borrow_mut();
      if !state.stage.has_slice() {
        return;
      }
      state.slice_playing = !state.slice_playing;
      state.playback_generation = state.playback_generation.wrapping_add(1);
      state.playback_generation
    };
    cx.notify();
    if !self.state.borrow().slice_playing {
      return;
    }

    cx.spawn(move |this: WeakEntity<SesDebugView>, async_cx: &mut AsyncApp| {
      let mut async_cx = async_cx.clone();
      async move {
        loop {
          Timer::after(SLICE_PLAYBACK_FRAME_INTERVAL).await;
          let should_continue = this
            .update(&mut async_cx, |view, cx| view.advance_slice_playback(generation, cx))
            .unwrap_or(false);
          if !should_continue {
            break;
          }
        }
      }
    })
    .detach();
  }

  /// Advances one playback frame if the initiating playback loop is current.
  fn advance_slice_playback(&mut self, generation: u64, cx: &mut Context<Self>) -> bool {
    let mut state = self.state.borrow_mut();
    if !state.slice_playing || state.playback_generation != generation || !state.stage.has_slice() {
      return false;
    }
    (state.slice_fraction, state.slice_playback_direction) =
      advance_bouncing_slice(state.slice_fraction, state.slice_playback_direction);
    drop(state);
    self.panel.update(cx, |_, cx| cx.notify());
    cx.notify();
    true
  }

  /// Stops playback and invalidates its asynchronous update loop.
  fn stop_slice_playback(&mut self) {
    let mut state = self.state.borrow_mut();
    if !state.slice_playing {
      return;
    }
    state.slice_playing = false;
    state.playback_generation = state.playback_generation.wrapping_add(1);
  }

  /// Updates an active slider gesture anywhere inside the example window.
  fn on_root_mouse_move(&mut self, event: &MouseMoveEvent, _window: &mut Window, cx: &mut Context<Self>) {
    if !self.slider_dragging {
      return;
    }
    // A release outside the application may not produce MouseUp locally. The
    // next unpressed move repairs that stale gesture instead of hijacking it.
    if !event.dragging() || event.pressed_button != Some(MouseButton::Left) {
      self.slider_dragging = false;
      return;
    }
    self.set_slice_from_pointer(event.position.x, cx);
    cx.stop_propagation();
  }

  /// Ends a slider gesture when the primary button is released in the window.
  fn on_root_mouse_up(&mut self, _event: &MouseUpEvent, _window: &mut Window, cx: &mut Context<Self>) {
    if !self.slider_dragging {
      return;
    }
    self.slider_dragging = false;
    cx.stop_propagation();
  }

  /// Converts a window-space pointer X coordinate into a normalized slice position.
  fn set_slice_from_pointer(&mut self, pointer_x: Pixels, cx: &mut Context<Self>) {
    let Some(bounds) = self.slider_bounds else {
      return;
    };
    let width = f32::from(bounds.size.width).max(1.0);
    let fraction = (f32::from(pointer_x - bounds.origin.x) / width).clamp(0.0, 1.0);
    let mut state = self.state.borrow_mut();
    if (state.slice_fraction - fraction).abs() <= f32::EPSILON {
      return;
    }
    state.slice_fraction = fraction;
    drop(state);
    self.panel.update(cx, |_, cx| cx.notify());
    cx.notify();
  }
}

impl Render for SesDebugView {
  /// Renders the viewport above the left/right stage controls.
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let (stage, slice_fraction, slice_playing) = {
      let state = self.state.borrow();
      (state.stage, state.slice_fraction, state.slice_playing)
    };
    let previous = cx.entity().clone();
    let next = cx.entity().clone();
    let grid_description = self.grid_diagnostics.label();
    let stage_description = if matches!(stage, Stage::ProbeCenters | Stage::ProbeField) {
      format!("{} · {} centers", stage.label(), self.probe_center_count)
    } else {
      stage.label().to_string()
    };
    div()
      .flex()
      .flex_col()
      .size_full()
      .on_mouse_move(cx.listener(Self::on_root_mouse_move))
      .on_mouse_up(MouseButton::Left, cx.listener(Self::on_root_mouse_up))
      // The wrapper must itself establish a flex formatting context. Without
      // it, the panel's internal `flex_1` has no parent flex axis and the WGPU
      // viewport can collapse to zero height while the control bar remains.
      .child(
        div()
          .relative()
          .flex()
          .flex_1()
          .min_h_0()
          .child(self.panel.clone())
          .child(
            div()
              .absolute()
              .top_2()
              .right_2()
              .px_2()
              .py_1()
              .rounded_sm()
              .bg(gpui::rgba(0x000000a8))
              .text_xs()
              .text_color(rgb(0xd7e0f2))
              .child(grid_description),
          ),
      )
      .child(
        div()
          .flex()
          .items_center()
          .justify_center()
          .gap_2()
          .px_2()
          .py_1()
          .bg(rgb(0x151b27))
          .text_xs()
          .text_color(rgb(0xd7e0f2))
          .child(button("ses-debug-previous", "◀", previous, -1))
          .child(format!("{} / {} · {stage_description}", stage.index() + 1, STAGE_COUNT))
          .when(stage.has_slice(), |controls| {
            controls
              .child(playback_button(
                if slice_playing { "Ⅱ Pause" } else { "▶ Auto" },
                cx.entity().clone(),
              ))
              .child(slice_slider(
                slice_fraction,
                self.slice_z_bounds,
                cx.entity().clone(),
                cx,
              ))
          })
          .child(button("ses-debug-next", "▶", next, 1)),
      )
  }
}

/// Renders the example-local scalar-slice position slider.
fn slice_slider(
  fraction: f32,
  z_bounds: [f32; 2],
  entity: Entity<SesDebugView>,
  cx: &mut Context<SesDebugView>,
) -> impl IntoElement {
  let measured_entity = entity.clone();
  let knob_left = fraction * (SLICE_SLIDER_WIDTH - 12.0);
  let z = z_bounds[0] + fraction * (z_bounds[1] - z_bounds[0]);
  div().flex().items_center().gap_2().child(format!("Z {z:.2} Å")).child(
    div()
      .flex()
      .items_center()
      .h(px(20.0))
      .cursor_pointer()
      .on_children_prepainted(move |children_bounds, _window, cx| {
        let Some(bounds) = children_bounds.first().copied() else {
          return;
        };
        measured_entity.update(cx, |view, _| view.slider_bounds = Some(bounds));
      })
      .on_mouse_down(MouseButton::Left, cx.listener(SesDebugView::on_slider_mouse_down))
      .child(
        div()
          .id("ses-debug-slice-slider")
          .relative()
          .w(px(SLICE_SLIDER_WIDTH))
          .h(px(6.0))
          .rounded_full()
          .bg(rgb(0x354867))
          .child(
            div()
              .absolute()
              .left(px(knob_left))
              .top(px(-3.0))
              .size(px(12.0))
              .rounded_full()
              .bg(rgb(0xf0d25e)),
          ),
      ),
  )
}

/// Creates one compact navigation button for the stage control bar.
fn button(id: &'static str, label: &'static str, entity: Entity<SesDebugView>, delta: i32) -> impl IntoElement {
  div()
    .id(id)
    .px_2()
    .py_1()
    .rounded_sm()
    .cursor_pointer()
    .bg(rgb(0x26344a))
    .hover(|style| style.bg(rgb(0x354867)))
    .child(label)
    .on_click(move |_event: &ClickEvent, _window, cx| {
      entity.update(cx, |view, cx| view.step(delta, cx));
    })
}

/// Creates the play/pause control for scalar-slice animation.
fn playback_button(label: &'static str, entity: Entity<SesDebugView>) -> impl IntoElement {
  div()
    .id("ses-debug-slice-playback")
    .px_2()
    .py_1()
    .rounded_sm()
    .cursor_pointer()
    .bg(rgb(0x26344a))
    .hover(|style| style.bg(rgb(0x354867)))
    .child(label)
    .on_click(move |_event: &ClickEvent, _window, cx| {
      entity.update(cx, |view, cx| view.toggle_slice_playback(cx));
    })
}

/// Advances a normalized slice position and reflects it at either boundary.
///
/// # Parameters
///
/// * `fraction` is the current normalized position in the closed range `[0, 1]`.
/// * `direction` is positive toward maximum Z and negative toward minimum Z.
///
/// # Returns
///
/// The next position and its possibly reflected direction.
fn advance_bouncing_slice(fraction: f32, direction: f32) -> (f32, f32) {
  let next = fraction + SLICE_PLAYBACK_STEP * direction.signum();
  if next >= 1.0 {
    (2.0 - next, -1.0)
  } else if next <= 0.0 {
    (-next, 1.0)
  } else {
    (next, direction.signum())
  }
}

/// Parses a local PDB or mmCIF file into the shared renderer-neutral scene.
///
/// # Parameters
///
/// * `path` is the input structure path supplied to the example.
///
/// # Returns
///
/// The first-model scene, or a readable parsing and I/O error.
fn load_scene(path: &Path) -> Result<StructureScene, String> {
  let bytes = fs::read(path).map_err(|error| error.to_string())?;
  let extension = path
    .extension()
    .and_then(|value| value.to_str())
    .unwrap_or_default()
    .to_ascii_lowercase();
  let structure = match extension.as_str() {
    "pdb" | "ent" => {
      PdbParser::new()
        .parse_bytes(&bytes)
        .map_err(|error| error.to_string())?
        .structure
    }
    "cif" | "mmcif" => {
      MmcifParser::new()
        .parse_bytes(&bytes)
        .map_err(|error| error.to_string())?
        .structure
    }
    _ => return Err("expected a .pdb, .ent, .cif, or .mmcif file".to_string()),
  };
  StructureScene::from_first_model(&structure).map_err(|error| error.to_string())
}

/// Parses the input path and scalar-grid budget accepted by the example.
///
/// # Parameters
///
/// * `arguments` contains command-line values after the executable name.
///
/// # Returns
///
/// Validated debug options, or a readable error for a missing, duplicate,
/// unknown, or invalid argument.
///
/// # Examples
///
/// Both `structure.cif --max-grid-points 4000000` and
/// `--max-grid-points 4_000_000 structure.cif` select the same budget.
fn parse_options(arguments: impl IntoIterator<Item = OsString>) -> Result<SesDebugOptions, String> {
  let mut arguments = arguments.into_iter();
  let mut path = None;
  let mut ses = SesParameters::default();
  let mut has_grid_budget = false;

  while let Some(argument) = arguments.next() {
    if argument == "--max-grid-points" {
      if has_grid_budget {
        return Err("--max-grid-points may only be specified once".to_string());
      }
      let value = arguments
        .next()
        .ok_or_else(|| "--max-grid-points requires an integer value".to_string())?;
      let value = value.to_string_lossy();
      let max_grid_points = value
        .replace('_', "")
        .parse::<usize>()
        .map_err(|_| format!("invalid --max-grid-points value: {value}"))?;
      ses = ses
        .with_max_grid_points(max_grid_points)
        .map_err(|error| error.to_string())?;
      has_grid_budget = true;
    } else if argument.to_string_lossy().starts_with('-') {
      return Err(format!("unknown option: {}", argument.to_string_lossy()));
    } else if path.is_some() {
      return Err(format!(
        "unexpected extra structure path: {}",
        argument.to_string_lossy()
      ));
    } else {
      path = Some(PathBuf::from(argument));
    }
  }

  Ok(SesDebugOptions {
    path: path.unwrap_or_else(|| PathBuf::from("structure.cif")),
    ses,
  })
}

/// Starts the stand-alone SES diagnostic window.
pub(super) fn run() {
  env_logger::init();
  let options = match parse_options(std::env::args_os().skip(1)) {
    Ok(options) => options,
    Err(error) => {
      eprintln!("failed to start SES debug: {error}\n{USAGE}");
      return;
    }
  };
  let scene = match load_scene(&options.path) {
    Ok(scene) => Arc::new(scene),
    Err(error) => {
      eprintln!("failed to load structure: {error}");
      return;
    }
  };
  // A scalar slice can visualize one regular grid at a time, so this diagnostic
  // explicitly requests a unified domain even though production surfaces are
  // partitioned by chain by default. The trace is computed once; slider movement
  // subsequently changes only one GPU uniform.
  let trace = trace_molecular_surface(
    &scene,
    MolecularSurfaceRequest {
      partition: SurfacePartition::Unified,
      ses: options.ses,
      ..MolecularSurfaceRequest::default()
    },
  );
  let Some(trace) = trace.domains.into_iter().next() else {
    eprintln!("failed to trace SES: structure contains no eligible surface atoms");
    return;
  };
  let trace = Arc::new(trace);
  Application::new().run(move |cx: &mut App| {
    let slice_z_bounds = [trace.sas_field.bounds_min[2], trace.sas_field.bounds_max()[2]];
    let probe_center_count = trace.probe_centers.len();
    let grid_diagnostics = GridDiagnostics::from_grid(&trace.sas_field);
    log::info!(
      "SES scalar grid: budget={}, spacing={:.3} A, dimensions={}x{}x{}, samples={}",
      options.ses.max_grid_points(),
      grid_diagnostics.spacing,
      grid_diagnostics.dimensions[0],
      grid_diagnostics.dimensions[1],
      grid_diagnostics.dimensions[2],
      grid_diagnostics.sample_count,
    );
    let state = Rc::new(RefCell::new(DebugState {
      stage: Stage::Atoms,
      revision: 1,
      slice_fraction: 0.5,
      slice_playing: false,
      slice_playback_direction: 1.0,
      playback_generation: 0,
    }));
    let panel_state = Rc::clone(&state);
    let panel_scene = SesDebugScene::new(Arc::clone(&scene), Arc::clone(&trace), panel_state);
    let result = cx.open_window(
      WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
          None,
          size(px(1180.0), px(800.0)),
          cx,
        ))),
        ..Default::default()
      },
      |window, cx| {
        window.activate_window();
        let surface = window.create_wgpu_surface(960, 540, wgpu::TextureFormat::Rgba8UnormSrgb);
        let panel = cx.new(|_| ChitinWgpuDocumentPanel::new_with_scene(surface, panel_scene));
        cx.new(|_| SesDebugView {
          panel,
          state,
          slider_bounds: None,
          slider_dragging: false,
          slice_z_bounds,
          probe_center_count,
          grid_diagnostics,
        })
      },
    );
    if let Err(error) = result {
      eprintln!("failed to open SES debug window: {error}");
      cx.quit();
    }
    cx.activate(true);
  });
}

#[cfg(test)]
mod tests {
  use super::*;
  use super::{
    geometry::{probe_center_mesh, visible_grid_coordinates},
    scalar_slice::SCALAR_SLICE_SHADER,
  };

  /// Builds a small regular grid for example-local geometry tests.
  fn example_grid() -> ScalarFieldGrid {
    const RESOLUTION: usize = 12;
    ScalarFieldGrid {
      bounds_min: [-2.0; 3],
      spacing: 0.5,
      dimensions: [RESOLUTION; 3],
      values: vec![0.0; RESOLUTION.pow(3)],
    }
  }

  #[test]
  fn grid_diagnostics_should_report_effective_grid_geometry() {
    let diagnostics = GridDiagnostics::from_grid(&example_grid());

    assert_eq!(diagnostics.label(), "Grid 12 × 12 × 12 · Δ 0.500 Å · 1728 samples");
  }

  #[test]
  fn parse_options_should_accept_a_grouped_grid_budget_after_the_path() {
    let options = parse_options([
      OsString::from("structure.cif"),
      OsString::from("--max-grid-points"),
      OsString::from("4_000_000"),
    ])
    .unwrap_or_else(|error| panic!("valid SES debug arguments should parse: {error}"));

    assert_eq!(options.ses.max_grid_points(), 4_000_000);
  }

  #[test]
  fn parse_options_should_accept_the_grid_budget_before_the_path() {
    let options = parse_options([
      OsString::from("--max-grid-points"),
      OsString::from("4000000"),
      OsString::from("structure.cif"),
    ])
    .unwrap_or_else(|error| panic!("valid SES debug arguments should parse: {error}"));

    assert_eq!(options.path, PathBuf::from("structure.cif"));
  }

  #[test]
  fn parse_options_should_reject_a_missing_grid_budget_value() {
    assert_eq!(
      parse_options([OsString::from("--max-grid-points")]),
      Err("--max-grid-points requires an integer value".to_string())
    );
  }

  #[test]
  fn parse_options_should_reject_an_unknown_option() {
    assert_eq!(
      parse_options([OsString::from("--quality")]),
      Err("unknown option: --quality".to_string())
    );
  }

  #[test]
  fn visible_grid_coordinates_should_preserve_both_axis_bounds() {
    let coordinates = visible_grid_coordinates(-2.0, 12.0, 0.5);

    assert_eq!(coordinates.first().copied(), Some(-2.0));
    assert_eq!(coordinates.last().copied(), Some(12.0));
  }

  #[test]
  fn slice_playback_should_only_be_available_for_field_stages() {
    assert!(!Stage::Atoms.has_slice());
    assert!(!Stage::Grid.has_slice());
    assert!(Stage::ScalarField.has_slice());
    assert!(Stage::ProbeCenters.has_slice());
    assert!(Stage::ProbeField.has_slice());
    assert!(Stage::RawProbeSurface.has_slice());
    assert!(Stage::InnerProbeSurface.has_slice());
    assert!(Stage::SmoothedInnerSurface.has_slice());
    assert!(!Stage::Ses.has_slice());
  }

  #[test]
  fn slice_playback_should_reflect_at_both_boundaries() {
    let (near_maximum, reverse) = advance_bouncing_slice(1.0, 1.0);
    let (near_minimum, forward) = advance_bouncing_slice(0.0, -1.0);

    assert!(near_maximum < 1.0);
    assert_eq!(reverse, -1.0);
    assert!(near_minimum > 0.0);
    assert_eq!(forward, 1.0);
  }

  #[test]
  fn grid_mesh_should_contain_only_non_degenerate_triangles() {
    let mesh = grid_mesh(&example_grid());

    assert!(!mesh.indices.is_empty());
    assert!(mesh.indices.chunks_exact(3).all(|triangle| {
      let a = glam::Vec3::from_slice(&mesh.vertices[triangle[0] as usize][0..3]);
      let b = glam::Vec3::from_slice(&mesh.vertices[triangle[1] as usize][0..3]);
      let c = glam::Vec3::from_slice(&mesh.vertices[triangle[2] as usize][0..3]);
      (b - a).cross(c - a).length_squared() > f32::EPSILON
    }));
  }

  #[test]
  fn probe_center_mesh_should_emit_one_closed_octahedron_per_center() {
    let grid = example_grid();
    let centers = [[0.0, 0.0, 0.0], [1.0, 1.0, 1.0]];
    let mesh = probe_center_mesh(&grid, &centers);

    assert_eq!(mesh.indices.len(), centers.len() * 8 * 3);
  }

  #[test]
  fn probe_center_stage_should_retain_grid_and_center_geometry() {
    let grid = example_grid();
    let centers = [[0.0, 0.0, 0.0], [1.0, 1.0, 1.0]];
    let grid_vertex_count = grid_mesh(&grid).vertices.len();
    let center_vertex_count = probe_center_mesh(&grid, &centers).vertices.len();
    let combined = probe_center_stage_mesh(&grid, &centers);

    assert_eq!(combined.vertices.len(), grid_vertex_count + center_vertex_count);
  }

  #[test]
  fn scalar_slice_shader_should_pass_wgsl_validation() {
    let module = wgpu::naga::front::wgsl::parse_str(SCALAR_SLICE_SHADER)
      .unwrap_or_else(|error| panic!("scalar-slice WGSL should parse: {error}"));
    let mut validator = wgpu::naga::valid::Validator::new(
      wgpu::naga::valid::ValidationFlags::all(),
      wgpu::naga::valid::Capabilities::all(),
    );

    assert!(validator.validate(&module).is_ok());
  }
}
