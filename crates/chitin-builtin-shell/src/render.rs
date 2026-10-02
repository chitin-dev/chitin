//! Shell-only grammar for typed molecular presentation commands.

use chitin_command::{
  RenderAtomStyle, RenderColorScheme, RenderCommand, RenderLayer, RenderOpacity, RenderPolymerStyle, RenderRgb,
  RenderSurfaceBackend, RenderSurfaceStyle,
};
use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub(crate) enum RenderArgs {
  /// Show the selected rendering view and its presentation settings.
  Status,
  /// Configure the atom-and-bond layer.
  Atom {
    #[command(subcommand)]
    command: AtomArgs,
  },
  /// Configure the polymer layer.
  Polymer {
    #[command(subcommand)]
    command: PolymerArgs,
  },
  /// Configure the molecular surface.
  Surface {
    #[command(subcommand)]
    command: SurfaceArgs,
  },
}

#[derive(Debug, Subcommand)]
pub(crate) enum AtomArgs {
  #[command(flatten)]
  Appearance(AppearanceArgs),
  /// Set the atom style, or disable the layer with none.
  Style {
    #[arg(value_enum)]
    style: RenderAtomStyle,
  },
}

#[derive(Debug, Subcommand)]
pub(crate) enum PolymerArgs {
  #[command(flatten)]
  Appearance(AppearanceArgs),
  /// Set the polymer style, or disable the layer with none.
  Style {
    #[arg(value_enum)]
    style: RenderPolymerStyle,
  },
}

#[derive(Debug, Subcommand)]
pub(crate) enum SurfaceArgs {
  #[command(flatten)]
  Appearance(AppearanceArgs),
  /// Set the surface style, or disable the layer with none.
  Style {
    #[arg(value_enum)]
    style: RenderSurfaceStyle,
  },
  /// Set the generation algorithm without changing surface visibility.
  Backend {
    #[arg(value_enum)]
    backend: RenderSurfaceBackend,
  },
}

#[derive(Debug, Subcommand)]
pub(crate) enum AppearanceArgs {
  /// Set coloring; uniform accepts --value "#RRGGBB".
  Color {
    #[arg(value_enum)]
    scheme: RenderColorScheme,
    #[arg(long)]
    value: Option<RenderRgb>,
  },
  /// Set opacity from 0 (transparent) to 1 (opaque).
  Opacity { value: RenderOpacity },
}

impl AppearanceArgs {
  fn into_command(self, layer: RenderLayer) -> RenderCommand {
    match self {
      Self::Color { scheme, value } => RenderCommand::Color { layer, scheme, value },
      Self::Opacity { value } => RenderCommand::Opacity { layer, value },
    }
  }
}

impl RenderArgs {
  pub(crate) fn into_command(self) -> RenderCommand {
    match self {
      Self::Atom {
        command: AtomArgs::Appearance(args),
      } => args.into_command(RenderLayer::Atom),
      Self::Polymer {
        command: PolymerArgs::Appearance(args),
      } => args.into_command(RenderLayer::Polymer),
      Self::Surface {
        command: SurfaceArgs::Appearance(args),
      } => args.into_command(RenderLayer::Surface),
      Self::Status => RenderCommand::Status,
      Self::Atom {
        command: AtomArgs::Style { style },
      } => RenderCommand::AtomStyle(style),
      Self::Polymer {
        command: PolymerArgs::Style { style },
      } => RenderCommand::PolymerStyle(style),
      Self::Surface {
        command: SurfaceArgs::Style { style },
      } => RenderCommand::SurfaceStyle(style),
      Self::Surface {
        command: SurfaceArgs::Backend { backend },
      } => RenderCommand::SurfaceBackend(backend),
    }
  }
}
