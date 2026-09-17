//! Molecule scene adapter for the desktop WGPU integration example.

use std::sync::Arc;

use chitin_bio::{
  structure::StructureScene,
  surface::{
    MolecularSurfaceArtifact, MolecularSurfaceRequest, generate_implicit_surface,
    msms::{MsmsRequest, MsmsTessellationParameters, generate_msms_surface},
  },
};
use chitin_desktop::wgpu_panel::{WgpuPanelFrame, WgpuPanelScene};
use chitin_molecule_renderer::{
  BallAndStickStyle, MoleculeDebugMode, MoleculeRenderInput, MoleculeRenderer, RepresentationLayers,
};

/// Lazily initializes a reusable molecular renderer for a structure scene.
pub struct ExampleMoleculeScene {
  /// CPU-side renderer-neutral structure data shared by split panel clones.
  scene: Arc<StructureScene>,
  /// GPU resources created after GPUI provides a concrete surface device.
  renderer: Option<MoleculeRenderer>,
  /// Scientific surface geometry computed independently of GPU resources.
  surface: Option<MolecularSurfaceArtifact>,
  /// Shader output selected through `CHITIN_MOLECULE_DEBUG_MODE`.
  debug_mode: MoleculeDebugMode,
  /// Representation layers selected by the example command line.
  representation: RepresentationLayers,
  /// Surface algorithm selected by the example command line.
  surface_backend: ExampleSurfaceBackend,
}

/// Molecular-surface backend available to the desktop integration example.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExampleSurfaceBackend {
  /// Stable sampled implicit-field renderer.
  #[default]
  Implicit,
  /// Analytical patch renderer with MSMS-style singularity handling.
  Msms,
}

impl ExampleMoleculeScene {
  /// Creates a lazy molecule scene from shared renderer-neutral data.
  pub fn new(
    scene: Arc<StructureScene>,
    representation: RepresentationLayers,
    surface_backend: ExampleSurfaceBackend,
  ) -> Self {
    let debug_mode = molecule_debug_mode_from_env();
    let surface = molecular_surface_for_layers(&scene, representation, None, surface_backend);
    Self {
      scene,
      renderer: None,
      surface,
      debug_mode,
      representation,
      surface_backend,
    }
  }
}

impl WgpuPanelScene for ExampleMoleculeScene {
  /// Renders one fitted atom-and-explicit-bond frame.
  ///
  /// # Parameters
  ///
  /// * `frame` contains the GPUI surface resources and interactive camera.
  ///
  /// # Returns
  ///
  /// The queue submission index used by the panel to present the frame.
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

  /// Returns the molecule-specific interaction hint.
  fn interaction_hint(&self) -> &'static str {
    "Atom representation | L-drag rotate | Shift-L/M-drag pan | R-drag/wheel zoom"
  }

  fn set_representation_layers(&mut self, representation: RepresentationLayers) -> bool {
    if self.representation == representation {
      return false;
    }
    self.representation = representation;
    self.surface = molecular_surface_for_layers(&self.scene, representation, self.surface.take(), self.surface_backend);
    self.renderer = None;
    true
  }
}

/// Computes the default protein-chain surface only when its layer is enabled.
fn molecular_surface_for_layers(
  scene: &StructureScene,
  representation: RepresentationLayers,
  current: Option<MolecularSurfaceArtifact>,
  backend: ExampleSurfaceBackend,
) -> Option<MolecularSurfaceArtifact> {
  match (representation.surface_style(), current) {
    (Some(_), Some(surface)) => Some(surface),
    (Some(_), None) => match backend {
      ExampleSurfaceBackend::Implicit => Some(generate_implicit_surface(scene, MolecularSurfaceRequest::default())),
      ExampleSurfaceBackend::Msms => {
        match generate_msms_surface(scene, MsmsRequest::default(), MsmsTessellationParameters::default()) {
          Ok(surface) => Some(surface),
          Err(error) => {
            log::error!("MSMS surface generation failed: {error}");
            None
          }
        }
      }
    },
    (None, _) => None,
  }
}

/// Reads the optional molecule shader diagnostic mode from the environment.
fn molecule_debug_mode_from_env() -> MoleculeDebugMode {
  let Ok(value) = std::env::var("CHITIN_MOLECULE_DEBUG_MODE") else {
    return MoleculeDebugMode::Final;
  };
  let Some(mode) = MoleculeDebugMode::from_name(&value) else {
    log::warn!(
      "unknown CHITIN_MOLECULE_DEBUG_MODE={value:?}; using final (expected final, normal, key-diffuse, fill-diffuse, specular, depth-cue, or element-color)"
    );
    return MoleculeDebugMode::Final;
  };
  log::info!("molecule shader debug mode: {mode:?}");
  mode
}
