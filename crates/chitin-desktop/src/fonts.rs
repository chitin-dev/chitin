use std::borrow::Cow;

use gpui::{App, Result};

/// Font family used by native and built-in terminal surfaces.
///
/// Cascadia Mono is the official non-ligature variant of Cascadia Code.
pub const TERMINAL_FONT_FAMILY: &str = "Cascadia Mono";

/// Registers the bundled terminal font faces with GPUI.
///
/// # Parameters
///
/// * `cx` provides the process-wide GPUI text system.
///
/// # Returns
///
/// `Ok(())` after the regular and bold upright faces are registered.
pub fn register_terminal_fonts(cx: &mut App) -> Result<()> {
  cx.text_system().add_fonts(vec![
    Cow::Borrowed(include_bytes!("../../../assets/fonts/cascadia-mono/CascadiaMono-Regular.ttf").as_slice()),
    Cow::Borrowed(include_bytes!("../../../assets/fonts/cascadia-mono/CascadiaMono-Bold.ttf").as_slice()),
  ])
}
