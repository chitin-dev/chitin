//! Semantic popup menu primitive.
//!
//! [`MenuState`] owns focus, highlighting, and selection. [`Menu`] only
//! renders that state, so callers can place it inside any popup surface while
//! retaining pointer and keyboard interaction semantics.

use gpui::{
  Context, EventEmitter, FocusHandle, IntoElement, KeyDownEvent, MouseButton, RenderOnce, SharedString, Window, div,
  prelude::*, px,
};

use crate::{
  primitive::icon::Icon,
  themes::{UIThemes, builtins},
};

/// One option displayed by a [`Menu`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuOption {
  /// Stable application-owned identifier used in selection events.
  id: SharedString,
  /// Text displayed in the menu row.
  label: SharedString,
  /// Optional asset-relative path for a leading icon.
  icon: Option<SharedString>,
  /// Whether pointer and keyboard activation are disabled.
  disabled: bool,
}

impl MenuOption {
  /// Creates an enabled menu option.
  pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
    Self {
      id: id.into(),
      label: label.into(),
      icon: None,
      disabled: false,
    }
  }

  /// Adds an optional leading icon asset path.
  pub fn icon(mut self, path: impl Into<SharedString>) -> Self {
    self.icon = Some(path.into());
    self
  }

  /// Marks this option unavailable for interaction.
  pub fn disabled(mut self, disabled: bool) -> Self {
    self.disabled = disabled;
    self
  }

  /// Returns the stable option identifier.
  pub fn id(&self) -> &str {
    &self.id
  }

  /// Returns the visible option label.
  pub fn label(&self) -> &str {
    &self.label
  }

  /// Returns the optional leading icon path.
  pub fn icon_path(&self) -> Option<&str> {
    self.icon.as_deref()
  }

  /// Reports whether this option is unavailable.
  pub fn is_disabled(&self) -> bool {
    self.disabled
  }
}

/// Semantic events emitted by [`MenuState`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuEvent {
  /// The highlighted option was activated or selected.
  SelectionChange {
    /// Identifier of the option that was activated.
    selected_id: SharedString,
  },
  /// The menu received keyboard focus.
  Focus,
  /// The menu lost keyboard focus.
  Blur,
}

/// Persistent focus, highlighting, and selection state for a popup menu.
pub struct MenuState {
  /// Options in the order in which they are rendered and navigated.
  options: Vec<MenuOption>,
  /// Option currently highlighted by keyboard navigation.
  highlighted_index: Option<usize>,
  /// Identifier of the last option selected by the user.
  selected_id: Option<SharedString>,
  /// Focus handle shared by the popup container.
  focus_handle: FocusHandle,
  /// Last focus state observed during rendering.
  focused: bool,
}

impl MenuState {
  /// Creates menu state from application-neutral options.
  pub fn new(options: impl IntoIterator<Item = MenuOption>, cx: &mut Context<Self>) -> Self {
    let options = options.into_iter().collect::<Vec<_>>();
    let highlighted_index = first_enabled(&options);
    Self {
      options,
      highlighted_index,
      selected_id: None,
      focus_handle: cx.focus_handle(),
      focused: false,
    }
  }

  /// Returns the focus handle tracked by this menu.
  pub fn focus_handle(&self) -> &FocusHandle {
    &self.focus_handle
  }

  /// Returns the menu options in visual order.
  pub fn options(&self) -> &[MenuOption] {
    &self.options
  }

  /// Returns the selected option identifier, if any.
  pub fn selected_id(&self) -> Option<&str> {
    self.selected_id.as_deref()
  }

  /// Returns the option currently highlighted for keyboard navigation.
  pub(crate) fn highlighted_index(&self) -> Option<usize> {
    self.highlighted_index
  }

  /// Replaces options while preserving a valid selection.
  ///
  /// # Parameters
  ///
  /// * `options` supplies the new visual and navigable menu rows.
  /// * `cx` refreshes the menu after its state is replaced.
  pub fn set_options(&mut self, options: impl IntoIterator<Item = MenuOption>, cx: &mut Context<Self>) {
    self.options = options.into_iter().collect();
    if self
      .selected_id
      .as_deref()
      .is_some_and(|id| self.options.iter().all(|option| option.id() != id))
    {
      self.selected_id = None;
    }
    self.highlighted_index = self
      .selected_id
      .as_deref()
      .and_then(|id| self.options.iter().position(|option| option.id() == id))
      .or_else(|| first_enabled(&self.options));
    cx.notify();
  }

  /// Synchronizes the selected option without emitting a selection event.
  ///
  /// This is intended for reflecting external application state when a menu
  /// opens; user activation should use [`Self::select`] instead.
  pub fn set_selected_id(&mut self, id: Option<impl Into<SharedString>>, cx: &mut Context<Self>) {
    let selected_id = id.map(Into::into);
    if self.selected_id == selected_id {
      // Rendering may synchronize the external selection on every frame;
      // preserve a highlight that the user moved with the arrow keys.
      return;
    }
    self.selected_id = selected_id;
    self.highlighted_index = self
      .selected_id
      .as_deref()
      .and_then(|selected| self.options.iter().position(|option| option.id() == selected))
      .or_else(|| first_enabled(&self.options));
    cx.notify();
  }

  /// Selects and emits the enabled option identified by `id`.
  ///
  /// # Returns
  ///
  /// `true` when an enabled option was found and activated.
  pub fn select(&mut self, id: impl AsRef<str>, cx: &mut Context<Self>) -> bool {
    let Some(index) = self
      .options
      .iter()
      .position(|option| option.id() == id.as_ref() && !option.is_disabled())
    else {
      return false;
    };
    self.activate_index(index, cx)
  }

  /// Handles arrow navigation and Enter/Space activation.
  ///
  /// The event is consumed by the rendered menu container, keeping keyboard
  /// behavior out of application-specific popup implementations.
  pub fn handle_key_down(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
    match event.keystroke.key.as_str() {
      "up" => self.move_highlight(-1, cx),
      "down" => self.move_highlight(1, cx),
      "enter" | "space" => {
        if let Some(index) = self.highlighted_index {
          self.activate_index(index, cx);
        }
      }
      _ => {}
    }
  }

  /// Moves the keyboard highlight to the next enabled option.
  ///
  /// Navigation wraps around the option list and skips disabled options. A
  /// state notification is emitted only when a new enabled option is found.
  ///
  /// # Parameters
  ///
  /// * `direction` is `-1` for upward navigation and `1` for downward navigation.
  /// * `cx` notifies the owner after the highlight changes.
  fn move_highlight(&mut self, direction: isize, cx: &mut Context<Self>) {
    let Some(current) = self.highlighted_index.or_else(|| first_enabled(&self.options)) else {
      return;
    };
    let mut index = current as isize;
    for _ in 0..self.options.len() {
      index = (index + direction).rem_euclid(self.options.len() as isize);
      if !self.options[index as usize].is_disabled() {
        self.highlighted_index = Some(index as usize);
        cx.notify();
        return;
      }
    }
  }

  /// Activates an enabled option, updates selection, and emits its event.
  ///
  /// # Returns
  ///
  /// `true` when `index` identifies an enabled option; otherwise `false`.
  ///
  /// # Parameters
  ///
  /// * `index` identifies the option to activate.
  /// * `cx` emits the selection event and notifies the owner.
  fn activate_index(&mut self, index: usize, cx: &mut Context<Self>) -> bool {
    let Some(option) = self.options.get(index).filter(|option| !option.is_disabled()) else {
      return false;
    };
    self.highlighted_index = Some(index);
    self.selected_id = Some(option.id.clone());
    cx.emit(MenuEvent::SelectionChange {
      selected_id: option.id.clone(),
    });
    cx.notify();
    true
  }

  /// Synchronizes focus state and emits a focus transition event when needed.
  ///
  /// # Parameters
  ///
  /// * `focused` is the latest focus state reported by the window.
  /// * `cx` emits the transition event and notifies the owner.
  pub(crate) fn sync_focus(&mut self, focused: bool, cx: &mut Context<Self>) {
    if self.focused == focused {
      return;
    }
    self.focused = focused;
    cx.emit(if focused { MenuEvent::Focus } else { MenuEvent::Blur });
    cx.notify();
  }
}

impl EventEmitter<MenuEvent> for MenuState {}

/// Popup menu content with semantic pointer and keyboard interaction.
#[derive(IntoElement)]
pub struct Menu {
  /// Persistent state that owns menu interaction semantics.
  state: gpui::Entity<MenuState>,
  /// Theme used for menu rows and text.
  theme: UIThemes,
  /// Explicit width of the popup content.
  width: gpui::Pixels,
}

impl Menu {
  /// Creates menu content backed by persistent [`MenuState`].
  pub fn new(state: gpui::Entity<MenuState>) -> Self {
    Self {
      state,
      theme: builtins::dark(),
      width: px(240.0),
    }
  }

  /// Sets the semantic theme used by the menu rows.
  pub fn theme(mut self, theme: UIThemes) -> Self {
    self.theme = theme;
    self
  }

  /// Sets the menu width.
  pub fn width(mut self, width: gpui::Pixels) -> Self {
    self.width = width;
    self
  }
}

impl RenderOnce for Menu {
  fn render(self, window: &mut Window, cx: &mut gpui::App) -> impl IntoElement {
    let focus_handle = self.state.read(cx).focus_handle().clone();
    let focused = focus_handle.is_focused(window);
    self.state.update(cx, |state, cx| state.sync_focus(focused, cx));
    let state_for_keys = self.state.clone();
    let options = self.state.read(cx).options().to_vec();
    let selected_id = self.state.read(cx).selected_id().map(str::to_owned);
    let highlighted_index = self.state.read(cx).highlighted_index();
    let theme = self.theme;
    let mut content = div()
      .flex()
      .flex_col()
      .w(self.width)
      .track_focus(&focus_handle)
      .on_key_down(move |event, _, cx| {
        state_for_keys.update(cx, |state, cx| state.handle_key_down(event, cx));
        cx.stop_propagation();
      });
    for (index, option) in options.into_iter().enumerate() {
      let state_for_click = self.state.clone();
      // Selection owns the checkmark; highlighting owns keyboard navigation.
      let selected = selected_id.as_deref() == Some(option.id());
      let highlighted = highlighted_index == Some(index);
      let disabled = option.is_disabled();
      content = content.child(
        div()
          .flex()
          .items_center()
          .justify_between()
          .w_full()
          .h(px(30.0))
          .px_2()
          .text_size(px(12.0))
          .text_color(if option.is_disabled() {
            theme.text.disabled
          } else {
            theme.text.primary
          })
          .bg(if selected || highlighted {
            theme.background.hover
          } else {
            theme.background.secondary
          })
          .when(!disabled, |item| item.hover(|style| style.bg(theme.background.hover)))
          .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            if disabled {
              return;
            }
            state_for_click.update(cx, |state, cx| {
              state.activate_index(index, cx);
            });
            cx.stop_propagation();
          })
          .child(
            div()
              .flex()
              .items_center()
              .gap_2()
              .when_some(option.icon_path(), |item, path| {
                item.child(Icon::new(path).size(px(16.0)).color(theme.text.primary))
              })
              .child(option.label.clone()),
          )
          .when(selected, |item| {
            item.child(Icon::new("icons/check.svg").size(px(14.0)).color(theme.text.primary))
          }),
      );
    }
    content
  }
}

fn first_enabled(options: &[MenuOption]) -> Option<usize> {
  options.iter().position(|option| !option.is_disabled())
}
