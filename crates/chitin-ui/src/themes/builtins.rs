//! Built-in UI themes.
//!
//! Built-ins are the only place in `chitin-ui` where concrete color values
//! should be defined. Components consume semantic tokens from [`UIThemes`]
//! instead of hardcoding colors directly.

use super::{UIAccentColors, UIBackgroundColors, UIBorderColors, UITextColors, UIThemes};
use vscode_modern::{dark_modern, light_modern};

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

  pub(super) mod dark_modern {
    use super::{Rgba, rgb_const, rgba_const};

    pub(in crate::themes::builtins) const ACTIVITY_BAR_ACTIVE_BORDER: Rgba = rgb_const(0x0078d4);
    pub(in crate::themes::builtins) const ACTIVITY_BAR_BACKGROUND: Rgba = rgb_const(0x181818);
    pub(in crate::themes::builtins) const ACTIVITY_BAR_BORDER: Rgba = rgb_const(0x2b2b2b);
    pub(in crate::themes::builtins) const ACTIVITY_BAR_FOREGROUND: Rgba = rgb_const(0xd7d7d7);
    pub(in crate::themes::builtins) const ACTIVITY_BAR_INACTIVE_FOREGROUND: Rgba =
      rgb_const(0x868686);
    pub(in crate::themes::builtins) const ACTIVITY_BAR_BADGE_BACKGROUND: Rgba = rgb_const(0x0078d4);
    pub(in crate::themes::builtins) const ACTIVITY_BAR_BADGE_FOREGROUND: Rgba = rgb_const(0xffffff);
    pub(in crate::themes::builtins) const BADGE_BACKGROUND: Rgba = rgb_const(0x616161);
    pub(in crate::themes::builtins) const BADGE_FOREGROUND: Rgba = rgb_const(0xf8f8f8);
    pub(in crate::themes::builtins) const BUTTON_BACKGROUND: Rgba = rgb_const(0x0078d4);
    pub(in crate::themes::builtins) const BUTTON_BORDER: Rgba = rgba_const(0xffffff1a);
    pub(in crate::themes::builtins) const BUTTON_FOREGROUND: Rgba = rgb_const(0xffffff);
    pub(in crate::themes::builtins) const BUTTON_HOVER_BACKGROUND: Rgba = rgb_const(0x026ec1);
    pub(in crate::themes::builtins) const BUTTON_SECONDARY_BACKGROUND: Rgba =
      rgba_const(0x00000000);
    pub(in crate::themes::builtins) const BUTTON_SECONDARY_FOREGROUND: Rgba = rgb_const(0xcccccc);
    pub(in crate::themes::builtins) const BUTTON_SECONDARY_HOVER_BACKGROUND: Rgba =
      rgb_const(0x2b2b2b);
    pub(in crate::themes::builtins) const DESCRIPTION_FOREGROUND: Rgba = rgb_const(0x9d9d9d);
    pub(in crate::themes::builtins) const DROPDOWN_BACKGROUND: Rgba = rgb_const(0x313131);
    pub(in crate::themes::builtins) const DROPDOWN_BORDER: Rgba = rgb_const(0x3c3c3c);
    pub(in crate::themes::builtins) const DROPDOWN_FOREGROUND: Rgba = rgb_const(0xcccccc);
    pub(in crate::themes::builtins) const EDITOR_BACKGROUND: Rgba = rgb_const(0x1f1f1f);
    pub(in crate::themes::builtins) const EDITOR_FIND_MATCH_BACKGROUND: Rgba = rgb_const(0x9e6a03);
    pub(in crate::themes::builtins) const EDITOR_FOREGROUND: Rgba = rgb_const(0xcccccc);
    pub(in crate::themes::builtins) const EDITOR_GROUP_BORDER: Rgba = rgba_const(0xffffff17);
    pub(in crate::themes::builtins) const EDITOR_GUTTER_ADDED_BACKGROUND: Rgba =
      rgb_const(0x2ea043);
    pub(in crate::themes::builtins) const EDITOR_GUTTER_DELETED_BACKGROUND: Rgba =
      rgb_const(0xf85149);
    pub(in crate::themes::builtins) const EDITOR_GUTTER_MODIFIED_BACKGROUND: Rgba =
      rgb_const(0x0078d4);
    pub(in crate::themes::builtins) const ERROR_FOREGROUND: Rgba = rgb_const(0xf85149);
    pub(in crate::themes::builtins) const FOCUS_BORDER: Rgba = rgb_const(0x0078d4);
    pub(in crate::themes::builtins) const FOREGROUND: Rgba = rgb_const(0xcccccc);
    pub(in crate::themes::builtins) const ICON_FOREGROUND: Rgba = rgb_const(0xcccccc);
    pub(in crate::themes::builtins) const INPUT_BACKGROUND: Rgba = rgb_const(0x313131);
    pub(in crate::themes::builtins) const INPUT_BORDER: Rgba = rgb_const(0x3c3c3c);
    pub(in crate::themes::builtins) const INPUT_FOREGROUND: Rgba = rgb_const(0xcccccc);
    pub(in crate::themes::builtins) const INPUT_PLACEHOLDER_FOREGROUND: Rgba = rgb_const(0x989898);
    pub(in crate::themes::builtins) const PANEL_BACKGROUND: Rgba = rgb_const(0x181818);
    pub(in crate::themes::builtins) const PANEL_BORDER: Rgba = rgb_const(0x2b2b2b);
    pub(in crate::themes::builtins) const PANEL_TITLE_ACTIVE_FOREGROUND: Rgba = rgb_const(0xcccccc);
    pub(in crate::themes::builtins) const PANEL_TITLE_INACTIVE_FOREGROUND: Rgba =
      rgb_const(0x9d9d9d);
    pub(in crate::themes::builtins) const SIDE_BAR_BACKGROUND: Rgba = rgb_const(0x181818);
    pub(in crate::themes::builtins) const SIDE_BAR_BORDER: Rgba = rgb_const(0x2b2b2b);
    pub(in crate::themes::builtins) const SIDE_BAR_FOREGROUND: Rgba = rgb_const(0xcccccc);
    pub(in crate::themes::builtins) const STATUS_BAR_BACKGROUND: Rgba = rgb_const(0x181818);
    pub(in crate::themes::builtins) const STATUS_BAR_BORDER: Rgba = rgb_const(0x2b2b2b);
    pub(in crate::themes::builtins) const STATUS_BAR_FOREGROUND: Rgba = rgb_const(0xcccccc);
    pub(in crate::themes::builtins) const STATUS_BAR_ITEM_HOVER_BACKGROUND: Rgba =
      rgba_const(0xf1f1f133);
    pub(in crate::themes::builtins) const TAB_ACTIVE_BACKGROUND: Rgba = rgb_const(0x1f1f1f);
    pub(in crate::themes::builtins) const TAB_ACTIVE_FOREGROUND: Rgba = rgb_const(0xffffff);
    pub(in crate::themes::builtins) const TAB_INACTIVE_BACKGROUND: Rgba = rgb_const(0x181818);
    pub(in crate::themes::builtins) const TAB_INACTIVE_FOREGROUND: Rgba = rgb_const(0x9d9d9d);
    pub(in crate::themes::builtins) const TITLE_BAR_ACTIVE_BACKGROUND: Rgba = rgb_const(0x181818);
    pub(in crate::themes::builtins) const TITLE_BAR_ACTIVE_FOREGROUND: Rgba = rgb_const(0xcccccc);
    pub(in crate::themes::builtins) const TITLE_BAR_BORDER: Rgba = rgb_const(0x2b2b2b);
    pub(in crate::themes::builtins) const TITLE_BAR_INACTIVE_BACKGROUND: Rgba = rgb_const(0x1f1f1f);
    pub(in crate::themes::builtins) const TITLE_BAR_INACTIVE_FOREGROUND: Rgba = rgb_const(0x9d9d9d);
  }

  pub(super) mod light_modern {
    use super::{Rgba, rgb_const, rgba_const};

    pub(in crate::themes::builtins) const ACTIVITY_BAR_ACTIVE_BORDER: Rgba = rgb_const(0x005fb8);
    pub(in crate::themes::builtins) const ACTIVITY_BAR_BACKGROUND: Rgba = rgb_const(0xf8f8f8);
    pub(in crate::themes::builtins) const ACTIVITY_BAR_BORDER: Rgba = rgb_const(0xe5e5e5);
    pub(in crate::themes::builtins) const ACTIVITY_BAR_FOREGROUND: Rgba = rgb_const(0x1f1f1f);
    pub(in crate::themes::builtins) const ACTIVITY_BAR_INACTIVE_FOREGROUND: Rgba =
      rgb_const(0x616161);
    pub(in crate::themes::builtins) const ACTIVITY_BAR_BADGE_BACKGROUND: Rgba = rgb_const(0x005fb8);
    pub(in crate::themes::builtins) const ACTIVITY_BAR_BADGE_FOREGROUND: Rgba = rgb_const(0xffffff);
    pub(in crate::themes::builtins) const BADGE_BACKGROUND: Rgba = rgb_const(0xcccccc);
    pub(in crate::themes::builtins) const BADGE_FOREGROUND: Rgba = rgb_const(0x3b3b3b);
    pub(in crate::themes::builtins) const BUTTON_BACKGROUND: Rgba = rgb_const(0x005fb8);
    pub(in crate::themes::builtins) const BUTTON_BORDER: Rgba = rgba_const(0x0000001a);
    pub(in crate::themes::builtins) const BUTTON_FOREGROUND: Rgba = rgb_const(0xffffff);
    pub(in crate::themes::builtins) const BUTTON_HOVER_BACKGROUND: Rgba = rgb_const(0x0258a8);
    pub(in crate::themes::builtins) const BUTTON_SECONDARY_BACKGROUND: Rgba = rgb_const(0xe5e5e5);
    pub(in crate::themes::builtins) const BUTTON_SECONDARY_FOREGROUND: Rgba = rgb_const(0x3b3b3b);
    pub(in crate::themes::builtins) const BUTTON_SECONDARY_HOVER_BACKGROUND: Rgba =
      rgb_const(0xcccccc);
    pub(in crate::themes::builtins) const DESCRIPTION_FOREGROUND: Rgba = rgb_const(0x3b3b3b);
    pub(in crate::themes::builtins) const DROPDOWN_BACKGROUND: Rgba = rgb_const(0xffffff);
    pub(in crate::themes::builtins) const DROPDOWN_BORDER: Rgba = rgb_const(0xcecece);
    pub(in crate::themes::builtins) const DROPDOWN_FOREGROUND: Rgba = rgb_const(0x3b3b3b);
    pub(in crate::themes::builtins) const EDITOR_BACKGROUND: Rgba = rgb_const(0xffffff);
    pub(in crate::themes::builtins) const EDITOR_FOREGROUND: Rgba = rgb_const(0x3b3b3b);
    pub(in crate::themes::builtins) const EDITOR_INACTIVE_SELECTION_BACKGROUND: Rgba =
      rgb_const(0xe5ebf1);
    pub(in crate::themes::builtins) const EDITOR_SELECTION_HIGHLIGHT_BACKGROUND: Rgba =
      rgba_const(0xadd6ff80);
    pub(in crate::themes::builtins) const EDITOR_GROUP_BORDER: Rgba = rgb_const(0xe5e5e5);
    pub(in crate::themes::builtins) const EDITOR_GUTTER_ADDED_BACKGROUND: Rgba =
      rgb_const(0x2ea043);
    pub(in crate::themes::builtins) const EDITOR_GUTTER_DELETED_BACKGROUND: Rgba =
      rgb_const(0xf85149);
    pub(in crate::themes::builtins) const EDITOR_GUTTER_MODIFIED_BACKGROUND: Rgba =
      rgb_const(0x005fb8);
    pub(in crate::themes::builtins) const ERROR_FOREGROUND: Rgba = rgb_const(0xf85149);
    pub(in crate::themes::builtins) const FOCUS_BORDER: Rgba = rgb_const(0x005fb8);
    pub(in crate::themes::builtins) const FOREGROUND: Rgba = rgb_const(0x3b3b3b);
    pub(in crate::themes::builtins) const ICON_FOREGROUND: Rgba = rgb_const(0x3b3b3b);
    pub(in crate::themes::builtins) const INPUT_BACKGROUND: Rgba = rgb_const(0xffffff);
    pub(in crate::themes::builtins) const INPUT_BORDER: Rgba = rgb_const(0xcecece);
    pub(in crate::themes::builtins) const INPUT_FOREGROUND: Rgba = rgb_const(0x3b3b3b);
    pub(in crate::themes::builtins) const INPUT_PLACEHOLDER_FOREGROUND: Rgba = rgb_const(0x767676);
    pub(in crate::themes::builtins) const LIST_HOVER_BACKGROUND: Rgba = rgb_const(0xf2f2f2);
    pub(in crate::themes::builtins) const LIST_ACTIVE_SELECTION_BACKGROUND: Rgba =
      rgb_const(0xe8e8e8);
    pub(in crate::themes::builtins) const PANEL_BACKGROUND: Rgba = rgb_const(0xf8f8f8);
    pub(in crate::themes::builtins) const PANEL_BORDER: Rgba = rgb_const(0xe5e5e5);
    pub(in crate::themes::builtins) const PANEL_TITLE_ACTIVE_FOREGROUND: Rgba = rgb_const(0x3b3b3b);
    pub(in crate::themes::builtins) const PANEL_TITLE_INACTIVE_FOREGROUND: Rgba =
      rgb_const(0x3b3b3b);
    pub(in crate::themes::builtins) const SIDE_BAR_BACKGROUND: Rgba = rgb_const(0xf8f8f8);
    pub(in crate::themes::builtins) const SIDE_BAR_BORDER: Rgba = rgb_const(0xe5e5e5);
    pub(in crate::themes::builtins) const SIDE_BAR_FOREGROUND: Rgba = rgb_const(0x3b3b3b);
    pub(in crate::themes::builtins) const STATUS_BAR_BACKGROUND: Rgba = rgb_const(0xf8f8f8);
    pub(in crate::themes::builtins) const STATUS_BAR_BORDER: Rgba = rgb_const(0xe5e5e5);
    pub(in crate::themes::builtins) const STATUS_BAR_FOREGROUND: Rgba = rgb_const(0x3b3b3b);
    pub(in crate::themes::builtins) const STATUS_BAR_ITEM_HOVER_BACKGROUND: Rgba =
      rgba_const(0x1f1f1f11);
    pub(in crate::themes::builtins) const TAB_ACTIVE_BACKGROUND: Rgba = rgb_const(0xffffff);
    pub(in crate::themes::builtins) const TAB_ACTIVE_FOREGROUND: Rgba = rgb_const(0x3b3b3b);
    pub(in crate::themes::builtins) const TAB_INACTIVE_BACKGROUND: Rgba = rgb_const(0xf8f8f8);
    pub(in crate::themes::builtins) const TAB_INACTIVE_FOREGROUND: Rgba = rgb_const(0x868686);
    pub(in crate::themes::builtins) const TITLE_BAR_ACTIVE_BACKGROUND: Rgba = rgb_const(0xf8f8f8);
    pub(in crate::themes::builtins) const TITLE_BAR_ACTIVE_FOREGROUND: Rgba = rgb_const(0x1e1e1e);
    pub(in crate::themes::builtins) const TITLE_BAR_BORDER: Rgba = rgb_const(0xe5e5e5);
    pub(in crate::themes::builtins) const TITLE_BAR_INACTIVE_BACKGROUND: Rgba = rgb_const(0xf8f8f8);
    pub(in crate::themes::builtins) const TITLE_BAR_INACTIVE_FOREGROUND: Rgba = rgb_const(0x8b949e);
  }
}

/// Returns the default dark UI theme.
///
/// The dark theme uses Visual Studio Code's Dark Modern colors, mapped into
/// Chitin's semantic UI roles.
pub fn dark() -> UIThemes {
  UIThemes {
    text: UITextColors {
      primary: dark_modern::FOREGROUND,
      secondary: dark_modern::DESCRIPTION_FOREGROUND,
      disabled: dark_modern::ACTIVITY_BAR_INACTIVE_FOREGROUND,
      hover: dark_modern::ACTIVITY_BAR_FOREGROUND,
      selection: dark_modern::TAB_ACTIVE_FOREGROUND,
      highlight: dark_modern::EDITOR_FIND_MATCH_BACKGROUND,
      error: dark_modern::ERROR_FOREGROUND,
      warning: rgb_const(0xe2c08d),
      info: dark_modern::FOCUS_BORDER,
      success: dark_modern::EDITOR_GUTTER_ADDED_BACKGROUND,
    },
    background: UIBackgroundColors {
      primary: dark_modern::ACTIVITY_BAR_BACKGROUND,
      secondary: dark_modern::EDITOR_BACKGROUND,
      hover: dark_modern::BUTTON_SECONDARY_HOVER_BACKGROUND,
      active: dark_modern::TAB_ACTIVE_BACKGROUND,
      selection: dark_modern::BUTTON_SECONDARY_HOVER_BACKGROUND,
      error: dark_modern::EDITOR_GUTTER_DELETED_BACKGROUND,
      warning: rgb_const(0xe2c08d),
      info: dark_modern::ACTIVITY_BAR_BADGE_BACKGROUND,
      success: dark_modern::EDITOR_GUTTER_ADDED_BACKGROUND,
    },
    border: UIBorderColors {
      primary: dark_modern::ACTIVITY_BAR_BORDER,
      muted: dark_modern::EDITOR_GROUP_BORDER,
      focus: dark_modern::FOCUS_BORDER,
    },
    accent: UIAccentColors {
      primary: dark_modern::ACTIVITY_BAR_ACTIVE_BORDER,
      foreground: dark_modern::ACTIVITY_BAR_BADGE_FOREGROUND,
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
      primary: light_modern::FOREGROUND,
      secondary: light_modern::DESCRIPTION_FOREGROUND,
      disabled: light_modern::ACTIVITY_BAR_INACTIVE_FOREGROUND,
      hover: light_modern::ACTIVITY_BAR_FOREGROUND,
      selection: light_modern::TAB_ACTIVE_FOREGROUND,
      highlight: rgb_const(0x895503),
      error: light_modern::ERROR_FOREGROUND,
      warning: rgb_const(0x895503),
      info: light_modern::FOCUS_BORDER,
      success: light_modern::EDITOR_GUTTER_ADDED_BACKGROUND,
    },
    background: UIBackgroundColors {
      primary: light_modern::ACTIVITY_BAR_BACKGROUND,
      secondary: light_modern::EDITOR_BACKGROUND,
      hover: light_modern::LIST_HOVER_BACKGROUND,
      active: light_modern::LIST_ACTIVE_SELECTION_BACKGROUND,
      selection: light_modern::LIST_ACTIVE_SELECTION_BACKGROUND,
      error: light_modern::EDITOR_GUTTER_DELETED_BACKGROUND,
      warning: rgb_const(0x895503),
      info: light_modern::ACTIVITY_BAR_BADGE_BACKGROUND,
      success: light_modern::EDITOR_GUTTER_ADDED_BACKGROUND,
    },
    border: UIBorderColors {
      primary: light_modern::ACTIVITY_BAR_BORDER,
      muted: light_modern::EDITOR_GROUP_BORDER,
      focus: light_modern::FOCUS_BORDER,
    },
    accent: UIAccentColors {
      primary: light_modern::ACTIVITY_BAR_ACTIVE_BORDER,
      foreground: light_modern::ACTIVITY_BAR_BADGE_FOREGROUND,
    },
  }
}
