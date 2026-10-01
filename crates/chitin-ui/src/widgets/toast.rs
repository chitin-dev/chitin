//! Chitin notification cards composed with GPUI Kit's toast infrastructure.
//!
//! Kit owns hover/focus expansion, stack motion, expiration, and independent
//! exit deadlines. Chitin owns card content, semantic events, and the three-item
//! capacity policy.

use std::time::{Duration, Instant};

use gpui_kit::component::theme::ThemeColor;

use gpui::{
  Animation, AnimationExt, App, AsyncApp, Context, Entity, EventEmitter, FocusHandle, IntoElement, ParentElement,
  Render, RenderOnce, SharedString, Timer, WeakEntity, Window, div, prelude::*, px,
};
use gpui_kit::{
  base::{
    Anchor, Toast as BaseToast, ToastManager, ToastMotion, ToastOptions, ToastStack, ToastStackState,
    ToastTransitionStatus,
  },
  component::{
    Icon, Sizable as _,
    button::{Button, ButtonVariant, ButtonVariants as _},
  },
};

/// Maximum number of active notifications retained by one viewport.
pub const MAX_VISIBLE_TOASTS: usize = 3;
/// Default active lifetime after the entry transition.
pub const DEFAULT_TOAST_DURATION: Duration = Duration::from_secs(5);

const TOAST_WIDTH: gpui::Pixels = px(380.0);
const TOAST_HEIGHT: gpui::Pixels = px(82.0);
const TOAST_CORNER_RADIUS: gpui::Pixels = px(8.0);
const TOAST_ACCENT_WIDTH: gpui::Pixels = px(3.0);
const TOAST_ACCENT_CLIP_WIDTH: gpui::Pixels = px(6.0);
const VIEWPORT_MARGIN: gpui::Pixels = px(16.0);
const LIFECYCLE_INTERVAL: Duration = Duration::from_millis(16);

/// Stable identity assigned to one notification in a viewport.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ToastId(u64);

/// Semantic emphasis applied to a notification.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToastVariant {
  /// Neutral application feedback.
  #[default]
  Default,
  /// Successful completion feedback.
  Success,
  /// Informational feedback.
  Info,
  /// Recoverable warning feedback.
  Warning,
  /// Failed operation feedback.
  Error,
}

/// Declarative content pushed into a [`ToastViewport`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Toast {
  /// Primary, always-visible notification text.
  title: SharedString,
  /// Optional secondary explanation rendered below [`Self::title`].
  description: Option<SharedString>,
  /// Optional label for an action button that emits [`ToastEvent::Action`].
  action_label: Option<SharedString>,
  /// Semantic style used to choose the card's accent color.
  variant: ToastVariant,
  /// Time after which the notification expires, or `None` for a persistent toast.
  duration: Option<Duration>,
}

impl Toast {
  /// Creates a neutral notification with a short title.
  pub fn new(title: impl Into<SharedString>) -> Self {
    Self {
      title: title.into(),
      description: None,
      action_label: None,
      variant: ToastVariant::Default,
      duration: Some(DEFAULT_TOAST_DURATION),
    }
  }

  /// Sets the secondary notification text.
  ///
  /// The description is stored as shared text and is rendered only when it is
  /// present. Calling this method replaces any description previously set on
  /// the builder value.
  pub fn description(mut self, description: impl Into<SharedString>) -> Self {
    self.description = Some(description.into());
    self
  }

  /// Adds a compact semantic action to the notification.
  ///
  /// The action callback is emitted by [`ToastViewport`] when the button is
  /// clicked. The viewport dismisses the toast after emitting the event, so
  /// consumers can react to the action without having to perform cleanup.
  pub fn action(mut self, label: impl Into<SharedString>) -> Self {
    self.action_label = Some(label.into());
    self
  }

  /// Sets the semantic notification emphasis.
  ///
  /// The variant affects presentation only; it does not change expiry or
  /// dismissal behavior.
  pub fn variant(mut self, variant: ToastVariant) -> Self {
    self.variant = variant;
    self
  }

  /// Sets the dismissal timeout, or disables automatic dismissal with `None`.
  ///
  /// The lifetime counts active time after entry. GPUI Kit pauses this
  /// countdown while the stack is hovered or focused.
  pub fn duration(mut self, duration: Option<Duration>) -> Self {
    self.duration = duration;
    self
  }
}

/// Semantic events emitted by [`ToastViewport`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastEvent {
  /// The optional action was activated before the notification closed.
  Action(ToastId),
  /// The notification was closed manually, expired, or displaced by capacity.
  Dismissed(ToastId),
}

/// Presentation and capacity policy over GPUI Kit's lifecycle manager.
pub struct ToastViewport {
  manager: ToastManager<ToastId, Toast>,
  stack: ToastStackState,
  next_id: u64,
  advancing: bool,
  focus: Option<FocusHandle>,
  theme: ThemeColor,
}

/// Positions a notification viewport at the window's bottom-right edge.
#[derive(IntoElement)]
pub struct ToastHost {
  viewport: Entity<ToastViewport>,
}

impl ToastHost {
  /// Creates a host for an application-owned notification viewport.
  pub fn new(viewport: Entity<ToastViewport>) -> Self {
    Self { viewport }
  }
}

impl RenderOnce for ToastHost {
  fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
    div()
      .absolute()
      .right(VIEWPORT_MARGIN)
      .bottom(VIEWPORT_MARGIN)
      .child(self.viewport)
  }
}

impl ToastViewport {
  /// Creates an empty notification viewport.
  pub fn new() -> Self {
    Self {
      manager: ToastManager::new(ToastMotion::sonner()),
      stack: ToastStackState::default(),
      next_id: 1,
      advancing: false,
      focus: None,
      theme: *ThemeColor::dark(),
    }
  }

  /// Sets the application-owned notification card palette.
  pub fn set_theme(&mut self, theme: ThemeColor, cx: &mut Context<Self>) {
    self.theme = theme;
    cx.notify();
  }

  /// Adds a notification and dismisses the oldest active item over capacity.
  ///
  /// # Parameters
  ///
  /// * `toast` supplies content, actions, variant, and optional lifetime.
  /// * `cx` emits capacity events and starts the lifecycle clock if necessary.
  ///
  /// # Returns
  ///
  /// A stable identity for later dismissal and action events.
  pub fn push(&mut self, toast: Toast, cx: &mut Context<Self>) -> ToastId {
    let (id, displaced) = self.enqueue(toast, cx.background_executor().now());
    if let Some(displaced) = displaced {
      cx.emit(ToastEvent::Dismissed(displaced));
    }
    self.start_advancing(cx);
    cx.notify();
    id
  }

  /// Dismisses an active notification; repeated dismissal has no effect.
  pub fn dismiss(&mut self, id: ToastId, cx: &mut Context<Self>) -> bool {
    if !self.manager.dismiss(&id, cx.background_executor().now()) {
      return false;
    }
    cx.emit(ToastEvent::Dismissed(id));
    self.start_advancing(cx);
    cx.notify();
    true
  }

  /// Returns the number of active notifications, excluding exiting cards.
  pub fn len(&self) -> usize {
    self
      .manager
      .iter()
      .filter(|(_, _, status)| *status != ToastTransitionStatus::Ending)
      .count()
  }

  /// Returns whether no active notification remains.
  pub fn is_empty(&self) -> bool {
    self.len() == 0
  }

  /// Applies the product's capacity policy without owning transition state.
  ///
  /// # Parameters
  ///
  /// * `toast` is the new notification.
  /// * `now` is the monotonic lifecycle timestamp.
  ///
  /// # Returns
  ///
  /// The new identity and the identity displaced by the capacity limit, if any.
  fn enqueue(&mut self, toast: Toast, now: Instant) -> (ToastId, Option<ToastId>) {
    let id = ToastId(self.next_id);
    self.next_id = self.next_id.wrapping_add(1).max(1);
    let timeout = toast.duration;
    self.manager.push(id, toast, ToastOptions { timeout }, now);
    let displaced = (self.len() > MAX_VISIBLE_TOASTS)
      .then(|| {
        self
          .manager
          .iter()
          .find(|(_, _, status)| *status != ToastTransitionStatus::Ending)
          .map(|(id, _, _)| *id)
      })
      .flatten();
    if let Some(displaced) = displaced {
      self.manager.dismiss(&displaced, now);
    }
    (id, displaced)
  }

  /// Runs one clock until every mounted entry is persistent and at rest.
  ///
  /// New pushes do not cancel this task: Kit retains each exit deadline, so
  /// bursts of notifications cannot keep previously dismissed entries alive.
  ///
  /// # Parameters
  ///
  /// * `cx` schedules lifecycle updates and delivers expiration events.
  ///
  /// # Returns
  ///
  /// Nothing; the detached task ends when no lifecycle time needs tracking.
  fn start_advancing(&mut self, cx: &mut Context<Self>) {
    if self.advancing {
      return;
    }
    self.advancing = true;
    cx.spawn(move |viewport: WeakEntity<Self>, async_cx: &mut AsyncApp| {
      let mut async_cx = async_cx.clone();
      async move {
        loop {
          Timer::after(LIFECYCLE_INTERVAL).await;
          let running = viewport.update(&mut async_cx, |this, cx| {
            let changes = this
              .manager
              .advance(cx.background_executor().now(), this.stack.is_expanded());
            for id in changes.ending {
              cx.emit(ToastEvent::Dismissed(id));
            }
            if changes.changed {
              cx.notify();
            }
            this.advancing = this
              .manager
              .iter()
              .any(|(_, toast, status)| status != ToastTransitionStatus::Present || toast.duration.is_some());
            this.advancing
          });
          if !matches!(running, Ok(true)) {
            break;
          }
        }
      }
    })
    .detach();
  }
}

impl Default for ToastViewport {
  fn default() -> Self {
    Self::new()
  }
}

impl EventEmitter<ToastEvent> for ToastViewport {}

impl Render for ToastViewport {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let focus = self.focus.get_or_insert_with(|| cx.focus_handle()).clone();
    let viewport = cx.weak_entity();
    let motion = ToastMotion::sonner();
    self.manager.visible(MAX_VISIBLE_TOASTS).fold(
      ToastStack::new("chitin-toasts", self.stack.clone())
        .placement(Anchor::BottomRight)
        .motion(motion)
        .focus_handle(focus)
        .w(TOAST_WIDTH),
      |stack, (id, toast, status)| {
        let closing = status == ToastTransitionStatus::Ending;
        stack.item(
          ("toast", id.0),
          BaseToast::new(("toast-card", id.0))
            .transition_status(status)
            .child(render_toast(*id, toast, self.theme, viewport.clone()))
            .with_animation(
              (
                "toast-visibility",
                id.0.wrapping_mul(2).wrapping_add(u64::from(closing)),
              ),
              Animation::new(if closing { motion.exit_duration } else { motion.duration })
                .with_easing(gpui::ease_in_out),
              move |card, delta| {
                let opacity = if closing { 1.0 - delta } else { delta };
                card.opacity(opacity).relative().top(px(24.0 * (1.0 - opacity)))
              },
            ),
        )
      },
    )
  }
}

/// Renders one notification card with optional action and close controls.
///
/// The card delegates activation to two gpui-kit buttons. Their handlers are
/// built here, against the owning viewport, so the composite stays responsible
/// only for layout and lifecycle wiring.
///
/// The left edge of the rounded card border is derived from [`ToastVariant`];
/// the action button is rendered only when the toast carries an action label,
/// and the close button is always present.
fn render_toast(id: ToastId, toast: &Toast, theme: ThemeColor, viewport: WeakEntity<ToastViewport>) -> gpui::Div {
  let accent = match toast.variant {
    ToastVariant::Default => theme.input,
    ToastVariant::Success => theme.success,
    ToastVariant::Info => theme.info,
    ToastVariant::Warning => theme.warning,
    ToastVariant::Error => theme.danger,
  };
  let content = div()
    .flex()
    .flex_col()
    .flex_1()
    .min_w_0()
    .gap_1()
    .child(
      div()
        .text_sm()
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(theme.popover_foreground)
        .child(toast.title.clone()),
    )
    .when_some(toast.description.clone(), |content, description| {
      content
        .text_color(theme.muted_foreground)
        .child(div().text_sm().child(description))
    });
  let close_id = id;
  let close_viewport = viewport.clone();
  let close = Button::new(format!("toast-close-{}", close_id.0))
    .with_variant(ButtonVariant::Ghost)
    .cursor_pointer()
    .w(px(26.0))
    .h(px(26.0))
    .px(px(5.0))
    .on_click(move |_, _, cx| {
      let _ = close_viewport.update(cx, |this, cx| this.dismiss(close_id, cx));
    })
    .child(Icon::default().path("icons/window-close.svg").with_size(px(14.0)));

  div()
    .relative()
    .flex()
    .items_center()
    .h(TOAST_HEIGHT)
    .rounded(TOAST_CORNER_RADIUS)
    .border_1()
    .border_color(theme.border)
    .bg(theme.popover)
    .shadow_md()
    .overflow_hidden()
    .child(
      div()
        .absolute()
        .left_0()
        .top_0()
        .bottom_0()
        .w(TOAST_ACCENT_CLIP_WIDTH)
        .overflow_hidden()
        .child(
          div()
            .absolute()
            .left_0()
            .top_0()
            .w(TOAST_WIDTH)
            .h(TOAST_HEIGHT)
            .rounded(TOAST_CORNER_RADIUS)
            .border(TOAST_ACCENT_WIDTH)
            .border_color(accent),
        ),
    )
    .child(
      div()
        .flex()
        .flex_1()
        .items_center()
        .gap_3()
        .px_3()
        .py_3()
        .child(content)
        .when_some(toast.action_label.clone(), |card, label| {
          let action_id = id;
          let action_viewport = viewport.clone();
          card.child(
            Button::new(format!("toast-action-{}", action_id.0))
              .with_variant(ButtonVariant::Secondary)
              .cursor_pointer()
              .on_click(move |_, _, cx| {
                let _ = action_viewport.update(cx, |this, cx| {
                  cx.emit(ToastEvent::Action(action_id));
                  this.dismiss(action_id, cx);
                });
              })
              .child(label),
          )
        })
        .child(close),
    )
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn fourth_notification_should_displace_oldest_active_entry() {
    let mut viewport = ToastViewport::new();
    let now = Instant::now();
    let (first, _) = viewport.enqueue(Toast::new("first").duration(None), now);
    viewport.enqueue(Toast::new("second").duration(None), now);
    viewport.enqueue(Toast::new("third").duration(None), now);
    let (_, displaced) = viewport.enqueue(Toast::new("fourth").duration(None), now);
    assert_eq!((viewport.len(), displaced), (MAX_VISIBLE_TOASTS, Some(first)));
  }

  #[test]
  fn new_pushes_should_not_postpone_displaced_entry_cleanup() {
    let mut viewport = ToastViewport::new();
    let now = Instant::now();
    let (first, _) = viewport.enqueue(Toast::new("first").duration(None), now);
    for index in 1..=3 {
      viewport.enqueue(Toast::new(format!("toast {index}")).duration(None), now);
    }
    let before_exit = now + ToastMotion::sonner().exit_duration / 2;
    viewport.enqueue(Toast::new("next").duration(None), before_exit);
    viewport
      .manager
      .advance(now + ToastMotion::sonner().exit_duration, false);
    assert!(viewport.manager.get(&first).is_none());
  }
}
