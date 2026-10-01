//! Host-resolved rendering targets for one built-in shell session.

/// Session navigation requests requiring the host's open rendering views.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderingPanelCommand {
  /// List all open molecular rendering views.
  List,
  /// Enter the view with this stable rendering identifier.
  Enter {
    /// Identifier shown by `panel list`, independent of visible position.
    id: u64,
  },
  /// Leave the selected rendering view without closing it.
  Leave,
}

/// Render-neutral descriptor supplied by the host, with no UI entity handles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderingPanel {
  id: u64,
  title: String,
}

impl RenderingPanel {
  /// Creates a descriptor for a live rendering view.
  pub fn new(id: u64, title: impl Into<String>) -> Self {
    Self {
      id,
      title: title.into(),
    }
  }

  /// Returns the stable rendering identifier.
  pub const fn id(&self) -> u64 {
    self.id
  }

  /// Returns the current document title.
  pub fn title(&self) -> &str {
    &self.title
  }
}
