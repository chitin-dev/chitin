use gpui::Hsla;

/// Semantic text color palette for the IDE interface.
///
/// These tokens define text appearance across different UI states and feedback levels.
/// They are designed to be used with the current theme context, not as absolute colors.
#[derive(Clone, Copy, Debug)]
pub struct UITextColors {
  /// Primary text color for main content (file names, menu items).
  pub primary: Hsla,
  /// Secondary text color for metadata, hints, path descriptions, and placeholders.
  pub secondary: Hsla,
  /// Text color for disabled or inactive UI elements (grayed out).
  pub disabled: Hsla,
  /// Text color when the mouse hovers over an interactive element.
  pub hover: Hsla,
  /// Text color for selected items in lists.
  pub selection: Hsla,
  /// Text color for transient highlights (e.g., search matches, find results).
  pub highlight: Hsla,
  /// Text color for critical errors, validation failures, or docking mistakes.
  pub error: Hsla,
  /// Text color for warnings or non-critical alerts (e.g., deprecated features).
  pub warning: Hsla,
  /// Text color for informational messages, tips, or neutral system notes.
  pub info: Hsla,
  /// Text color for success info
  pub success: Hsla,
}

/// Semantic background colors for the Chitin IDE interface.
///
/// These tokens define the visual surface hierarchy and interactive feedback states.
/// They work together with `UITextColors` to create a cohesive theme system.
#[derive(Clone, Copy, Debug)]
pub struct UIBackgroundColors {
  /// Primary background color for main content (file names, menu items).
  pub primary: Hsla,
  /// Secondary background color for metadata, hints, path descriptions, and placeholders.
  pub secondary: Hsla,
  /// Background color when the mouse hovers over an interactive element.
  pub hover: Hsla,
  /// Background color for active or pressed interface elements.
  pub active: Hsla,
  /// Background color for selected rows, tabs, or navigation items.
  pub selection: Hsla,
  /// Background color for badges, destructive indicators, or urgent counts.
  pub danger: Hsla,
}

/// Semantic border colors for UI containers and controls.
///
/// Border tokens keep component outlines consistent across panels, list items,
/// focus states, and separators. Components should use these semantic fields
/// instead of hardcoding visual divider colors.
#[derive(Clone, Copy, Debug)]
pub struct UIBorderColors {
  /// Default border color for panels, sidebars, and controls.
  pub primary: Hsla,
  /// Muted border color for subtle separators.
  pub muted: Hsla,
  /// Focus border color for active keyboard or mouse focus.
  pub focus: Hsla,
}

/// Semantic accent colors for interactive highlights.
///
/// Accent tokens are used for selected navigation indicators, focus rings,
/// active item marks, and other UI elements that should stand out from neutral
/// surfaces.
#[derive(Clone, Copy, Debug)]
pub struct UIAccentColors {
  /// Primary accent color for selected indicators and focused controls.
  pub primary: Hsla,
  /// Foreground color that remains readable on top of the primary accent.
  pub foreground: Hsla,
}

/// This is the root UI theme structure used throughout the Chitin IDE.
/// It provides a consistent color system across all UI components.
#[derive(Clone, Copy, Debug)]
pub struct UIThemes {
  /// Text color tokens for all text-based UI elements.
  pub text: UITextColors,
  /// Background color tokens for all surface and interactive elements.
  pub background: UIBackgroundColors,
  /// Border color tokens for outlines and separators.
  pub border: UIBorderColors,
  /// Accent color tokens for active indicators and high-emphasis affordances.
  pub accent: UIAccentColors,
}
