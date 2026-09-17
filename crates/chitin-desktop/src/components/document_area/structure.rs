//! Structure-file loading and molecular WGPU scene integration.

use std::{cell::Cell, path::Path, rc::Rc, sync::Arc};

use chitin_bio::{
  structure::{MmcifParser, PdbParser, StructureScene},
  surface::{
    MolecularSurfaceArtifact, MolecularSurfaceBackend, MolecularSurfaceRequest, generate_implicit_surface,
    msms::{MsmsRequest, MsmsTessellationParameters, generate_msms_surface},
  },
};
use chitin_molecule_renderer::{
  AtomStyle, BallAndStickStyle, MoleculeDebugMode, MoleculeRenderInput, MoleculeRenderer, PolymerStyle,
  RepresentationLayers,
};
use chitin_ui::composite::toast::{Toast, ToastVariant, ToastViewport};
use gpui::{AnyWindowHandle, App, AppContext, AsyncApp, Entity, Window};

use crate::components::{
  document_area::state::WgpuDocumentView,
  wgpu_panel::{ChitinWgpuDocumentPanel, WgpuPanelFrame, WgpuPanelScene},
};

/// Default representation used when GPUI opens a PDB or mmCIF document.
const DEFAULT_STRUCTURE_REPRESENTATION: RepresentationLayers =
  RepresentationLayers::atom(AtomStyle::Stick).with_polymer(PolymerStyle::Cartoon);

/// Mutable lifecycle state shared by one molecular view's surface controls.
struct SurfaceGenerationState {
  /// Backend selected in the document options popover.
  backend: Cell<MolecularSurfaceBackend>,
  /// Whether the representation currently includes a surface layer.
  visible: Cell<bool>,
  /// Backend that produced the cached scene artifact, when one is ready.
  ready_backend: Cell<Option<MolecularSurfaceBackend>>,
  /// Backend currently being calculated, when a worker is active.
  pending_backend: Cell<Option<MolecularSurfaceBackend>>,
  /// Monotonic token used to reject results from superseded workers.
  generation: Cell<u64>,
}

impl SurfaceGenerationState {
  /// Creates surface lifecycle state for a new molecular document view.
  fn new() -> Self {
    Self {
      backend: Cell::new(MolecularSurfaceBackend::default()),
      visible: Cell::new(false),
      ready_backend: Cell::new(None),
      pending_backend: Cell::new(None),
      generation: Cell::new(0),
    }
  }

  /// Invalidates pending and cached geometry after a backend change.
  fn select_backend(&self, backend: MolecularSurfaceBackend) {
    self.backend.set(backend);
    self.ready_backend.set(None);
    self.pending_backend.set(None);
    self.generation.set(self.generation.get().wrapping_add(1));
  }
}

/// Molecular scene backed by one parsed structure and a selectable representation.
pub(crate) struct StructureMoleculeScene {
  /// Renderer-neutral structure data shared by split-panel clones.
  scene: Arc<StructureScene>,
  /// GPU resources created lazily after a surface device is available.
  renderer: Option<MoleculeRenderer>,
  /// Scientific surface geometry computed independently of GPU resources.
  surface: Option<MolecularSurfaceArtifact>,
  /// Shader output selected through the optional debug environment variable.
  debug_mode: MoleculeDebugMode,
  /// Representation layers currently rendered by this scene.
  representation: RepresentationLayers,
}

impl StructureMoleculeScene {
  /// Creates a lazy molecular scene with the selected representation layers.
  pub(crate) fn new(scene: Arc<StructureScene>, representation: RepresentationLayers) -> Self {
    Self {
      scene,
      renderer: None,
      surface: None,
      debug_mode: MoleculeDebugMode::Final,
      representation,
    }
  }
}

impl WgpuPanelScene for StructureMoleculeScene {
  /// Renders one fitted molecular frame into the GPUI-owned surface.
  fn render_frame(&mut self, frame: WgpuPanelFrame<'_>) -> wgpu::SubmissionIndex {
    let renderer = self.renderer.get_or_insert_with(|| {
      MoleculeRenderer::new_with_layers(
        Arc::new(frame.device.clone()),
        Arc::new(frame.queue.clone()),
        frame.size,
        frame.format,
        MoleculeRenderInput {
          scene: &self.scene,
          surface: self.surface.as_ref(),
          surface_fragments: &[],
        },
        self.representation,
        &BallAndStickStyle::default(),
      )
    });
    renderer.resize_if_needed(frame.size);
    renderer.set_debug_mode(self.debug_mode);
    renderer.render(
      frame.view,
      frame.camera.view_matrix(),
      frame.camera.projection_matrix(renderer.aspect()),
    )
  }

  /// Returns the interaction hint displayed over a molecular viewport.
  fn interaction_hint(&self) -> &'static str {
    match (self.representation.atom_style(), self.representation.polymer_style()) {
      (Some(AtomStyle::Stick), None) => "Atom style: Stick | L-drag rotate | Shift-L/M-drag pan | R-drag/wheel zoom",
      (Some(AtomStyle::BallAndStick), None) => {
        "Atom style: Ball and stick | L-drag rotate | Shift-L/M-drag pan | R-drag/wheel zoom"
      }
      (Some(AtomStyle::Sphere), None) => "Atom style: Sphere | L-drag rotate | Shift-L/M-drag pan | R-drag/wheel zoom",
      (Some(AtomStyle::Stick), Some(PolymerStyle::Cartoon)) => {
        "Molecule representations: Stick + Cartoon | L-drag rotate | Shift-L/M-drag pan | R-drag/wheel zoom"
      }
      (_, Some(PolymerStyle::Cartoon)) => {
        "Molecule representation includes Cartoon | L-drag rotate | Shift-L/M-drag pan | R-drag/wheel zoom"
      }
      (None, None) => "No molecule representation | L-drag rotate | Shift-L/M-drag pan | R-drag/wheel zoom",
    }
  }

  fn set_representation_layers(&mut self, representation: RepresentationLayers) -> bool {
    if self.representation == representation {
      return false;
    }
    self.representation = representation;
    if let Some(renderer) = self.renderer.as_mut() {
      renderer.set_representation_layers(
        MoleculeRenderInput {
          scene: &self.scene,
          surface: self.surface.as_ref(),
          surface_fragments: &[],
        },
        representation,
        &BallAndStickStyle::default(),
      );
    }
    true
  }

  fn set_molecular_surface(&mut self, surface: MolecularSurfaceArtifact) -> bool {
    let is_visible = self.representation.surface_style().is_some();
    if is_visible && let Some(renderer) = self.renderer.as_mut() {
      renderer.set_surface_artifact(&self.scene, &surface, &BallAndStickStyle::default());
    }
    self.surface = Some(surface);
    is_visible
  }

  fn clear_molecular_surface(&mut self) -> bool {
    if self.surface.take().is_none() {
      return false;
    }
    // Recreate GPU pipelines lazily so no buffers from the previous backend
    // survive into the next frame.
    self.renderer = None;
    self.representation.surface_style().is_some()
  }
}

/// Creates a WGPU document view from an already parsed, renderer-neutral scene.
///
/// This function must run on the GPUI event thread because surface and entity
/// creation require `Window` and `App`. File reading and parsing should happen
/// before this function is called.
///
/// # Parameters
///
/// * `scene` is the parsed structure scene shared with the renderer and surface task.
/// * `toast_viewport` receives surface lifecycle notifications without re-entering root app state.
/// * `window` creates the WGPU surface and identifies the window updated on completion.
/// * `cx` creates the panel entity and schedules background surface generation.
///
/// # Returns
///
/// A molecular document view whose surface representation is generated lazily.
pub(crate) fn build_structure_view_from_scene(
  scene: Arc<StructureScene>,
  toast_viewport: Entity<ToastViewport>,
  window: &mut Window,
  cx: &mut App,
) -> WgpuDocumentView {
  let surface = window.create_wgpu_surface(960, 540, wgpu::TextureFormat::Rgba8UnormSrgb);
  let rendering_scene = Arc::clone(&scene);
  let panel = cx.new(|_| {
    ChitinWgpuDocumentPanel::new_with_scene(
      surface,
      StructureMoleculeScene::new(rendering_scene, DEFAULT_STRUCTURE_REPRESENTATION),
    )
  });
  let generation_state = Rc::new(SurfaceGenerationState::new());
  let window_handle = window.window_handle();
  let representation_panel = panel.clone();
  let representation_scene = Arc::clone(&scene);
  let representation_toasts = toast_viewport.clone();
  let representation_state = Rc::clone(&generation_state);
  let view = WgpuDocumentView::with_representation_layers(
    panel.clone(),
    DEFAULT_STRUCTURE_REPRESENTATION,
    move |representation, cx| {
      representation_state
        .visible
        .set(representation.surface_style().is_some());
      representation_panel.update(cx, |panel, cx| {
        if panel.set_representation_layers(representation) {
          cx.notify();
        }
      });
      request_surface_generation(
        &representation_scene,
        &representation_panel,
        &representation_toasts,
        &representation_state,
        window_handle,
        cx,
      );
    },
  );

  let backend_scene = Arc::clone(&scene);
  let backend_toasts = toast_viewport;
  let backend_state = generation_state;
  view.with_surface_backend(MolecularSurfaceBackend::default(), move |backend, cx| {
    backend_state.select_backend(backend);
    panel.update(cx, |panel, cx| {
      if panel.clear_molecular_surface() {
        cx.notify();
      }
    });
    request_surface_generation(
      &backend_scene,
      &panel,
      &backend_toasts,
      &backend_state,
      window_handle,
      cx,
    );
  })
}

/// Starts the selected surface backend when its layer is visible and uncached.
///
/// # Parameters
///
/// * `scene` supplies the renderer-neutral atoms used by either backend.
/// * `panel` receives a completed artifact on the GPUI event thread.
/// * `toast_viewport` reports the active backend's lifecycle to the user.
/// * `state` rejects duplicate work and results from superseded selections.
/// * `window_handle` schedules completion work in the owning native window.
/// * `cx` schedules the background calculation.
fn request_surface_generation(
  scene: &Arc<StructureScene>,
  panel: &Entity<ChitinWgpuDocumentPanel>,
  toast_viewport: &Entity<ToastViewport>,
  state: &Rc<SurfaceGenerationState>,
  window_handle: AnyWindowHandle,
  cx: &mut App,
) {
  if !state.visible.get() {
    return;
  }
  let backend = state.backend.get();
  if state.ready_backend.get() == Some(backend) || state.pending_backend.get() == Some(backend) {
    return;
  }

  let generation = state.generation.get().wrapping_add(1);
  state.generation.set(generation);
  state.pending_backend.set(Some(backend));
  let backend_name = surface_backend_name(backend);
  let info_toast = toast_viewport.update(cx, |viewport, cx| {
    viewport.push(
      Toast::new(format!("Generating {backend_name} surface"))
        .description("Computing the solvent-excluded surface in the background.")
        .variant(ToastVariant::Info)
        .duration(None),
      cx,
    )
  });
  let scene = Arc::clone(scene);
  let panel = panel.downgrade();
  let toast_viewport = toast_viewport.clone();
  let state = Rc::clone(state);
  cx.spawn(async move |async_cx: &mut AsyncApp| {
    let result = async_cx
      .background_executor()
      .spawn(async move { generate_surface(&scene, backend) })
      .await;
    let _ = async_cx.update_window(window_handle, |_, _, cx| {
      toast_viewport.update(cx, |viewport, cx| viewport.dismiss(info_toast, cx));
      if state.generation.get() != generation || state.backend.get() != backend {
        return;
      }
      state.pending_backend.set(None);
      match result {
        Ok(surface) => {
          state.ready_backend.set(Some(backend));
          toast_viewport.update(cx, |viewport, cx| {
            viewport.push(
              Toast::new(format!("{backend_name} surface ready"))
                .description("Surface generation completed successfully.")
                .variant(ToastVariant::Success),
              cx,
            );
          });
          let _ = panel.update(cx, |panel, cx| {
            if panel.set_molecular_surface(surface) {
              cx.notify();
            }
          });
        }
        Err(error) => {
          log::error!("{backend_name} molecular surface generation failed: {error}");
          toast_viewport.update(cx, |viewport, cx| {
            viewport.push(
              Toast::new(format!("{backend_name} surface generation failed"))
                .description(error)
                .variant(ToastVariant::Error),
              cx,
            );
          });
        }
      }
    });
  })
  .detach();
}

/// Generates a molecular surface with the selected algorithm.
///
/// # Parameters
///
/// * `scene` supplies atoms, radii, and molecular coordinates.
/// * `backend` selects sampled implicit-field or analytical MSMS generation.
///
/// # Returns
///
/// Renderable surface geometry, or a backend error when generation fails or
/// produces no triangles.
fn generate_surface(
  scene: &StructureScene,
  backend: MolecularSurfaceBackend,
) -> Result<MolecularSurfaceArtifact, String> {
  let surface = match backend {
    MolecularSurfaceBackend::ImplicitScalarField => {
      generate_implicit_surface(scene, MolecularSurfaceRequest::default())
    }
    MolecularSurfaceBackend::Msms => {
      generate_msms_surface(scene, MsmsRequest::default(), MsmsTessellationParameters::default())
        .map_err(|error| error.to_string())?
    }
  };
  if molecular_surface_has_geometry(&surface) {
    Ok(surface)
  } else {
    Err("The calculation produced no renderable surface geometry.".to_string())
  }
}

/// Returns the user-facing name of one surface backend.
const fn surface_backend_name(backend: MolecularSurfaceBackend) -> &'static str {
  match backend {
    MolecularSurfaceBackend::ImplicitScalarField => "Implicit scalar field",
    MolecularSurfaceBackend::Msms => "MSMS",
  }
}

/// Reports whether a generated artifact contains triangles the renderer can draw.
fn molecular_surface_has_geometry(surface: &MolecularSurfaceArtifact) -> bool {
  surface.domains.iter().any(|domain| !domain.mesh.indices.is_empty())
}

/// Loads a local PDB or mmCIF file and extracts its first renderable model.
pub(crate) fn load_structure_scene(path: &Path) -> Result<Arc<StructureScene>, String> {
  let bytes = std::fs::read(path).map_err(|error| format!("cannot read '{}': {error}", path.display()))?;
  let extension = path
    .extension()
    .and_then(|extension| extension.to_str())
    .map(str::to_ascii_lowercase);
  let structure = match extension.as_deref() {
    Some("pdb") | Some("ent") => PdbParser::new()
      .parse_bytes(&bytes)
      .map(|parsed| parsed.structure)
      .map_err(|error| error.to_string())?,
    Some("cif") | Some("mmcif") => MmcifParser::new()
      .parse_bytes(&bytes)
      .map(|parsed| parsed.structure)
      .map_err(|error| error.to_string())?,
    _ => return Err("expected a .pdb, .ent, .cif, or .mmcif file".to_string()),
  };
  StructureScene::from_first_model(&structure)
    .map(Arc::new)
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
  use super::*;
  use chitin_molecule_renderer::SurfaceStyle;

  fn single_atom_scene() -> Arc<StructureScene> {
    let parsed = PdbParser::new()
      .parse_bytes(b"ATOM      1  C   GLY A   1       0.000   0.000   0.000  1.00 10.00           C  \nEND\n")
      .unwrap_or_else(|error| panic!("surface fixture should parse: {error}"));
    StructureScene::from_first_model(&parsed.structure)
      .map(Arc::new)
      .unwrap_or_else(|error| panic!("surface fixture should produce a scene: {error}"))
  }

  #[test]
  fn enabling_surface_should_defer_calculation() {
    let mut scene = StructureMoleculeScene::new(single_atom_scene(), DEFAULT_STRUCTURE_REPRESENTATION);
    let representation = DEFAULT_STRUCTURE_REPRESENTATION.with_surface(SurfaceStyle::Solid);

    assert!(scene.set_representation_layers(representation));
    assert!(scene.surface.is_none());
    assert_eq!(scene.representation, representation);
  }

  #[test]
  fn completed_surface_should_remain_cached_when_hidden() {
    let structure = single_atom_scene();
    let surface = generate_implicit_surface(&structure, MolecularSurfaceRequest::default());
    let mut scene = StructureMoleculeScene::new(structure, DEFAULT_STRUCTURE_REPRESENTATION);

    assert!(!scene.set_molecular_surface(surface));
    assert!(scene.surface.is_some());
  }

  #[test]
  fn clearing_surface_should_remove_cached_backend_geometry() {
    let structure = single_atom_scene();
    let surface = generate_implicit_surface(&structure, MolecularSurfaceRequest::default());
    let representation = DEFAULT_STRUCTURE_REPRESENTATION.with_surface(SurfaceStyle::Solid);
    let mut scene = StructureMoleculeScene::new(structure, representation);
    scene.set_molecular_surface(surface);

    assert!(scene.clear_molecular_surface());
    assert!(scene.surface.is_none());
  }

  #[test]
  fn selecting_backend_should_invalidate_cached_and_pending_work() {
    let state = SurfaceGenerationState::new();
    state
      .ready_backend
      .set(Some(MolecularSurfaceBackend::ImplicitScalarField));
    state
      .pending_backend
      .set(Some(MolecularSurfaceBackend::ImplicitScalarField));
    let previous_generation = state.generation.get();

    state.select_backend(MolecularSurfaceBackend::Msms);

    assert_eq!(state.backend.get(), MolecularSurfaceBackend::Msms);
    assert_eq!(state.ready_backend.get(), None);
    assert_eq!(state.pending_backend.get(), None);
    assert_ne!(state.generation.get(), previous_generation);
  }

  #[test]
  fn msms_backend_should_produce_analytical_surface_geometry() {
    let surface = generate_surface(&single_atom_scene(), MolecularSurfaceBackend::Msms)
      .unwrap_or_else(|error| panic!("single atom should produce an MSMS surface: {error}"));

    assert!(matches!(
      surface.source,
      chitin_bio::surface::SurfaceGeometrySource::Msms { .. }
    ));
  }

  #[test]
  fn surface_without_domains_should_not_be_renderable() {
    let surface = MolecularSurfaceArtifact {
      source: chitin_bio::surface::SurfaceGeometrySource::ImplicitGrid(MolecularSurfaceRequest::default()),
      domains: Vec::new(),
    };

    assert!(!molecular_surface_has_geometry(&surface));
  }
}
