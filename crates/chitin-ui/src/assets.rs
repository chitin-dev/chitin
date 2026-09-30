//! Shared asset bundle for Chitin windows and GPUI Kit controls.

use std::{borrow::Cow, collections::BTreeSet};

use gpui::{AssetSource, Result, SharedString};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/../../assets"]
#[exclude = "fonts/**"]
struct EmbeddedAssets;

/// Chitin's icons with GPUI Kit's bundled assets as a fallback.
///
/// Application assets win name collisions. Fonts are registered separately by
/// the desktop so terminal shaping continues to use its bundled font family.
pub struct ChitinAssets;

impl AssetSource for ChitinAssets {
  fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
    match EmbeddedAssets::get(path) {
      Some(asset) => Ok(Some(asset.data)),
      None => gpui_kit::assets::Assets.load(path),
    }
  }

  fn list(&self, path: &str) -> Result<Vec<SharedString>> {
    let path = path.trim_matches('/');
    let prefix = if path.is_empty() {
      String::new()
    } else {
      format!("{path}/")
    };
    let mut children = BTreeSet::new();
    for asset in EmbeddedAssets::iter() {
      if let Some(name) = asset.strip_prefix(&prefix).and_then(|path| path.split('/').next()) {
        children.insert(name.to_owned());
      }
    }
    // Kit enumerates full asset paths; GPUI consumers expect direct child names.
    for asset in gpui_kit::assets::Assets.list(path)? {
      if let Some(name) = asset.strip_prefix(&prefix).and_then(|path| path.split('/').next()) {
        children.insert(name.to_owned());
      }
    }
    Ok(children.into_iter().map(SharedString::from).collect())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn asset_bundle_should_include_application_and_kit_icons() -> Result<()> {
    assert!(ChitinAssets.load("icons/terminal-builtin.svg")?.is_some());
    assert!(ChitinAssets.load("icons/chevron-down.svg")?.is_some());
    Ok(())
  }

  #[test]
  fn asset_listing_should_merge_icons_without_duplicates_or_directory_prefixes() -> Result<()> {
    let names = ChitinAssets.list("icons")?;
    assert!(names.iter().any(|name| name == "terminal-builtin.svg"));
    assert!(names.iter().any(|name| name == "chevron-down.svg"));
    assert!(names.iter().all(|name| !name.contains('/')));
    assert!(names.windows(2).all(|pair| pair[0] < pair[1]));
    Ok(())
  }
}
