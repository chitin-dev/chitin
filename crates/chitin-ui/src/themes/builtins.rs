//! Built-in UI themes.
//!
//! Built-ins are the only place in `chitin-ui` where concrete color values
//! should be defined. Components consume semantic tokens from [`UIThemes`]
//! instead of hardcoding colors directly.

use super::{UIAccentColors, UIBackgroundColors, UIBorderColors, UITextColors, UIThemes};
use catppuccin::{latte, macchiato};

const fn rgb_const(hex: u32) -> gpui::Rgba {
  gpui::Rgba {
    r: ((hex >> 16) & 0xff) as f32 / 255.0,
    g: ((hex >> 8) & 0xff) as f32 / 255.0,
    b: (hex & 0xff) as f32 / 255.0,
    a: 1.0,
  }
}

const fn rgba_const(hex: u32, alpha: f32) -> gpui::Rgba {
  gpui::Rgba {
    r: ((hex >> 16) & 0xff) as f32 / 255.0,
    g: ((hex >> 8) & 0xff) as f32 / 255.0,
    b: (hex & 0xff) as f32 / 255.0,
    a: alpha,
  }
}

#[allow(dead_code)]
mod catppuccin {
  use gpui::Rgba;

  use super::{rgb_const, rgba_const};

  // Catppuccin Macchiato. See https://catppuccin.com/palette/
  pub(super) mod macchiato {
    use super::{Rgba, rgb_const, rgba_const};

    pub(in crate::themes::builtins) const ROSEWATER: Rgba = rgb_const(0xf4dbd6);
    pub(in crate::themes::builtins) const FLAMINGO: Rgba = rgb_const(0xf0c6c6);
    pub(in crate::themes::builtins) const PINK: Rgba = rgb_const(0xf5bde6);
    pub(in crate::themes::builtins) const MAUVE: Rgba = rgb_const(0xc6a0f6);
    pub(in crate::themes::builtins) const RED: Rgba = rgb_const(0xed8796);
    pub(in crate::themes::builtins) const MAROON: Rgba = rgb_const(0xee99a0);
    pub(in crate::themes::builtins) const PEACH: Rgba = rgb_const(0xf5a97f);
    pub(in crate::themes::builtins) const YELLOW: Rgba = rgb_const(0xeed49f);
    pub(in crate::themes::builtins) const GREEN: Rgba = rgb_const(0xa6da95);
    pub(in crate::themes::builtins) const TEAL: Rgba = rgb_const(0x8bd5ca);
    pub(in crate::themes::builtins) const SKY: Rgba = rgb_const(0x91d7e3);
    pub(in crate::themes::builtins) const SAPPHIRE: Rgba = rgb_const(0x7dc4e4);
    pub(in crate::themes::builtins) const BLUE: Rgba = rgb_const(0x8aadf4);
    pub(in crate::themes::builtins) const LAVENDER: Rgba = rgb_const(0xb7bdf8);
    pub(in crate::themes::builtins) const TEXT: Rgba = rgb_const(0xcad3f5);
    pub(in crate::themes::builtins) const SUBTEXT1: Rgba = rgb_const(0xb8c0e0);
    pub(in crate::themes::builtins) const SUBTEXT0: Rgba = rgb_const(0xa5adcb);
    pub(in crate::themes::builtins) const OVERLAY2: Rgba = rgb_const(0x939ab7);
    pub(in crate::themes::builtins) const OVERLAY1: Rgba = rgb_const(0x8087a2);
    pub(in crate::themes::builtins) const OVERLAY0: Rgba = rgb_const(0x6e738d);
    pub(in crate::themes::builtins) const SURFACE2: Rgba = rgb_const(0x5b6078);
    pub(in crate::themes::builtins) const SURFACE1: Rgba = rgb_const(0x494d64);
    pub(in crate::themes::builtins) const SURFACE0: Rgba = rgb_const(0x363a4f);
    pub(in crate::themes::builtins) const BASE: Rgba = rgb_const(0x24273a);
    pub(in crate::themes::builtins) const MANTLE: Rgba = rgb_const(0x1e2030);
    pub(in crate::themes::builtins) const CRUST: Rgba = rgb_const(0x181926);

    pub(in crate::themes::builtins) const TEXT_10: Rgba = rgba_const(0xcad3f5, 0.1);
    pub(in crate::themes::builtins) const TEXT_08: Rgba = rgba_const(0xcad3f5, 0.08);
  }

  // Catppuccin Latte. See https://catppuccin.com/palette/
  pub(super) mod latte {
    use super::{Rgba, rgb_const};

    pub(in crate::themes::builtins) const ROSEWATER: Rgba = rgb_const(0xdc8a78);
    pub(in crate::themes::builtins) const FLAMINGO: Rgba = rgb_const(0xdd7878);
    pub(in crate::themes::builtins) const PINK: Rgba = rgb_const(0xea76cb);
    pub(in crate::themes::builtins) const MAUVE: Rgba = rgb_const(0x8839ef);
    pub(in crate::themes::builtins) const RED: Rgba = rgb_const(0xd20f39);
    pub(in crate::themes::builtins) const MAROON: Rgba = rgb_const(0xe64553);
    pub(in crate::themes::builtins) const PEACH: Rgba = rgb_const(0xfe640b);
    pub(in crate::themes::builtins) const YELLOW: Rgba = rgb_const(0xdf8e1d);
    pub(in crate::themes::builtins) const GREEN: Rgba = rgb_const(0x40a02b);
    pub(in crate::themes::builtins) const TEAL: Rgba = rgb_const(0x179299);
    pub(in crate::themes::builtins) const SKY: Rgba = rgb_const(0x04a5e5);
    pub(in crate::themes::builtins) const SAPPHIRE: Rgba = rgb_const(0x209fb5);
    pub(in crate::themes::builtins) const BLUE: Rgba = rgb_const(0x1e66f5);
    pub(in crate::themes::builtins) const LAVENDER: Rgba = rgb_const(0x7287fd);
    pub(in crate::themes::builtins) const TEXT: Rgba = rgb_const(0x4c4f69);
    pub(in crate::themes::builtins) const SUBTEXT1: Rgba = rgb_const(0x5c5f77);
    pub(in crate::themes::builtins) const SUBTEXT0: Rgba = rgb_const(0x6c6f85);
    pub(in crate::themes::builtins) const OVERLAY2: Rgba = rgb_const(0x7c7f93);
    pub(in crate::themes::builtins) const OVERLAY1: Rgba = rgb_const(0x8c8fa1);
    pub(in crate::themes::builtins) const OVERLAY0: Rgba = rgb_const(0x9ca0b0);
    pub(in crate::themes::builtins) const SURFACE2: Rgba = rgb_const(0xacb0be);
    pub(in crate::themes::builtins) const SURFACE1: Rgba = rgb_const(0xbcc0cc);
    pub(in crate::themes::builtins) const SURFACE0: Rgba = rgb_const(0xccd0da);
    pub(in crate::themes::builtins) const BASE: Rgba = rgb_const(0xeff1f5);
    pub(in crate::themes::builtins) const MANTLE: Rgba = rgb_const(0xe6e9ef);
    pub(in crate::themes::builtins) const CRUST: Rgba = rgb_const(0xdce0e8);
  }
}

/// Returns the default dark UI theme.
///
/// The dark theme uses Catppuccin Macchiato, mapped into Chitin's semantic UI
/// roles. Components should depend on these roles rather than palette names.
pub fn dark() -> UIThemes {
  UIThemes {
    text: UITextColors {
      primary: macchiato::TEXT,
      secondary: macchiato::SUBTEXT1,
      disabled: macchiato::OVERLAY1,
      hover: macchiato::TEXT,
      selection: macchiato::TEXT,
      highlight: macchiato::YELLOW,
      error: macchiato::RED,
      warning: macchiato::YELLOW,
      info: macchiato::BLUE,
      success: macchiato::GREEN,
    },
    background: UIBackgroundColors {
      primary: macchiato::CRUST,
      secondary: macchiato::BASE,
      hover: macchiato::SURFACE0,
      active: macchiato::SURFACE1,
      selection: macchiato::SURFACE0,
      error: macchiato::RED,
      warning: macchiato::YELLOW,
      info: macchiato::BLUE,
      success: macchiato::GREEN,
    },
    border: UIBorderColors {
      primary: macchiato::TEXT_10,
      muted: macchiato::TEXT_08,
      focus: macchiato::LAVENDER,
    },
    accent: UIAccentColors {
      primary: macchiato::BLUE,
      foreground: macchiato::CRUST,
    },
  }
}

/// Returns the default light UI theme.
///
/// The light theme uses Catppuccin Latte, mapped into the same semantic UI roles
/// as [`dark`].
pub fn light() -> UIThemes {
  UIThemes {
    text: UITextColors {
      primary: latte::TEXT,
      secondary: latte::SUBTEXT1,
      disabled: latte::OVERLAY1,
      hover: latte::TEXT,
      selection: latte::TEXT,
      highlight: latte::YELLOW,
      error: latte::RED,
      warning: latte::YELLOW,
      info: latte::BLUE,
      success: latte::GREEN,
    },
    background: UIBackgroundColors {
      primary: latte::BASE,
      secondary: latte::MANTLE,
      hover: latte::SURFACE0,
      active: latte::SURFACE1,
      selection: latte::SURFACE0,
      error: latte::RED,
      warning: latte::YELLOW,
      info: latte::BLUE,
      success: latte::GREEN,
    },
    border: UIBorderColors {
      primary: latte::SURFACE1,
      muted: latte::SURFACE0,
      focus: latte::LAVENDER,
    },
    accent: UIAccentColors {
      primary: latte::BLUE,
      foreground: latte::BASE,
    },
  }
}
