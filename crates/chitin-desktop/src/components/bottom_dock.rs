//! Desktop-owned controls for the reusable workbench bottom dock.

use chitin_ui::primitive::button::{ButtonEvent, ButtonState};
use gpui::{AppContext, Context, Entity, Subscription, Window};

use crate::app::ChitinApp;

/// Persistent controls shared by every bottom-dock item.
#[derive(Clone)]
pub(crate) struct BottomDockControls {
  close: Entity<ButtonState>,
}

impl BottomDockControls {
  /// Creates and subscribes the generic dock controls.
  fn new(window: &mut Window, cx: &mut Context<ChitinApp>) -> Self {
    let close = cx.new(ButtonState::new);
    let subscription: Subscription = cx.subscribe_in(&close, window, |this, _, event, _, cx| {
      if matches!(event, ButtonEvent::Click) {
        this.close_bottom_dock(cx);
      }
    });
    subscription.detach();
    Self { close }
  }

  /// Returns the shared dock close-button state.
  pub(crate) fn close(&self) -> Entity<ButtonState> {
    self.close.clone()
  }
}

impl ChitinApp {
  /// Returns generic bottom-dock controls, creating them on first use.
  pub(crate) fn bottom_dock_controls(&mut self, window: &mut Window, cx: &mut Context<Self>) -> BottomDockControls {
    if let Some(controls) = self.bottom_dock_controls.as_ref() {
      return controls.clone();
    }
    let controls = BottomDockControls::new(window, cx);
    self.bottom_dock_controls = Some(controls.clone());
    controls
  }

  /// Closes whichever tool is active in the bottom dock.
  pub(crate) fn close_bottom_dock(&mut self, cx: &mut Context<Self>) {
    self.bottom_dock.close();
    self.terminal_panel.request_focus(false);
    cx.notify();
  }
}
