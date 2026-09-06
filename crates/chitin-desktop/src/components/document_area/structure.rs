//! Structure-file loading and molecular WGPU scene integration.

use std::{path::Path, sync::Arc};

use chitin_bio::structure::{MmcifParser, PdbParser, StructureScene};
use chitin_molecule_renderer::{
  AtomStyle, BallAndStickStyle, MoleculeDebugMode, MoleculeRenderer, PolymerStyle, RepresentationLayers,
};
use gpui::{App, AppContext, Window};

use crate::components::{
  document_area::state::WgpuDocumentView,
  wgpu_panel::{ChitinWgpuDocumentPanel, WgpuPanelFrame, WgpuPanelScene},
};

/// Default representation used when GPUI opens a PDB or mmCIF document.
const DEFAULT_STRUCTURE_REPRESENTATION: RepresentationLayers =
  RepresentationLayers::atom(AtomStyle::Stick).with_polymer(PolymerStyle::Cartoon);

/// Molecular scene backed by one parsed structure and a selectable representation.
pub(crate) struct StructureMoleculeScene {
  /// Renderer-neutral structure data shared by split-panel clones.
  scene: Arc<StructureScene>,
  /// GPU resources created lazily after a surface device is available.
  renderer: Option<MoleculeRenderer>,
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
        &self.scene,
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
    self.renderer = None;
    true
  }
}

/// Creates a WGPU document view from an already parsed, renderer-neutral scene.
///
/// This function must run on the GPUI event thread because surface and entity
/// creation require `Window` and `App`. File reading and parsing should happen
/// before this function is called.
pub(crate) fn build_structure_view_from_scene(
  scene: Arc<StructureScene>,
  window: &mut Window,
  cx: &mut App,
) -> WgpuDocumentView {
  let surface = window.create_wgpu_surface(960, 540, wgpu::TextureFormat::Rgba8UnormSrgb);
  let panel = cx.new(|_| {
    ChitinWgpuDocumentPanel::new_with_scene(
      surface,
      StructureMoleculeScene::new(scene, DEFAULT_STRUCTURE_REPRESENTATION),
    )
  });
  let controlled_panel = panel.clone();
  WgpuDocumentView::with_representation_layers(panel, DEFAULT_STRUCTURE_REPRESENTATION, move |representation, cx| {
    controlled_panel.update(cx, |panel, cx| {
      if panel.set_representation_layers(representation) {
        cx.notify();
      }
    });
  })
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
