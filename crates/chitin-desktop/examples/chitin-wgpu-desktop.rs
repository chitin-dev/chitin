#![forbid(unsafe_code)]
//! Chitin desktop shell with an interactive WGPU document-area panel.
//!
//! Run with `cargo run --example chitin-wgpu-desktop -- . structure.pdb`.
//! Add `--representation stick|ball-and-stick|sphere` to choose the atom view.
//!
//! This example validates the integration path where GPUI owns the app shell,
//! document tabs, splits, and side panels while WGPU renders an interactive
//! viewport inside one document tab.

#[path = "./chitin-wgpu/molecule.rs"]
mod molecule;

use std::{
  borrow::Cow,
  fs,
  path::{Path, PathBuf},
  sync::Arc,
};

use chitin_bio::structure::{MmcifParser, PdbParser, StructureScene};
use chitin_desktop::{
  app::{ChitinApp, WgpuDocumentView, WgpuDocumentViewFactory},
  keybindings::default_key_bindings,
  wgpu_panel::ChitinWgpuDocumentPanel,
};
use chitin_wgpu::AtomRepresentation;
use gpui::{
  App, AppContext, Application, AssetSource, Bounds, Result, SharedString, WindowBounds, WindowOptions, px, size,
};
use molecule::ExampleMoleculeScene;

/// Parsed command-line arguments for the WGPU molecule example.
struct ExampleArguments {
  /// Workspace path passed to the desktop shell.
  project_path: Option<PathBuf>,
  /// PDB or mmCIF file to display.
  structure_path: PathBuf,
  /// Atom-level display representation.
  representation: AtomRepresentation,
}

/// Parses positional paths and the atom representation option.
fn parse_example_arguments() -> std::result::Result<ExampleArguments, String> {
  let mut positional = Vec::new();
  let mut representation = AtomRepresentation::default();
  let mut arguments = std::env::args_os().skip(1);

  while let Some(argument) = arguments.next() {
    let argument = argument.to_string_lossy();
    if argument == "--representation" {
      let Some(value) = arguments.next() else {
        return Err("--representation requires stick, ball-and-stick, or sphere".to_string());
      };
      let value = value.to_string_lossy();
      representation = AtomRepresentation::from_name(&value)
        .ok_or_else(|| format!("unknown representation {value:?}; expected stick, ball-and-stick, or sphere"))?;
    } else if let Some(value) = argument.strip_prefix("--representation=") {
      representation = AtomRepresentation::from_name(value)
        .ok_or_else(|| format!("unknown representation {value:?}; expected stick, ball-and-stick, or sphere"))?;
    } else if argument.starts_with('-') {
      return Err(format!("unknown option {argument:?}"));
    } else {
      positional.push(PathBuf::from(argument.as_ref()));
    }
  }

  if positional.len() > 2 {
    return Err("expected PROJECT_PATH and STRUCTURE_PATH".to_string());
  }

  let structure_path = positional
    .get(1)
    .cloned()
    .ok_or_else(|| "missing STRUCTURE_PATH; pass a .pdb, .ent, .cif, or .mmcif file".to_string())?;

  Ok(ExampleArguments {
    project_path: positional.first().cloned(),
    structure_path,
    representation,
  })
}

/// GPUI asset source backed by the repository's `assets/` directory.
struct DesktopAssets {
  /// Filesystem directory containing desktop assets.
  base: PathBuf,
}

impl AssetSource for DesktopAssets {
  /// Loads one asset file from the configured desktop asset directory.
  ///
  /// # Parameters
  ///
  /// * `path` is the asset-relative path requested by GPUI.
  ///
  /// # Returns
  ///
  /// `Ok(Some(bytes))` when the asset exists and is readable.
  fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
    fs::read(self.base.join(path))
      .map(|data| Some(Cow::Owned(data)))
      .map_err(Into::into)
  }

  /// Lists child asset names inside an asset directory.
  ///
  /// # Parameters
  ///
  /// * `path` is the asset-relative directory path requested by GPUI.
  ///
  /// # Returns
  ///
  /// `Ok(Vec<SharedString>)` with UTF-8 child asset names.
  fn list(&self, path: &str) -> Result<Vec<SharedString>> {
    fs::read_dir(self.base.join(path))
      .map(|entries| {
        entries
          .filter_map(|entry| {
            entry
              .ok()
              .and_then(|entry| entry.file_name().into_string().ok())
              .map(SharedString::from)
          })
          .collect()
      })
      .map_err(Into::into)
  }
}

/// Starts the desktop WGPU integration example.
fn main() {
  env_logger::init();
  let ExampleArguments {
    project_path,
    structure_path,
    representation,
  } = match parse_example_arguments() {
    Ok(arguments) => arguments,
    Err(error) => {
      eprintln!("invalid arguments: {error}");
      return;
    }
  };
  let (scene, title) = match load_structure_scene(&structure_path) {
    Ok(scene) => scene,
    Err(error) => {
      eprintln!("failed to load molecular structure example: {error}");
      return;
    }
  };
  let scene = Arc::new(scene);

  Application::new()
    .with_assets(DesktopAssets {
      base: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
    })
    .run(move |cx: &mut App| {
      cx.bind_keys(default_key_bindings());

      let bounds = Bounds::centered(None, size(px(1180.0), px(800.0)), cx);
      let result = cx.open_window(
        WindowOptions {
          window_bounds: Some(WindowBounds::Windowed(bounds)),
          app_id: Some("dev.chitin.ChitinWgpuDesktop".to_string()),
          ..Default::default()
        },
        |window, cx| {
          let project_sidebar_focus = cx.focus_handle();
          window.focus(&project_sidebar_focus, cx);
          window.activate_window();

          let factory_scene = Arc::clone(&scene);
          let factory_representation = representation;
          let wgpu_panel_factory = WgpuDocumentViewFactory::new(move |window, cx| {
            let surface = window.create_wgpu_surface(960, 540, wgpu::TextureFormat::Rgba8UnormSrgb);
            let panel_scene = ExampleMoleculeScene::new(Arc::clone(&factory_scene), factory_representation);
            let panel = cx.new(|_| ChitinWgpuDocumentPanel::new_with_scene(surface, panel_scene));
            let controlled_panel = panel.clone();
            WgpuDocumentView::with_atom_representation(panel, factory_representation, move |representation, cx| {
              controlled_panel.update(cx, |panel, cx| {
                if panel.set_atom_representation(representation) {
                  cx.notify();
                }
              });
            })
          });
          let wgpu_panel = wgpu_panel_factory.build(window, cx);

          cx.new(|_| {
            ChitinApp::new_with_wgpu_document_panel(
              project_path,
              project_sidebar_focus,
              title.clone(),
              wgpu_panel,
              wgpu_panel_factory,
            )
          })
        },
      );

      if let Err(error) = result {
        eprintln!("failed to open Chitin WGPU desktop window: {error}");
        cx.quit();
        return;
      }

      cx.activate(true);
    });
}

/// Loads a PDB or mmCIF file and extracts its first renderer-neutral scene.
///
/// # Parameters
///
/// * `path` selects a local `.pdb`, `.cif`, or `.mmcif` file.
///
/// # Returns
///
/// The extracted first-model scene and document title, or a readable parsing
/// error for unsupported and malformed input.
fn load_structure_scene(path: &Path) -> std::result::Result<(StructureScene, String), String> {
  let bytes = fs::read(path).map_err(|error| format!("cannot read '{}': {error}", path.display()))?;
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
    _ => {
      return Err(format!(
        "'{}' must use a .pdb, .ent, .cif, or .mmcif extension",
        path.display()
      ));
    }
  };
  let title = path
    .file_name()
    .map(|name| name.to_string_lossy().into_owned())
    .unwrap_or_else(|| path.display().to_string());

  StructureScene::from_first_model(&structure)
    .map(|scene| (scene, title))
    .map_err(|error| error.to_string())
}
