//! Built-in UI themes.
//!
//! Built-ins are the only place in `chitin-ui` where concrete color values
//! should be defined. Components consume semantic tokens from [`UIThemes`]
//! instead of hardcoding colors directly.

use super::{UIAccentColors, UIBackgroundColors, UIBorderColors, UITextColors, UIThemes};
use vscode_modern::{dark, light};

const fn rgb_const(hex: u32) -> gpui::Rgba {
  gpui::Rgba {
    r: ((hex >> 16) & 0xff) as f32 / 255.0,
    g: ((hex >> 8) & 0xff) as f32 / 255.0,
    b: (hex & 0xff) as f32 / 255.0,
    a: 1.0,
  }
}

const fn rgba_const(hex: u32) -> gpui::Rgba {
  gpui::Rgba {
    r: ((hex >> 24) & 0xff) as f32 / 255.0,
    g: ((hex >> 16) & 0xff) as f32 / 255.0,
    b: ((hex >> 8) & 0xff) as f32 / 255.0,
    a: (hex & 0xff) as f32 / 255.0,
  }
}

#[allow(dead_code)]
mod vscode_modern {
  use gpui::Rgba;

  use super::{rgb_const, rgba_const};

  pub(super) mod dark {
    use super::{Rgba, rgb_const, rgba_const};

    pub(in crate::themes::builtins) const BLUE: Rgba = rgb_const(0x0078d4);
    pub(in crate::themes::builtins) const BLUE_TRANSPARENT: Rgba = rgba_const(0x0078d455);
    pub(in crate::themes::builtins) const BLUE_HARD: Rgba = rgb_const(0x026ec1);
    pub(in crate::themes::builtins) const BLUE_LIGHT: Rgba = rgb_const(0x4daafc);
    pub(in crate::themes::builtins) const GREEN: Rgba = rgb_const(0x2ea043);
    pub(in crate::themes::builtins) const RED: Rgba = rgb_const(0xf85149);
    pub(in crate::themes::builtins) const YELLOW: Rgba = rgb_const(0xe2c08d);
    pub(in crate::themes::builtins) const YELLOW_DARK: Rgba = rgb_const(0x9e6a03);

    pub(in crate::themes::builtins) const WHITE: Rgba = rgb_const(0xffffff);
    pub(in crate::themes::builtins) const WHITE_SOFT: Rgba = rgb_const(0xf8f8f8);
    pub(in crate::themes::builtins) const WHITE_09: Rgba = rgba_const(0xffffff17);
    pub(in crate::themes::builtins) const WHITE_10: Rgba = rgba_const(0xffffff1a);
    pub(in crate::themes::builtins) const WHITE_20: Rgba = rgba_const(0xf1f1f133);

    pub(in crate::themes::builtins) const GRAY_D7: Rgba = rgb_const(0xd7d7d7);
    pub(in crate::themes::builtins) const GRAY_CC: Rgba = rgb_const(0xcccccc);
    pub(in crate::themes::builtins) const GRAY_9D: Rgba = rgb_const(0x9d9d9d);
    pub(in crate::themes::builtins) const GRAY_98: Rgba = rgb_const(0x989898);
    pub(in crate::themes::builtins) const GRAY_86: Rgba = rgb_const(0x868686);
    pub(in crate::themes::builtins) const GRAY_61: Rgba = rgb_const(0x616161);
    pub(in crate::themes::builtins) const GRAY_3C: Rgba = rgb_const(0x3c3c3c);
    pub(in crate::themes::builtins) const GRAY_31: Rgba = rgb_const(0x313131);
    pub(in crate::themes::builtins) const GRAY_2B: Rgba = rgb_const(0x2b2b2b);
    pub(in crate::themes::builtins) const GRAY_22: Rgba = rgb_const(0x222222);
    pub(in crate::themes::builtins) const GRAY_20: Rgba = rgb_const(0x202020);
    pub(in crate::themes::builtins) const GRAY_1F: Rgba = rgb_const(0x1f1f1f);
    pub(in crate::themes::builtins) const GRAY_18: Rgba = rgb_const(0x181818);
    pub(in crate::themes::builtins) const BLACK_TRANSPARENT: Rgba = rgba_const(0x00000000);
  }

  pub(super) mod light {
    use super::{Rgba, rgb_const, rgba_const};

    pub(in crate::themes::builtins) const BLUE: Rgba = rgb_const(0x005fb8);
    pub(in crate::themes::builtins) const BLUE_TRANSPARENT: Rgba = rgba_const(0x005fb8cc);
    pub(in crate::themes::builtins) const BLUE_HARD: Rgba = rgb_const(0x0258a8);
    pub(in crate::themes::builtins) const BLUE_LIGHT: Rgba = rgb_const(0x68a3da);
    pub(in crate::themes::builtins) const GREEN: Rgba = rgb_const(0x2ea043);
    pub(in crate::themes::builtins) const RED: Rgba = rgb_const(0xf85149);
    pub(in crate::themes::builtins) const YELLOW: Rgba = rgb_const(0x895503);
    pub(in crate::themes::builtins) const YELLOW_ALPHA: Rgba = rgba_const(0xbb800966);

    pub(in crate::themes::builtins) const WHITE: Rgba = rgb_const(0xffffff);
    pub(in crate::themes::builtins) const BLACK: Rgba = rgb_const(0x000000);
    pub(in crate::themes::builtins) const BLACK_07: Rgba = rgba_const(0x1f1f1f11);
    pub(in crate::themes::builtins) const BLACK_12: Rgba = rgba_const(0x0000001f);
    pub(in crate::themes::builtins) const BLACK_10: Rgba = rgba_const(0x0000001a);

    pub(in crate::themes::builtins) const GRAY_F8: Rgba = rgb_const(0xf8f8f8);
    pub(in crate::themes::builtins) const GRAY_F3: Rgba = rgb_const(0xf3f3f3);
    pub(in crate::themes::builtins) const GRAY_F2: Rgba = rgb_const(0xf2f2f2);
    pub(in crate::themes::builtins) const GRAY_E8: Rgba = rgb_const(0xe8e8e8);
    pub(in crate::themes::builtins) const GRAY_E5: Rgba = rgb_const(0xe5e5e5);
    pub(in crate::themes::builtins) const GRAY_CE: Rgba = rgb_const(0xcecece);
    pub(in crate::themes::builtins) const GRAY_CC: Rgba = rgb_const(0xcccccc);
    pub(in crate::themes::builtins) const GRAY_8B: Rgba = rgb_const(0x8b949e);
    pub(in crate::themes::builtins) const GRAY_86: Rgba = rgb_const(0x868686);
    pub(in crate::themes::builtins) const GRAY_76: Rgba = rgb_const(0x767676);
    pub(in crate::themes::builtins) const GRAY_61: Rgba = rgb_const(0x616161);
    pub(in crate::themes::builtins) const GRAY_3B: Rgba = rgb_const(0x3b3b3b);
    pub(in crate::themes::builtins) const GRAY_1F: Rgba = rgb_const(0x1f1f1f);
    pub(in crate::themes::builtins) const GRAY_1E: Rgba = rgb_const(0x1e1e1e);
  }
}

/// Returns the default dark UI theme.
///
/// The dark theme uses Visual Studio Code's Dark Modern colors, mapped into
/// Chitin's semantic UI roles.
pub fn dark() -> UIThemes {
  UIThemes {
    text: UITextColors {
      primary: dark::GRAY_CC,
      secondary: dark::GRAY_9D,
      disabled: dark::GRAY_86,
      hover: dark::GRAY_D7,
      selection: dark::WHITE,
      highlight: dark::YELLOW_DARK,
      error: dark::RED,
      warning: dark::YELLOW,
      info: dark::BLUE,
      success: dark::GREEN,
    },
    background: UIBackgroundColors {
      primary: dark::GRAY_18,
      secondary: dark::GRAY_1F,
      hover: dark::GRAY_2B,
      active: dark::GRAY_1F,
      selection: dark::BLUE_TRANSPARENT,
      error: dark::RED,
      warning: dark::YELLOW,
      info: dark::BLUE,
      success: dark::GREEN,
    },
    border: UIBorderColors {
      primary: dark::GRAY_2B,
      muted: dark::WHITE_09,
      focus: dark::BLUE_HARD,
    },
    accent: UIAccentColors {
      primary: dark::BLUE,
      foreground: dark::WHITE,
    },
  }
}

/// Returns the default light UI theme.
///
/// The light theme uses Visual Studio Code's Light Modern colors, mapped into
/// the same semantic UI roles as [`dark`].
pub fn light() -> UIThemes {
  UIThemes {
    text: UITextColors {
      primary: light::GRAY_3B,
      secondary: light::GRAY_3B,
      disabled: light::GRAY_61,
      hover: light::GRAY_1F,
      selection: light::GRAY_3B,
      highlight: light::YELLOW,
      error: light::RED,
      warning: light::YELLOW,
      info: light::BLUE,
      success: light::GREEN,
    },
    background: UIBackgroundColors {
      primary: light::GRAY_F8,
      secondary: light::WHITE,
      hover: light::GRAY_F2,
      active: light::GRAY_E8,
      selection: light::BLUE,
      error: light::RED,
      warning: light::YELLOW,
      info: light::BLUE,
      success: light::GREEN,
    },
    border: UIBorderColors {
      primary: light::GRAY_E5,
      muted: light::GRAY_E5,
      focus: light::BLUE_HARD,
    },
    accent: UIAccentColors {
      primary: light::BLUE,
      foreground: light::WHITE,
    },
  }
}
