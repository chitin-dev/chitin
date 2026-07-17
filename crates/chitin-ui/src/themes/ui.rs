use gpui::Rgba;

/// Semantic text color palette for the IDE interface.
///
/// These tokens define text appearance across different UI states and feedback levels.
/// They are designed to be used with the current theme context, not as absolute colors.
#[derive(Clone, Copy, Debug)]
pub struct UITextColors {
  /// Primary text color for main content (file names, menu items).
  pub primary: Rgba,
  /// Secondary text color for metadata, hints, path descriptions, and placeholders.
  pub secondary: Rgba,
  /// Text color for disabled or inactive UI elements (grayed out).
  pub disabled: Rgba,
  /// Text color when the mouse hovers over an interactive element.
  pub hover: Rgba,
  /// Text color for selected items in lists.
  pub selection: Rgba,
  /// Text color for transient highlights (e.g., search matches, find results).
  pub highlight: Rgba,
  /// Text color for critical errors, validation failures, or docking mistakes.
  pub error: Rgba,
  /// Text color for warnings or non-critical alerts (e.g., deprecated features).
  pub warning: Rgba,
  /// Text color for informational messages, tips, or neutral system notes.
  pub info: Rgba,
  /// Text color for success info
  pub success: Rgba,
}

/// Semantic background colors for the Chitin IDE interface.
///
/// These tokens define the visual surface hierarchy and interactive feedback states.
/// They work together with `UITextColors` to create a cohesive theme system.
#[derive(Clone, Copy, Debug)]
pub struct UIBackgroundColors {
  /// Primary background color for main content (file names, menu items).
  pub primary: Rgba,
  /// Secondary background color for metadata, hints, path descriptions, and placeholders.
  pub secondary: Rgba,
  /// Background color when the mouse hovers over an interactive element.
  pub hover: Rgba,
  /// Background color for active or pressed interface elements.
  pub active: Rgba,
  /// Background color for selected rows, tabs, or navigation items.
  pub selection: Rgba,
  /// Background color for error badges, destructive indicators, or urgent counts.
  pub error: Rgba,
  /// Background color for warning badges
  pub warning: Rgba,
  /// Background color for info badges
  pub info: Rgba,
  /// Background color for successful badges
  pub success: Rgba,
}

/// Semantic border colors for UI containers and controls.
///
/// Border tokens keep component outlines consistent across panels, list items,
/// focus states, and separators. Components should use these semantic fields
/// instead of hardcoding visual divider colors.
#[derive(Clone, Copy, Debug)]
pub struct UIBorderColors {
  /// Default border color for panels, sidebars, and controls.
  pub primary: Rgba,
  /// Muted border color for subtle separators.
  pub muted: Rgba,
  /// Focus border color for active keyboard or mouse focus.
  pub focus: Rgba,
}

/// Semantic accent colors for interactive highlights.
///
/// Accent tokens are used for selected navigation indicators, focus rings,
/// active item marks, and other UI elements that should stand out from neutral
/// surfaces.
#[derive(Clone, Copy, Debug)]
pub struct UIAccentColors {
  /// Primary accent color for selected indicators and focused controls.
  pub primary: Rgba,
  /// Foreground color that remains readable on top of the primary accent.
  pub foreground: Rgba,
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
