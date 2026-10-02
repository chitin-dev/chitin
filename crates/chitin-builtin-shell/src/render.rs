//! Shell-only grammar for typed molecular presentation commands.

use chitin_command::{RenderAtomStyle, RenderCommand, RenderPolymerStyle, RenderSurfaceBackend, RenderSurfaceStyle};
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
  /// Set the atom style, or disable the layer with none.
  Style {
    #[arg(value_enum)]
    style: RenderAtomStyle,
  },
}

#[derive(Debug, Subcommand)]
pub(crate) enum PolymerArgs {
  /// Set the polymer style, or disable the layer with none.
  Style {
    #[arg(value_enum)]
    style: RenderPolymerStyle,
  },
}

#[derive(Debug, Subcommand)]
pub(crate) enum SurfaceArgs {
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

impl RenderArgs {
  pub(crate) fn into_command(self) -> RenderCommand {
    match self {
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
