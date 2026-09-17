//! Stand-alone orchestration for the analytical MSMS visualizer.

mod geometry;

use std::{
  cell::RefCell,
  ffi::OsString,
  fs,
  path::{Path, PathBuf},
  rc::Rc,
  sync::Arc,
  time::Instant,
};

use chitin_bio::{
  structure::{MmcifParser, PdbParser, StructureScene},
  surface::{
    MolecularSurfaceArtifact, SurfacePartition,
    msms::{
      MsmsParameters, MsmsPatchGeometryDomain, MsmsRequest, MsmsTessellationParameters, ToroidalPatchTopology,
      build_msms_patch_geometry,
    },
  },
};
use chitin_desktop::wgpu_panel::{ChitinWgpuDocumentPanel, WgpuPanelFrame, WgpuPanelScene};
use chitin_molecule_renderer::{
  AtomStyle, BallAndStickStyle, MoleculeRenderInput, MoleculeRenderer, RepresentationLayers, SurfaceFragment,
  SurfaceStyle,
};
use gpui::{
  App, AppContext, Application, AsyncApp, Bounds, ClickEvent, Context, Entity, IntoElement, Render, Window,
  WindowBounds, WindowOptions, div, prelude::*, px, rgb, size,
};
use tokio::sync::oneshot;

use self::geometry::{MsmsDebugMeshes, build_debug_meshes, single_surface};

const STAGE_COUNT: usize = 6;
const USAGE: &str = "usage: msms-debug [STRUCTURE] [--probe-radius ANGSTROMS] [--max-edge-length ANGSTROMS]";
/// Linear RGB color used for accessible rolling-probe trajectories.
const PROBE_ARC_COLOR: [f32; 3] = [0.08, 0.65, 1.0];
/// Linear RGB color used for reduced-surface faces and fixed probe centers.
const PROBE_FACE_COLOR: [f32; 3] = [0.72, 0.28, 1.0];
/// Linear RGB color used for convex atom-contact patches.
const CONTACT_PATCH_COLOR: [f32; 3] = [0.20, 0.82, 0.36];
/// Linear RGB color used for saddle-shaped toroidal patches.
const TOROIDAL_PATCH_COLOR: [f32; 3] = [1.0, 0.48, 0.08];
/// Linear RGB color used for concave reentrant patches.
const REENTRANT_PATCH_COLOR: [f32; 3] = [0.95, 0.16, 0.28];

/// Validated command-line inputs for one MSMS diagnostic run.
#[derive(Clone, Copy, Debug, PartialEq)]
struct MsmsDebugParameters {
  request: MsmsRequest,
  tessellation: MsmsTessellationParameters,
}

/// Input path and scientific parameters selected by the command line.
#[derive(Debug, PartialEq)]
struct MsmsDebugOptions {
  path: PathBuf,
  parameters: MsmsDebugParameters,
}

/// Result transferred from the dedicated construction thread to GPUI.
type MsmsTraceResult = Result<(Arc<StructureScene>, Arc<MsmsDebugTrace>), String>;

/// Renderable checkpoints in analytical MSMS construction order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
  Atoms,
  ProbeArcs,
  ProbeFaces,
  ContactPatches,
  ToroidalPatches,
  ReentrantPatches,
}

impl Stage {
  /// Returns the stable ordinal used by navigation controls.
  const fn index(self) -> usize {
    match self {
      Self::Atoms => 0,
      Self::ProbeArcs => 1,
      Self::ProbeFaces => 2,
      Self::ContactPatches => 3,
      Self::ToroidalPatches => 4,
      Self::ReentrantPatches => 5,
    }
  }

  /// Returns one stage from a clamped navigation index.
  const fn from_index(index: usize) -> Self {
    match index {
      0 => Self::Atoms,
      1 => Self::ProbeArcs,
      2 => Self::ProbeFaces,
      3 => Self::ContactPatches,
      4 => Self::ToroidalPatches,
      _ => Self::ReentrantPatches,
    }
  }

  /// Returns the stage title displayed below the viewport.
  const fn label(self) -> &'static str {
    match self {
      Self::Atoms => "Input atoms",
      Self::ProbeArcs => "Accessible rolling-probe arcs",
      Self::ProbeFaces => "Reduced-surface probe faces",
      Self::ContactPatches => "Atom-contact patches",
      Self::ToroidalPatches => "Toroidal rolling patches",
      Self::ReentrantPatches => "Resolved reentrant patches · complete MSMS",
    }
  }
}

/// Shared checkpoint selection read by the UI and WGPU scene.
struct DebugState {
  stage: Stage,
  revision: u64,
}

/// Precomputed topology, patch-family meshes, and final MSMS tessellation.
struct MsmsDebugTrace {
  probe_arcs: MolecularSurfaceArtifact,
  probe_faces: MolecularSurfaceArtifact,
  contact_patches: MolecularSurfaceArtifact,
  toroidal_patches: MolecularSurfaceArtifact,
  reentrant_patches: MolecularSurfaceArtifact,
  edge_count: usize,
  face_count: usize,
  contact_count: usize,
  toroidal_count: usize,
  singular_toroidal_count: usize,
  reentrant_count: usize,
}

impl MsmsDebugTrace {
  /// Converts analytical domain data and display meshes into a retained trace.
  fn new(domain: &MsmsPatchGeometryDomain, meshes: MsmsDebugMeshes, parameters: MsmsDebugParameters) -> Self {
    let probe_radius = parameters.request.parameters.probe_radius();
    let max_edge_length = parameters.tessellation.max_edge_length();
    let singular_toroidal_count = domain
      .toroidal_patches
      .iter()
      .filter(|patch| patch.topology == ToroidalPatchTopology::SelfIntersecting)
      .count();
    Self {
      probe_arcs: single_surface(meshes.probe_arcs, probe_radius, max_edge_length),
      probe_faces: single_surface(meshes.probe_faces, probe_radius, max_edge_length),
      contact_patches: single_surface(meshes.contact_patches, probe_radius, max_edge_length),
      toroidal_patches: single_surface(meshes.toroidal_patches, probe_radius, max_edge_length),
      reentrant_patches: single_surface(meshes.reentrant_patches, probe_radius, max_edge_length),
      edge_count: domain.topology.edges.len(),
      face_count: domain.topology.faces.len(),
      contact_count: domain.contact_patches.len(),
      toroidal_count: domain.toroidal_patches.len(),
      singular_toroidal_count,
      reentrant_count: domain.reentrant_patches.len(),
    }
  }

  /// Returns every colored fragment introduced up to one stage.
  fn visible_fragments(&self, stage: Stage) -> Vec<SurfaceFragment<'_>> {
    let mut fragments = Vec::with_capacity(stage.index());
    if stage.index() >= Stage::ProbeArcs.index() {
      fragments.push(SurfaceFragment {
        surface: &self.probe_arcs,
        color: PROBE_ARC_COLOR,
      });
    }
    if stage.index() >= Stage::ProbeFaces.index() {
      fragments.push(SurfaceFragment {
        surface: &self.probe_faces,
        color: PROBE_FACE_COLOR,
      });
    }
    if stage.index() >= Stage::ContactPatches.index() {
      fragments.push(SurfaceFragment {
        surface: &self.contact_patches,
        color: CONTACT_PATCH_COLOR,
      });
    }
    if stage.index() >= Stage::ToroidalPatches.index() {
      fragments.push(SurfaceFragment {
        surface: &self.toroidal_patches,
        color: TOROIDAL_PATCH_COLOR,
      });
    }
    if stage.index() >= Stage::ReentrantPatches.index() {
      fragments.push(SurfaceFragment {
        surface: &self.reentrant_patches,
        color: REENTRANT_PATCH_COLOR,
      });
    }
    fragments
  }

  /// Returns stage-specific topology or patch counts for the control bar.
  fn stage_detail(&self, stage: Stage) -> String {
    match stage {
      Stage::Atoms => "van der Waals input geometry".to_string(),
      Stage::ProbeArcs => format!("{} accessible edges", self.edge_count),
      Stage::ProbeFaces => format!("{} edges · {} fixed probe centers", self.edge_count, self.face_count),
      Stage::ContactPatches => format!("{} convex spherical patches", self.contact_count),
      Stage::ToroidalPatches => format!(
        "{} saddle patches · {} radially singular",
        self.toroidal_count, self.singular_toroidal_count
      ),
      Stage::ReentrantPatches => format!("{} concave spherical patches", self.reentrant_count),
    }
  }
}

/// WGPU adapter that uploads a new diagnostic mesh only after navigation.
struct MsmsDebugScene {
  scene: Arc<StructureScene>,
  trace: Arc<MsmsDebugTrace>,
  state: Rc<RefCell<DebugState>>,
  renderer: Option<MoleculeRenderer>,
  applied_revision: u64,
}

impl WgpuPanelScene for MsmsDebugScene {
  /// Renders atoms together with the currently selected analytical checkpoint.
  fn render_frame(&mut self, frame: WgpuPanelFrame<'_>) -> wgpu::SubmissionIndex {
    let (stage, revision) = {
      let state = self.state.borrow();
      (state.stage, state.revision)
    };
    if revision != self.applied_revision {
      self.renderer = None;
      self.applied_revision = revision;
      log::debug!("MSMS debug stage uploaded: {stage:?}, revision={revision}");
    }
    let fragments = self.trace.visible_fragments(stage);
    let layers = if fragments.is_empty() {
      RepresentationLayers::atom(AtomStyle::Stick)
    } else {
      RepresentationLayers::atom(AtomStyle::Stick).with_surface(SurfaceStyle::Solid)
    };
    let renderer = self.renderer.get_or_insert_with(|| {
      MoleculeRenderer::new_with_layers(
        Arc::new(frame.device.clone()),
        Arc::new(frame.queue.clone()),
        frame.size,
        frame.format,
        MoleculeRenderInput::new(&self.scene).with_surface_fragments(&fragments),
        layers,
        &BallAndStickStyle::default(),
      )
    });
    renderer.resize_if_needed(frame.size);
    renderer.render(
      frame.view,
      frame.camera.view_matrix(),
      frame.camera.projection_matrix(renderer.aspect()),
    )
  }

  /// Returns the interaction hint rendered by the reusable WGPU panel.
  fn interaction_hint(&self) -> &'static str {
    "MSMS debug | L-drag rotate | Shift-L/M-drag pan | R-drag/wheel zoom"
  }
}

/// Interactive viewport with previous/next construction-stage controls.
struct MsmsDebugView {
  panel: Entity<ChitinWgpuDocumentPanel>,
  state: Rc<RefCell<DebugState>>,
  trace: Arc<MsmsDebugTrace>,
}

impl MsmsDebugView {
  /// Moves to the adjacent checkpoint and schedules a renderer refresh.
  fn step(&mut self, delta: i32, cx: &mut Context<Self>) {
    let mut state = self.state.borrow_mut();
    let previous = state.stage;
    let index = (previous.index() as i32 + delta).clamp(0, (STAGE_COUNT - 1) as i32) as usize;
    state.stage = Stage::from_index(index);
    if state.stage == previous {
      return;
    }
    state.revision = state.revision.wrapping_add(1);
    log::debug!("MSMS debug stage changed: {previous:?} -> {:?}", state.stage);
    drop(state);
    self.panel.update(cx, |_, cx| cx.notify());
    cx.notify();
  }
}

impl Render for MsmsDebugView {
  /// Renders the molecular viewport above compact stage controls.
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let stage = self.state.borrow().stage;
    let detail = self.trace.stage_detail(stage);
    div()
      .flex()
      .flex_col()
      .size_full()
      .child(
        div()
          .relative()
          .flex()
          .flex_1()
          .min_h_0()
          .child(self.panel.clone())
          .when(stage != Stage::Atoms, |viewport| viewport.child(fragment_legend(stage))),
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
          .child(stage_button("msms-debug-previous", "◀", cx.entity().clone(), -1))
          .child(format!(
            "{} / {} · {} · {detail}",
            stage.index() + 1,
            STAGE_COUNT,
            stage.label()
          ))
          .child(stage_button("msms-debug-next", "▶", cx.entity().clone(), 1)),
      )
  }
}

/// Root view shown while MSMS construction runs off the UI thread.
#[derive(Default)]
struct MsmsDebugRoot {
  content: Option<Entity<MsmsDebugView>>,
  error: Option<String>,
}

impl MsmsDebugRoot {
  /// Installs a completed trace into the WGPU-backed diagnostic viewport.
  fn install(
    &mut self,
    scene: Arc<StructureScene>,
    trace: Arc<MsmsDebugTrace>,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    let state = Rc::new(RefCell::new(DebugState {
      stage: Stage::Atoms,
      revision: 1,
    }));
    let panel_scene = MsmsDebugScene {
      scene,
      trace: Arc::clone(&trace),
      state: Rc::clone(&state),
      renderer: None,
      applied_revision: 0,
    };
    let surface = window.create_wgpu_surface(960, 540, wgpu::TextureFormat::Rgba8UnormSrgb);
    let panel = cx.new(|_| ChitinWgpuDocumentPanel::new_with_scene(surface, panel_scene));
    self.content = Some(cx.new(|_| MsmsDebugView { panel, state, trace }));
    self.error = None;
    cx.notify();
  }

  /// Displays one construction failure in place of the loading view.
  fn fail(&mut self, error: String, cx: &mut Context<Self>) {
    self.error = Some(error);
    cx.notify();
  }
}

impl Render for MsmsDebugRoot {
  /// Renders the loading, failure, or completed diagnostic state.
  fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
    if let Some(content) = self.content.as_ref() {
      return div().size_full().child(content.clone());
    }
    let (message, color) = match self.error.as_ref() {
      Some(error) => (format!("Failed to generate MSMS trace: {error}"), rgb(0xffb4c4)),
      None => (
        "Computing analytical MSMS topology and patch tessellations…".to_string(),
        rgb(0xd7e0f2),
      ),
    };
    div()
      .flex()
      .size_full()
      .items_center()
      .justify_center()
      .bg(rgb(0x0b0e14))
      .text_color(color)
      .child(message)
  }
}

/// Renders the stable colors of every fragment visible at one stage.
fn fragment_legend(stage: Stage) -> impl IntoElement {
  div()
    .absolute()
    .top_2()
    .right_2()
    .flex()
    .flex_col()
    .gap_1()
    .px_2()
    .py_2()
    .rounded_sm()
    .bg(gpui::rgba(0x000000b8))
    .text_xs()
    .text_color(rgb(0xd7e0f2))
    .when(stage.index() >= Stage::ProbeArcs.index(), |legend| {
      legend.child(fragment_legend_row("Probe arcs", 0x29a8ff))
    })
    .when(stage.index() >= Stage::ProbeFaces.index(), |legend| {
      legend.child(fragment_legend_row("Probe faces", 0xb65cff))
    })
    .when(stage.index() >= Stage::ContactPatches.index(), |legend| {
      legend.child(fragment_legend_row("Contact", 0x48d66a))
    })
    .when(stage.index() >= Stage::ToroidalPatches.index(), |legend| {
      legend.child(fragment_legend_row("Toroidal", 0xff8b24))
    })
    .when(stage.index() >= Stage::ReentrantPatches.index(), |legend| {
      legend.child(fragment_legend_row("Reentrant", 0xf44258))
    })
}

/// Creates one colored label in the fragment legend.
fn fragment_legend_row(label: &'static str, color: u32) -> impl IntoElement {
  div()
    .flex()
    .items_center()
    .gap_2()
    .child(div().size(px(8.0)).rounded_full().bg(rgb(color)))
    .child(label)
}

/// Creates one compact construction-stage navigation button.
fn stage_button(id: &'static str, label: &'static str, entity: Entity<MsmsDebugView>, delta: i32) -> impl IntoElement {
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

/// Parses a local PDB or mmCIF file into the shared renderer-neutral scene.
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

/// Parses the structure path and validated MSMS display parameters.
fn parse_options(arguments: impl IntoIterator<Item = OsString>) -> Result<MsmsDebugOptions, String> {
  let mut arguments = arguments.into_iter();
  let mut path = None;
  let mut probe_radius = MsmsParameters::default().probe_radius();
  let mut max_edge_length = MsmsTessellationParameters::default().max_edge_length();
  let mut has_probe_radius = false;
  let mut has_max_edge_length = false;

  while let Some(argument) = arguments.next() {
    let argument_text = argument.to_string_lossy();
    if argument_text == "--probe-radius" {
      if has_probe_radius {
        return Err("--probe-radius may only be specified once".to_string());
      }
      probe_radius = parse_f64_option("--probe-radius", arguments.next())?;
      has_probe_radius = true;
    } else if argument_text == "--max-edge-length" {
      if has_max_edge_length {
        return Err("--max-edge-length may only be specified once".to_string());
      }
      max_edge_length = parse_f64_option("--max-edge-length", arguments.next())?;
      has_max_edge_length = true;
    } else if argument_text.starts_with('-') {
      return Err(format!("unknown option: {argument_text}"));
    } else if path.is_some() {
      return Err(format!("unexpected extra structure path: {argument_text}"));
    } else {
      path = Some(PathBuf::from(argument));
    }
  }

  let parameters = MsmsParameters::new(probe_radius).map_err(|error| error.to_string())?;
  let tessellation = MsmsTessellationParameters::new(max_edge_length).map_err(|error| error.to_string())?;
  Ok(MsmsDebugOptions {
    path: path.unwrap_or_else(|| PathBuf::from("structure.cif")),
    parameters: MsmsDebugParameters {
      request: MsmsRequest {
        partition: SurfacePartition::Unified,
        parameters,
        ..MsmsRequest::default()
      },
      tessellation,
    },
  })
}

/// Parses one required finite floating-point command-line value.
fn parse_f64_option(name: &str, value: Option<OsString>) -> Result<f64, String> {
  let value = value.ok_or_else(|| format!("{name} requires a numeric value"))?;
  value
    .to_string_lossy()
    .parse::<f64>()
    .map_err(|_| format!("invalid {name} value: {}", value.to_string_lossy()))
}

/// Starts analytical construction and tessellation on a dedicated thread.
fn spawn_trace_worker(
  scene: Arc<StructureScene>,
  parameters: MsmsDebugParameters,
) -> Result<oneshot::Receiver<MsmsTraceResult>, String> {
  let (sender, receiver) = oneshot::channel();
  std::thread::Builder::new()
    .name("msms-debug-trace".to_string())
    .spawn(move || {
      let started_at = Instant::now();
      log::info!(
        "starting MSMS trace: atoms={}, probe_radius={:.3} A, max_edge_length={:.3} A",
        scene.atoms.len(),
        parameters.request.parameters.probe_radius(),
        parameters.tessellation.max_edge_length(),
      );
      let result = build_msms_patch_geometry(&scene, parameters.request)
        .map_err(|error| error.to_string())
        .and_then(|domains| {
          let domain = domains
            .into_iter()
            .next()
            .ok_or_else(|| "structure contains no eligible surface atoms".to_string())?;
          let meshes = build_debug_meshes(&domain, &scene, parameters.tessellation)?;
          Ok((scene, Arc::new(MsmsDebugTrace::new(&domain, meshes, parameters))))
        });
      if let Err(error) = &result {
        eprintln!("MSMS trace failed: {error}");
        log::error!("MSMS trace failed: {error}");
      }
      log::info!("MSMS trace worker finished after {:.2?}", started_at.elapsed());
      let _ = sender.send(result);
    })
    .map_err(|error| format!("failed to start MSMS trace worker: {error}"))?;
  Ok(receiver)
}

/// Starts the stand-alone MSMS diagnostic window.
pub(super) fn run() {
  env_logger::init();
  let options = match parse_options(std::env::args_os().skip(1)) {
    Ok(options) => options,
    Err(error) => {
      eprintln!("failed to start MSMS debug: {error}\n{USAGE}");
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
  let parameters = options.parameters;
  Application::new().run(move |cx: &mut App| {
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
        let root = cx.new(|_| MsmsDebugRoot::default());
        let window_handle = window.window_handle();
        match spawn_trace_worker(Arc::clone(&scene), parameters) {
          Ok(receiver) => {
            let root = root.downgrade();
            cx.spawn(async move |async_cx: &mut AsyncApp| {
              let result = receiver
                .await
                .unwrap_or_else(|error| Err(format!("MSMS trace worker stopped unexpectedly: {error}")));
              let _ = async_cx.update_window(window_handle, |_, window, cx| {
                let _ = root.update(cx, |root, cx| match result {
                  Ok((scene, trace)) => root.install(scene, trace, window, cx),
                  Err(error) => root.fail(error, cx),
                });
              });
            })
            .detach();
          }
          Err(error) => root.update(cx, |root, cx| root.fail(error, cx)),
        }
        root
      },
    );
    if let Err(error) = result {
      eprintln!("failed to open MSMS debug window: {error}");
      cx.quit();
    }
    cx.activate(true);
  });
}

#[cfg(test)]
mod tests {
  use super::*;

  /// Builds an empty artifact suitable for testing fragment selection.
  fn empty_surface() -> MolecularSurfaceArtifact {
    MolecularSurfaceArtifact {
      source: chitin_bio::surface::SurfaceGeometrySource::Msms {
        probe_radius: 1.4,
        max_edge_length: 0.35,
      },
      domains: Vec::new(),
    }
  }

  /// Builds a trace whose geometry identity is irrelevant to stage tests.
  fn empty_trace() -> MsmsDebugTrace {
    MsmsDebugTrace {
      probe_arcs: empty_surface(),
      probe_faces: empty_surface(),
      contact_patches: empty_surface(),
      toroidal_patches: empty_surface(),
      reentrant_patches: empty_surface(),
      edge_count: 0,
      face_count: 0,
      contact_count: 0,
      toroidal_count: 0,
      singular_toroidal_count: 0,
      reentrant_count: 0,
    }
  }

  #[test]
  fn parse_options_should_apply_msms_parameters() {
    let options = parse_options([
      OsString::from("structure.cif"),
      OsString::from("--probe-radius"),
      OsString::from("1.5"),
      OsString::from("--max-edge-length"),
      OsString::from("0.25"),
    ])
    .unwrap_or_else(|error| panic!("valid MSMS debug arguments should parse: {error}"));

    assert_eq!(options.parameters.request.parameters.probe_radius(), 1.5);
    assert_eq!(options.parameters.tessellation.max_edge_length(), 0.25);
  }

  #[test]
  fn stage_indices_should_follow_msms_construction_order() {
    let stages = (0..STAGE_COUNT).map(Stage::from_index).collect::<Vec<_>>();

    assert_eq!(
      stages,
      vec![
        Stage::Atoms,
        Stage::ProbeArcs,
        Stage::ProbeFaces,
        Stage::ContactPatches,
        Stage::ToroidalPatches,
        Stage::ReentrantPatches,
      ]
    );
  }

  #[test]
  fn visible_fragments_should_accumulate_across_stages() {
    let trace = empty_trace();
    let counts = [
      Stage::Atoms,
      Stage::ProbeArcs,
      Stage::ProbeFaces,
      Stage::ContactPatches,
      Stage::ToroidalPatches,
      Stage::ReentrantPatches,
    ]
    .map(|stage| trace.visible_fragments(stage).len());

    assert_eq!(counts, [0, 1, 2, 3, 4, 5]);
  }

  #[test]
  fn complete_stage_should_preserve_the_fragment_palette() {
    let colors = empty_trace()
      .visible_fragments(Stage::ReentrantPatches)
      .into_iter()
      .map(|fragment| fragment.color)
      .collect::<Vec<_>>();

    assert_eq!(
      colors,
      vec![
        PROBE_ARC_COLOR,
        PROBE_FACE_COLOR,
        CONTACT_PATCH_COLOR,
        TOROIDAL_PATCH_COLOR,
        REENTRANT_PATCH_COLOR,
      ]
    );
  }
}
