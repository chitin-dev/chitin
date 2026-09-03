//! WebGPU and JavaScript bindings for the browser molecule viewer.
//!
//! The bridge deliberately keeps browser concerns at this boundary. JavaScript
//! supplies the canvas and file bytes, `chitin-bio` parses the structure, and
//! `chitin-wgpu` owns the renderer, camera, and GPU resource details. Keeping
//! those responsibilities separate lets the native renderer and browser
//! renderer share the same scene representation.

use std::rc::Rc;

use chitin_bio::structure::{MmcifParser, PdbParser, StructureScene};
use chitin_wgpu::{
  AtomRepresentation, BallAndStickStyle, ClearRenderer, DragMode, GpuHandle, MoleculeRenderer, RenderTargetSize,
  ViewerCamera, ViewportDrag,
};
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;
use wgpu::{CurrentSurfaceTexture, SurfaceTarget};

/// Creates a browser molecule viewer attached to an HTML canvas.
///
/// # Parameters
///
/// * `canvas` is the HTML canvas that owns the browser presentation surface.
///
/// # Returns
///
/// A [`MoleculeViewer`] retaining the configured surface, device, queue, and
/// size-dependent render resources, or a JavaScript error when WebGPU cannot
/// create a compatible adapter or device.
#[wasm_bindgen]
pub async fn create_viewer(canvas: HtmlCanvasElement) -> Result<MoleculeViewer, JsValue> {
  // Install readable panic messages before any asynchronous GPU operation can
  // fail; these messages are otherwise difficult to diagnose from JavaScript.
  console_error_panic_hook::set_once();

  // Browser WebGPU does not use the native display-handle backends. Restricting
  // the instance here also prevents an accidental fallback to an unavailable
  // native backend in a wasm build.
  let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
    backends: wgpu::Backends::BROWSER_WEBGPU,
    ..wgpu::InstanceDescriptor::new_without_display_handle()
  });
  // The canvas is cloned because wgpu keeps the browser surface target alive
  // for as long as the surface is used by the viewer.
  let surface = instance
    .create_surface(SurfaceTarget::Canvas(canvas.clone()))
    .map_err(|error| js_error(format!("failed to create WebGPU canvas surface: {error}")))?;
  // Prefer a high-performance adapter while requiring presentation to this
  // exact canvas; an adapter without a compatible surface is not useful here.
  let adapter = instance
    .request_adapter(&wgpu::RequestAdapterOptions {
      power_preference: wgpu::PowerPreference::HighPerformance,
      compatible_surface: Some(&surface),
      ..Default::default()
    })
    .await
    .map_err(|error| js_error(format!("no compatible WebGPU adapter: {error}")))?;
  // The default device limits are sufficient for the shared molecule shaders,
  // and avoiding custom limits keeps this bridge portable across browsers.
  let (device, queue) = adapter
    .request_device(&wgpu::DeviceDescriptor {
      label: Some("chitin_browser_device"),
      ..Default::default()
    })
    .await
    .map_err(|error| js_error(format!("failed to create WebGPU device: {error}")))?;

  // A zero-sized drawing buffer is invalid for surface configuration, so the
  // initial dimensions follow the same minimum-size rule as `resize`.
  let width = canvas.width().max(1);
  let height = canvas.height().max(1);
  let config = surface
    .get_default_config(&adapter, width, height)
    .ok_or_else(|| js_error("the WebGPU adapter cannot present to this canvas"))?;
  surface.configure(&device, &config);

  let device = Rc::new(device);
  let queue = Rc::new(queue);
  let size = RenderTargetSize::new(width, height);
  // Render a neutral background before a structure is loaded. This makes the
  // canvas immediately presentable while parsing and renderer construction
  // remain explicit operations.
  let clear_renderer = ClearRenderer::new(
    Rc::clone(&device),
    Rc::clone(&queue),
    size,
    wgpu::Color {
      r: 0.018,
      g: 0.024,
      b: 0.038,
      a: 1.0,
    },
  );

  Ok(MoleculeViewer {
    surface,
    device,
    queue,
    config,
    clear_renderer,
    renderer: None,
    scene: None,
    representation: AtomRepresentation::BallAndStick,
    camera: ViewerCamera::default(),
    active_drag: None,
  })
}

/// WebAssembly-owned molecule renderer and interaction state.
///
/// A viewer owns one configured surface and may replace its molecule renderer
/// whenever the structure or representation changes. The scene is retained so
/// the renderer can be rebuilt without asking JavaScript to resend the file.
#[wasm_bindgen]
pub struct MoleculeViewer {
  /// Canvas presentation surface configured for the current drawing-buffer size.
  surface: wgpu::Surface<'static>,
  /// Shared device used by clear and molecule renderers.
  device: GpuHandle<wgpu::Device>,
  /// Shared submission queue used to present completed browser frames.
  queue: GpuHandle<wgpu::Queue>,
  /// Current surface format and drawing-buffer dimensions.
  config: wgpu::SurfaceConfiguration,
  /// Background renderer used before a molecule renderer exists.
  clear_renderer: ClearRenderer,
  /// Renderer for the currently loaded scene, if one has been built.
  renderer: Option<MoleculeRenderer>,
  /// Renderer-neutral scene retained for representation changes.
  scene: Option<StructureScene>,
  /// Representation used when rebuilding the molecule renderer.
  representation: AtomRepresentation,
  /// Camera state shared by pointer, wheel, and render operations.
  camera: ViewerCamera,
  /// Pointer gesture currently being applied to the camera.
  active_drag: Option<ViewportDrag>,
}

#[wasm_bindgen]
impl MoleculeViewer {
  /// Parses PDB or mmCIF bytes and replaces the displayed molecule.
  ///
  /// # Parameters
  ///
  /// * `bytes` contains one PDB or mmCIF structure encoded as UTF-8 bytes.
  /// * `format` selects the parser and accepts `pdb`, `cif`, or `mmcif`.
  ///
  /// # Returns
  ///
  /// A human-readable atom and bond count after rebuilding the renderer and
  /// resetting the camera, or a JavaScript error when parsing or scene
  /// extraction fails. The input bytes are borrowed only for parsing.
  pub fn load_structure(&mut self, bytes: &[u8], format: &str) -> Result<String, JsValue> {
    let scene = parse_scene(bytes, format).map_err(js_error)?;
    let summary = format!("{} atoms · {} bonds", scene.atoms.len(), scene.bonds.len());
    self.scene = Some(scene);
    self.rebuild_renderer();
    self.camera.reset();
    Ok(summary)
  }

  /// Selects `stick`, `ball-and-stick`, or `sphere` atom rendering.
  ///
  /// # Parameters
  ///
  /// * `representation` is one of `stick`, `ball-and-stick`, or `sphere`.
  ///
  /// # Returns
  ///
  /// This function returns `()` after storing the mode and rebuilding the
  /// renderer from the retained scene, or a JavaScript error for an unsupported
  /// mode. When no scene is loaded, the mode is used by the next load operation.
  pub fn set_representation(&mut self, representation: &str) -> Result<(), JsValue> {
    self.representation = match representation {
      "stick" => AtomRepresentation::Stick,
      "ball-and-stick" => AtomRepresentation::BallAndStick,
      "sphere" => AtomRepresentation::Sphere,
      _ => return Err(js_error(format!("unsupported representation: {representation}"))),
    };
    self.rebuild_renderer();
    Ok(())
  }

  /// Resizes the surface and size-dependent depth resources.
  ///
  /// # Parameters
  ///
  /// * `width` and `height` are physical canvas dimensions supplied by the
  ///   browser-side resize observer.
  ///
  /// # Returns
  ///
  /// This function returns `()` after reconfiguring the surface and refreshing
  /// size-dependent depth resources when either dimension changes.
  pub fn resize(&mut self, width: u32, height: u32) {
    let width = width.max(1);
    let height = height.max(1);
    if self.config.width == width && self.config.height == height {
      return;
    }

    self.config.width = width;
    self.config.height = height;
    self.surface.configure(&self.device, &self.config);
    let size = RenderTargetSize::new(width, height);
    self.clear_renderer.resize_if_needed(size);
    if let Some(renderer) = self.renderer.as_mut() {
      renderer.resize_if_needed(size);
    }
  }

  /// Draws and presents one browser frame.
  ///
  /// # Returns
  ///
  /// This function returns `()` after acquiring, drawing, and presenting one
  /// surface texture. Timeout, occlusion, and outdated-surface states are
  /// treated as recoverable; lost and validation-failed surfaces return a
  /// JavaScript error so the UI can stop scheduling frames.
  pub fn render(&mut self) -> Result<(), JsValue> {
    let frame = match self.surface.get_current_texture() {
      CurrentSurfaceTexture::Success(frame) | CurrentSurfaceTexture::Suboptimal(frame) => frame,
      CurrentSurfaceTexture::Timeout | CurrentSurfaceTexture::Occluded => return Ok(()),
      CurrentSurfaceTexture::Outdated => {
        self.surface.configure(&self.device, &self.config);
        return Ok(());
      }
      CurrentSurfaceTexture::Lost => return Err(js_error("the WebGPU canvas surface was lost")),
      CurrentSurfaceTexture::Validation => return Err(js_error("WebGPU rejected the current canvas frame")),
    };
    let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());

    if let Some(renderer) = self.renderer.as_mut() {
      renderer.render(
        &view,
        self.camera.view_matrix(),
        self.camera.projection_matrix(renderer.aspect()),
      );
    } else {
      self.clear_renderer.render(&view);
    }
    drop(view);
    self.queue.present(frame);
    Ok(())
  }

  /// Starts a pointer drag using DOM button numbering.
  ///
  /// # Parameters
  ///
  /// * `button` follows the DOM `PointerEvent.button` numbering.
  /// * `shift_key` changes primary-button rotation into panning.
  /// * `x` and `y` are canvas-local pointer coordinates in logical pixels.
  ///
  /// # Returns
  ///
  /// This function returns `()` after recording a supported camera gesture.
  /// Unknown buttons are ignored.
  pub fn pointer_down(&mut self, button: i16, shift_key: bool, x: f32, y: f32) {
    let mode = match (button, shift_key) {
      (0, false) => DragMode::Rotate,
      (0, true) | (1, _) => DragMode::Pan,
      (2, _) => DragMode::Zoom,
      _ => return,
    };
    self.active_drag = Some(ViewportDrag {
      mode,
      last_x: x,
      last_y: y,
    });
  }

  /// Applies the latest pointer position to an active drag.
  ///
  /// # Parameters
  ///
  /// * `x` and `y` are the latest canvas-local pointer coordinates in logical
  ///   pixels.
  ///
  /// # Returns
  ///
  /// This function returns `()` after applying the delta to the selected camera
  /// operation. Calls without an active drag are no-ops.
  pub fn pointer_move(&mut self, x: f32, y: f32) {
    let Some(mut drag) = self.active_drag else {
      return;
    };
    let delta_x = x - drag.last_x;
    let delta_y = y - drag.last_y;
    match drag.mode {
      DragMode::Rotate => self.camera.rotate_pixels(delta_x, delta_y),
      DragMode::Pan => self.camera.pan_pixels(delta_x, delta_y),
      DragMode::Zoom => self.camera.zoom_pixels(delta_y),
    }
    drag.last_x = x;
    drag.last_y = y;
    self.active_drag = Some(drag);
  }

  /// Ends the active pointer drag.
  pub fn pointer_up(&mut self) {
    self.active_drag = None;
  }

  /// Applies a DOM wheel delta to camera zoom.
  pub fn zoom(&mut self, delta_y: f32) {
    self.camera.zoom_pixels(delta_y);
  }

  /// Restores the default camera orientation.
  pub fn reset_camera(&mut self) {
    self.camera.reset();
  }
}

impl MoleculeViewer {
  /// Rebuilds the GPU renderer from the retained scene and current view mode.
  ///
  /// The same construction path is used after structure loads and
  /// representation changes so size, format, style, and GPU ownership remain
  /// consistent. With no scene, the viewer falls back to its clear renderer.
  fn rebuild_renderer(&mut self) {
    let Some(scene) = self.scene.as_ref() else {
      self.renderer = None;
      return;
    };
    let size = RenderTargetSize::new(self.config.width, self.config.height);
    self.renderer = Some(MoleculeRenderer::new_with_representation(
      Rc::clone(&self.device),
      Rc::clone(&self.queue),
      size,
      self.config.format,
      scene,
      self.representation,
      &BallAndStickStyle::default(),
    ));
  }
}

/// Parses caller-provided structure bytes into a renderer-neutral scene.
///
/// # Parameters
///
/// * `bytes` contains the encoded structure data.
/// * `format` selects the PDB or mmCIF parser.
///
/// # Returns
///
/// A [`StructureScene`] projected from the first model, or a format-specific
/// parse or scene-extraction error.
fn parse_scene(bytes: &[u8], format: &str) -> Result<StructureScene, String> {
  let parsed = match format {
    "pdb" => PdbParser::new()
      .parse_bytes(bytes)
      .map_err(|error| format!("PDB parse failed: {error}"))?,
    "cif" | "mmcif" => MmcifParser::new()
      .parse_bytes(bytes)
      .map_err(|error| format!("mmCIF parse failed: {error}"))?,
    _ => return Err(format!("unsupported structure format: {format}")),
  };
  StructureScene::from_first_model(&parsed.structure).map_err(|error| format!("scene extraction failed: {error}"))
}

/// Converts a Rust error message into a JavaScript `Error` value.
fn js_error(message: impl Into<String>) -> JsValue {
  js_sys::Error::new(&message.into()).into()
}
