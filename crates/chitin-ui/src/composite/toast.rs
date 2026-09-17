//! Stacked transient notifications for window-level feedback.
//!
//! A [`ToastViewport`] owns the notification lifecycle, including capacity,
//! automatic expiry, pointer-paused expiry, and entry animations. [`ToastHost`]
//! supplies the window-root overlay needed to keep the viewport above the
//! document content and to observe pointer movement across the complete stack.

use std::{
  collections::VecDeque,
  time::{Duration, Instant},
};

use gpui::{
  App, AsyncApp, Context, DispatchPhase, Entity, EventEmitter, IntoElement, MouseExitEvent, MouseMoveEvent,
  ParentElement, Render, RenderOnce, SharedString, Subscription, Timer, WeakEntity, Window, canvas, div, prelude::*,
  px,
};

use crate::{
  primitive::{
    button::{Button, ButtonEvent, ButtonSize, ButtonState, ButtonStyle, ButtonVariant},
    icon::Icon,
  },
  themes::{UIThemes, builtins},
};

/// Maximum number of notifications retained by one viewport.
pub const MAX_VISIBLE_TOASTS: usize = 3;
/// Default time for which an unhovered notification remains visible.
pub const DEFAULT_TOAST_DURATION: Duration = Duration::from_secs(5);

/// Fixed width of each notification card.
const TOAST_WIDTH: gpui::Pixels = px(380.0);
/// Fixed height used by both collapsed and expanded card layouts.
const TOAST_HEIGHT: gpui::Pixels = px(82.0);
/// Outer radius shared by every notification card.
const TOAST_CORNER_RADIUS: gpui::Pixels = px(8.0);
/// Width of the semantic border stroke drawn over the card's left edge.
const TOAST_ACCENT_WIDTH: gpui::Pixels = px(3.0);
/// Portion of the semantic outline retained to include both left corner arcs.
const TOAST_ACCENT_CLIP_WIDTH: gpui::Pixels = px(6.0);
/// Vertical gap between cards after the stack is expanded.
const TOAST_GAP: gpui::Pixels = px(8.0);
/// Vertical amount by which each hidden card remains visible in the collapsed stack.
const STACK_PEEK: gpui::Pixels = px(12.0);
/// Horizontal inset applied to cards behind the frontmost collapsed card.
const STACK_INSET: gpui::Pixels = px(19.0);
/// Opacity reduction applied to each successive card in the collapsed stack.
const STACK_OPACITY_STEP: f32 = 0.1;
/// Full duration of a stack expansion or entry transition.
const STACK_TRANSITION_DURATION: Duration = Duration::from_millis(500);
/// Target cadence for asynchronous stack animation updates.
const STACK_ANIMATION_FRAME_INTERVAL: Duration = Duration::from_millis(16);
/// Distance between the viewport and the containing window's bottom-right corner.
const VIEWPORT_MARGIN: gpui::Pixels = px(16.0);

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
  /// The timer starts when the toast is pushed into a viewport. If the pointer
  /// is over the viewport when the timer expires, dismissal is deferred until
  /// the pointer leaves the stack.
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

/// One retained notification and the primitive controls it owns.
struct ToastEntry {
  /// Stable identity used to target dismissal and action events.
  id: ToastId,
  /// Immutable content and lifecycle configuration for the entry.
  toast: Toast,
  /// Primitive close-button state owned by this entry.
  close: Entity<ButtonState>,
  /// Primitive action-button state, present only when the toast has an action.
  action: Option<Entity<ButtonState>>,
  /// Records expiry while hovered so it can be applied after pointer exit.
  expired_while_hovered: bool,
  /// Marks an entry for exit animation and excludes it from active capacity.
  dismissing: bool,
  /// Interpolated stack depth: `0.0` is the frontmost card.
  visual_depth: f32,
  /// Interpolated visibility: entering cards start at `0.0`, visible cards at `1.0`.
  visibility: f32,
}

/// Window-level notification queue rendered as a bottom-right stack.
pub struct ToastViewport {
  /// Entries retained during normal and exit-animation states.
  entries: VecDeque<ToastEntry>,
  /// Next numeric identity assigned to a pushed toast.
  next_id: u64,
  /// Whether pointer interaction currently keeps the stack expanded.
  expanded: bool,
  /// Animated expansion progress between collapsed (`0.0`) and expanded (`1.0`).
  expansion: f32,
  /// Generation used to invalidate expansion animations superseded by a new one.
  stack_animation_generation: u64,
  /// Generation used to invalidate entry animations superseded by a new one.
  entry_animation_generation: u64,
  /// Theme propagated to card backgrounds, text, borders, and controls.
  theme: UIThemes,
}

/// Interpolated position and appearance of one card in the toast stack.
#[derive(Clone, Copy, Debug, PartialEq)]
struct ToastCardLayout {
  /// Distance from the bottom edge of the viewport.
  bottom: gpui::Pixels,
  /// Horizontal inset from the viewport edges.
  inset: gpui::Pixels,
  /// Final card opacity after depth and lifecycle visibility are combined.
  opacity: f32,
}

/// Start and target values for one toast lifecycle transition.
#[derive(Clone, Copy, Debug, PartialEq)]
struct ToastEntryAnimation {
  /// Entry identity used to apply this transition after asynchronous updates.
  id: ToastId,
  /// Starting stack depth captured when the animation begins.
  from_depth: f32,
  /// Target stack depth after active entries have been reflowed.
  to_depth: f32,
  /// Starting lifecycle visibility.
  from_visibility: f32,
  /// Target visibility; dismissing entries target `0.0`.
  to_visibility: f32,
}

/// Window-root anchor that positions a viewport above bottom-right content.
#[derive(IntoElement)]
pub struct ToastHost {
  viewport: Entity<ToastViewport>,
}

impl ToastHost {
  /// Creates a root overlay for an existing notification viewport.
  ///
  /// The host does not own notification state. It only anchors the supplied
  /// entity and provides the window-level pointer hitbox required for hover
  /// expansion when a child card or one of its controls intercepts events.
  pub fn new(viewport: Entity<ToastViewport>) -> Self {
    Self { viewport }
  }
}

impl RenderOnce for ToastHost {
  /// Anchors the viewport in the containing window's bottom-right corner.
  ///
  /// The transparent canvas covers the viewport bounds and listens during the
  /// capture phase. This makes hover state independent of which card or button
  /// is currently under the pointer and resets expansion on mouse exit.
  fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
    let pointer_viewport = self.viewport.clone();
    let exit_viewport = self.viewport.clone();

    div()
      .absolute()
      .right(VIEWPORT_MARGIN)
      .bottom(VIEWPORT_MARGIN)
      .child(self.viewport)
      // The controls inside a toast occlude their parent hitbox. Observe the
      // host's measured bounds directly so hovering buttons still keeps the
      // complete stack expanded.
      .child(
        canvas(
          |bounds, _, _| bounds,
          move |_, bounds, window, _| {
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
              if phase != DispatchPhase::Capture {
                return;
              }
              let expanded = bounds.contains(&event.position);
              pointer_viewport.update(cx, |viewport, cx| viewport.set_expanded(expanded, cx));
            });
            window.on_mouse_event(move |_: &MouseExitEvent, phase, _, cx| {
              if phase == DispatchPhase::Capture {
                exit_viewport.update(cx, |viewport, cx| viewport.set_expanded(false, cx));
              }
            });
          },
        )
        .absolute()
        .inset_0(),
      )
  }
}

impl ToastViewport {
  /// Creates an empty viewport using the default dark theme.
  ///
  /// The viewport starts collapsed and has no active animation. Toast IDs begin
  /// at one; wrapping IDs are skipped back to one so the zero value remains an
  /// unused sentinel for diagnostics and debugging.
  pub fn new() -> Self {
    Self {
      entries: VecDeque::new(),
      next_id: 1,
      expanded: false,
      expansion: 0.0,
      stack_animation_generation: 0,
      entry_animation_generation: 0,
      theme: builtins::dark(),
    }
  }

  /// Sets the semantic theme used by every notification card.
  ///
  /// Existing entries are restyled on the next GPUI render. The operation does
  /// not restart lifecycle or stack animations.
  pub fn set_theme(&mut self, theme: UIThemes, cx: &mut Context<Self>) {
    self.theme = theme;
    cx.notify();
  }

  /// Adds a notification, wires its controls, and schedules optional expiry.
  ///
  /// The viewport retains at most [`MAX_VISIBLE_TOASTS`] entries. Adding a
  /// fourth entry dismisses the oldest one before placing the new entry at the
  /// front of the collapsed visual stack.
  ///
  /// # Parameters
  ///
  /// * `toast` contains the message, semantic variant, action, and lifetime.
  /// * `cx` creates primitive controls and schedules automatic dismissal.
  ///
  /// # Returns
  ///
  /// The stable identity used by action and dismissal events.
  pub fn push(&mut self, toast: Toast, cx: &mut Context<Self>) -> ToastId {
    let id = ToastId(self.next_id);
    self.next_id = self.next_id.wrapping_add(1).max(1);
    let close = cx.new(ButtonState::new);
    let close_subscription: Subscription = cx.subscribe(&close, move |this, _, event, cx| {
      if matches!(event, ButtonEvent::Click) {
        this.dismiss(id, cx);
      }
    });
    close_subscription.detach();

    let action = toast.action_label.as_ref().map(|_| {
      let button = cx.new(ButtonState::new);
      let action_subscription: Subscription = cx.subscribe(&button, move |this, _, event, cx| {
        if matches!(event, ButtonEvent::Click) {
          cx.emit(ToastEvent::Action(id));
          this.dismiss(id, cx);
        }
      });
      action_subscription.detach();
      button
    });
    let duration = toast.duration;
    self.entries.push_back(ToastEntry {
      id,
      toast,
      close,
      action,
      expired_while_hovered: false,
      dismissing: false,
      visual_depth: 0.0,
      visibility: 0.0,
    });
    if self.active_entry_count() > MAX_VISIBLE_TOASTS
      && let Some(displaced_id) = oldest_active_toast_id(&self.entries)
      && let Some(displaced) = self.entries.iter_mut().find(|entry| entry.id == displaced_id)
    {
      displaced.dismissing = true;
      cx.emit(ToastEvent::Dismissed(displaced_id));
    }
    self.start_entry_animation(cx);
    if let Some(duration) = duration {
      cx.spawn(move |this: WeakEntity<Self>, async_cx: &mut AsyncApp| {
        let mut async_cx = async_cx.clone();
        async move {
          Timer::after(duration).await;
          let _ = this.update(&mut async_cx, |this, cx| this.expire(id, cx));
        }
      })
      .detach();
    }
    cx.notify();
    id
  }

  /// Dismisses a retained notification by identity.
  ///
  /// Dismissal is idempotent while the entry is present: an already-dismissing
  /// entry is ignored. A successful call emits [`ToastEvent::Dismissed`], then
  /// lets the normal exit animation remove the entry from the queue.
  pub fn dismiss(&mut self, id: ToastId, cx: &mut Context<Self>) -> bool {
    let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == id) else {
      return false;
    };
    if entry.dismissing {
      return false;
    }
    entry.dismissing = true;
    cx.emit(ToastEvent::Dismissed(id));
    self.start_entry_animation(cx);
    cx.notify();
    true
  }

  /// Returns the number of retained notifications that are not exiting.
  pub fn len(&self) -> usize {
    self.active_entry_count()
  }

  /// Returns whether the viewport contains no notifications that are still active.
  pub fn is_empty(&self) -> bool {
    self.active_entry_count() == 0
  }

  /// Returns the number of entries that have not begun their exit animation.
  fn active_entry_count(&self) -> usize {
    self.entries.iter().filter(|entry| !entry.dismissing).count()
  }

  /// Pauses or resumes collapsed-stack expiry behavior.
  ///
  /// Expanding the stack does not cancel expiry timers. Instead, an entry whose
  /// timer fires while hovered is marked as expired. When the pointer leaves,
  /// all such entries are dismissed together. This keeps the timer semantics
  /// deterministic while preventing a hovered notification from disappearing.
  fn set_expanded(&mut self, expanded: bool, cx: &mut Context<Self>) {
    if self.expanded == expanded {
      return;
    }
    self.expanded = expanded;
    self.start_expansion_animation(cx);
    if !expanded {
      let expired = self
        .entries
        .iter()
        .filter(|entry| entry.expired_while_hovered)
        .map(|entry| entry.id)
        .collect::<Vec<_>>();
      for id in expired {
        self.dismiss(id, cx);
      }
    }
    cx.notify();
  }

  /// Animates between the compact stack and its fully separated layout.
  ///
  /// A monotonically changing generation makes an older asynchronous loop stop
  /// when pointer movement starts a newer transition. The duration is scaled by
  /// the remaining distance so reversing halfway through an animation does not
  /// move at a different visual speed.
  ///
  /// # Parameters
  ///
  /// * `cx` schedules animation frames and redraws the viewport.
  ///
  /// # Returns
  ///
  /// This function has no return value.
  fn start_expansion_animation(&mut self, cx: &mut Context<Self>) {
    let from = self.expansion;
    let to = if self.expanded { 1.0 } else { 0.0 };
    let distance = (to - from).abs();
    if distance <= f32::EPSILON {
      self.expansion = to;
      return;
    }

    self.stack_animation_generation = self.stack_animation_generation.wrapping_add(1);
    let generation = self.stack_animation_generation;
    let duration = STACK_TRANSITION_DURATION.mul_f32(distance);
    cx.spawn(move |this: WeakEntity<Self>, async_cx: &mut AsyncApp| {
      let mut async_cx = async_cx.clone();
      async move {
        let started_at = Instant::now();
        loop {
          let linear_progress = (started_at.elapsed().as_secs_f32() / duration.as_secs_f32()).min(1.0);
          let expansion = from + (to - from) * stack_transition_easing(linear_progress);
          let finished = linear_progress >= 1.0;
          let Ok(continue_animation) = this.update(&mut async_cx, |this, cx| {
            if this.stack_animation_generation != generation {
              return false;
            }
            this.expansion = if finished { to } else { expansion };
            cx.notify();
            !finished
          }) else {
            break;
          };
          if !continue_animation {
            break;
          }
          Timer::after(STACK_ANIMATION_FRAME_INTERVAL).await;
        }
      }
    })
    .detach();
  }

  /// Animates entering, exiting, and reflowing notification cards.
  ///
  /// Targets are captured before the asynchronous loop starts. Each frame then
  /// interpolates both depth and visibility from that snapshot. A newer push or
  /// dismissal invalidates the previous loop through
  /// `entry_animation_generation`, preventing stale frames from restoring old
  /// positions or visibility values.
  ///
  /// # Parameters
  ///
  /// * `cx` schedules animation frames and redraws the viewport.
  ///
  /// # Returns
  ///
  /// This function has no return value.
  fn start_entry_animation(&mut self, cx: &mut Context<Self>) {
    let animations = entry_animation_targets(&self.entries);
    if animations.is_empty() {
      return;
    }

    self.entry_animation_generation = self.entry_animation_generation.wrapping_add(1);
    let generation = self.entry_animation_generation;
    cx.spawn(move |this: WeakEntity<Self>, async_cx: &mut AsyncApp| {
      let mut async_cx = async_cx.clone();
      async move {
        let started_at = Instant::now();
        loop {
          let linear_progress = (started_at.elapsed().as_secs_f32() / STACK_TRANSITION_DURATION.as_secs_f32()).min(1.0);
          let progress = stack_transition_easing(linear_progress);
          let finished = linear_progress >= 1.0;
          let Ok(continue_animation) = this.update(&mut async_cx, |this, cx| {
            if this.entry_animation_generation != generation {
              return false;
            }
            apply_entry_animation_frame(&mut this.entries, &animations, progress);
            if finished {
              this.entries.retain(|entry| !entry.dismissing);
              if this.entries.is_empty() {
                this.expanded = false;
                this.expansion = 0.0;
                this.stack_animation_generation = this.stack_animation_generation.wrapping_add(1);
              }
            }
            cx.notify();
            !finished
          }) else {
            break;
          };
          if !continue_animation {
            break;
          }
          Timer::after(STACK_ANIMATION_FRAME_INTERVAL).await;
        }
      }
    })
    .detach();
  }

  /// Expires immediately unless pointer interaction currently holds the stack open.
  ///
  /// Timer callbacks identify entries by [`ToastId`], so a delayed callback for
  /// an already-removed entry is harmless. Hovered expiry is recorded rather
  /// than repeatedly rescheduled, and is applied by [`Self::set_expanded`] when
  /// the stack closes.
  fn expire(&mut self, id: ToastId, cx: &mut Context<Self>) {
    if self.expanded {
      if let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == id) {
        entry.expired_while_hovered = true;
      }
      return;
    }
    self.dismiss(id, cx);
  }
}

impl Default for ToastViewport {
  fn default() -> Self {
    Self::new()
  }
}

impl EventEmitter<ToastEvent> for ToastViewport {}

impl Render for ToastViewport {
  /// Renders the collapsed stack or its pointer-expanded vertical list.
  ///
  /// Every card remains absolutely positioned inside one fixed viewport. In
  /// the collapsed state, depth controls peeking, inset, and opacity; in the
  /// expanded state, cards use regular height-plus-gap spacing. Lifecycle
  /// visibility is multiplied into the layout opacity so entering and exiting
  /// cards can animate without changing the queue during a frame.
  fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
    let expansion = self.expansion;
    let mut viewport = div()
      .relative()
      .w(TOAST_WIDTH)
      .h(toast_viewport_height(maximum_visual_depth(&self.entries), expansion))
      .when(self.entries.is_empty(), |style| style.invisible());

    for entry in &self.entries {
      let mut layout = toast_card_layout(entry.visual_depth, expansion);
      layout.bottom -= TOAST_HEIGHT * 1.5 * (1.0 - entry.visibility);
      layout.opacity *= entry.visibility;
      viewport = viewport.child(
        render_toast(entry, self.theme)
          .absolute()
          .left(layout.inset)
          .right(layout.inset)
          .bottom(layout.bottom)
          .opacity(layout.opacity),
      );
    }

    viewport
  }
}

/// Builds lifecycle animation targets for every retained notification.
///
/// Active entries are assigned their position in reverse queue order so the
/// newest active entry is rendered in front. Entries already marked for
/// dismissal keep their current depth and only animate their visibility to
/// zero; this prevents an exiting card from jumping behind the newly reflowed
/// stack.
///
/// # Parameters
///
/// * `entries` contains active, entering, and exiting notification cards.
///
/// # Returns
///
/// Start and target values for the next lifecycle animation.
fn entry_animation_targets(entries: &VecDeque<ToastEntry>) -> Vec<ToastEntryAnimation> {
  entries
    .iter()
    .map(|entry| {
      let target_depth = if entry.dismissing {
        entry.visual_depth
      } else {
        entries
          .iter()
          .rev()
          .filter(|candidate| !candidate.dismissing)
          .position(|candidate| candidate.id == entry.id)
          .unwrap_or_default() as f32
      };
      ToastEntryAnimation {
        id: entry.id,
        from_depth: entry.visual_depth,
        to_depth: target_depth,
        from_visibility: entry.visibility,
        to_visibility: if entry.dismissing { 0.0 } else { 1.0 },
      }
    })
    .collect()
}

/// Applies one interpolated lifecycle frame to retained entries.
///
/// An animation may outlive the entry that created it because a newer lifecycle
/// event can remove or replace that entry. Missing IDs are therefore skipped,
/// allowing stale animation snapshots to finish harmlessly when they are not
/// the current generation.
///
/// # Parameters
///
/// * `entries` contains the mutable card presentation state.
/// * `animations` supplies the captured start and target values.
/// * `progress` is the eased animation progress.
///
/// # Returns
///
/// This function has no return value.
fn apply_entry_animation_frame(entries: &mut VecDeque<ToastEntry>, animations: &[ToastEntryAnimation], progress: f32) {
  for animation in animations {
    let Some(entry) = entries.iter_mut().find(|entry| entry.id == animation.id) else {
      continue;
    };
    entry.visual_depth = lerp_f32(animation.from_depth, animation.to_depth, progress);
    entry.visibility = lerp_f32(animation.from_visibility, animation.to_visibility, progress);
  }
}

/// Calculates one card's layout between compact and expanded stack states.
///
/// In the collapsed state, deeper cards are lifted, inset, and made slightly
/// translucent to communicate depth while keeping the stack compact. At full
/// expansion, all cards share the same horizontal bounds and are separated by
/// the regular card gap.
///
/// # Parameters
///
/// * `depth` is the card's distance behind the frontmost notification.
/// * `expansion` is the normalized transition progress.
///
/// # Returns
///
/// The interpolated offset, inset, and opacity for the card.
fn toast_card_layout(depth: f32, expansion: f32) -> ToastCardLayout {
  let expansion = expansion.clamp(0.0, 1.0);
  let collapsed_bottom = STACK_PEEK * depth;
  let expanded_bottom = (TOAST_HEIGHT + TOAST_GAP) * depth;
  ToastCardLayout {
    bottom: lerp_pixels(collapsed_bottom, expanded_bottom, expansion),
    inset: STACK_INSET * depth * (1.0 - expansion),
    opacity: 1.0 - depth * STACK_OPACITY_STEP * (1.0 - expansion),
  }
}

/// Calculates the animated bounds required to contain the current stack.
///
/// The viewport must grow during expansion before cards move into their final
/// positions; interpolating this height prevents cards from being clipped by
/// the overlay container. An empty viewport reports zero height so the host
/// does not reserve space when no notifications remain.
///
/// # Parameters
///
/// * `maximum_depth` is the deepest retained card, or `None` for an empty queue.
/// * `expansion` is the normalized transition progress.
///
/// # Returns
///
/// The viewport height at the requested animation position.
fn toast_viewport_height(maximum_depth: Option<f32>, expansion: f32) -> gpui::Pixels {
  let Some(maximum_depth) = maximum_depth else {
    return px(0.0);
  };
  let collapsed = TOAST_HEIGHT + STACK_PEEK * maximum_depth;
  let expanded = TOAST_HEIGHT + (TOAST_HEIGHT + TOAST_GAP) * maximum_depth;
  lerp_pixels(collapsed, expanded, expansion.clamp(0.0, 1.0))
}

/// Returns the greatest animated depth currently retained by the viewport.
///
/// The value includes entries that are leaving until their exit animation has
/// completed, which keeps the viewport large enough for the visible animation.
fn maximum_visual_depth(entries: &VecDeque<ToastEntry>) -> Option<f32> {
  entries.iter().map(|entry| entry.visual_depth).reduce(f32::max)
}

/// Interpolates pixel values without converting through device coordinates.
///
/// GPUI pixel values are interpolated directly so layout animation remains in
/// logical coordinates and does not depend on the current scale factor.
fn lerp_pixels(from: gpui::Pixels, to: gpui::Pixels, progress: f32) -> gpui::Pixels {
  from + (to - from) * progress
}

/// Interpolates scalar presentation values.
///
/// Callers pass an eased progress value when the transition should have a
/// non-linear visual timing curve.
fn lerp_f32(from: f32, to: f32, progress: f32) -> f32 {
  from + (to - from) * progress
}

/// Applies the stack transition's `cubic-bezier(0.22, 1, 0.36, 1)` easing.
///
/// # Parameters
///
/// * `progress` is the normalized linear animation time.
///
/// # Returns
///
/// The eased transition progress.
fn stack_transition_easing(progress: f32) -> f32 {
  let progress = progress.clamp(0.0, 1.0);
  let mut parameter = progress;
  for _ in 0..6 {
    let estimate = cubic_bezier_coordinate(parameter, 0.22, 0.36);
    let derivative = cubic_bezier_derivative(parameter, 0.22, 0.36);
    if derivative.abs() <= f32::EPSILON {
      break;
    }
    parameter = (parameter - (estimate - progress) / derivative).clamp(0.0, 1.0);
  }
  cubic_bezier_coordinate(parameter, 1.0, 1.0)
}

/// Evaluates one coordinate of a unit cubic Bezier curve.
///
/// # Parameters
///
/// * `parameter` is the curve parameter in the unit interval.
/// * `first_control` is the first control point coordinate.
/// * `second_control` is the second control point coordinate.
///
/// # Returns
///
/// The curve coordinate at `parameter`.
fn cubic_bezier_coordinate(parameter: f32, first_control: f32, second_control: f32) -> f32 {
  let inverse = 1.0 - parameter;
  3.0 * inverse * inverse * parameter * first_control
    + 3.0 * inverse * parameter * parameter * second_control
    + parameter * parameter * parameter
}

/// Evaluates the derivative used to invert a unit cubic Bezier curve.
///
/// # Parameters
///
/// * `parameter` is the curve parameter in the unit interval.
/// * `first_control` is the first control point coordinate.
/// * `second_control` is the second control point coordinate.
///
/// # Returns
///
/// The coordinate derivative at `parameter`.
fn cubic_bezier_derivative(parameter: f32, first_control: f32, second_control: f32) -> f32 {
  let inverse = 1.0 - parameter;
  3.0 * inverse * inverse * first_control
    + 6.0 * inverse * parameter * (second_control - first_control)
    + 3.0 * parameter * parameter * (1.0 - second_control)
}

/// Renders one notification card with optional action and close controls.
///
/// The card deliberately delegates interaction state to the reusable button
/// primitive. This keeps keyboard activation, focus behavior, and semantic
/// button events consistent with the rest of the UI library while the toast
/// composite remains responsible only for layout and lifecycle wiring.
///
/// The left edge of the rounded card border is derived from [`ToastVariant`];
/// action and close buttons are rendered only when their corresponding entry
/// state exists.
fn render_toast(entry: &ToastEntry, theme: UIThemes) -> gpui::Div {
  let accent = match entry.toast.variant {
    ToastVariant::Default => theme.border.tertiary,
    ToastVariant::Success => theme.text.success,
    ToastVariant::Info => theme.text.info,
    ToastVariant::Warning => theme.text.warning,
    ToastVariant::Error => theme.text.error,
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
        .text_color(theme.text.primary)
        .child(entry.toast.title.clone()),
    )
    .when_some(entry.toast.description.clone(), |content, description| {
      content
        .text_color(theme.text.secondary)
        .child(div().text_sm().child(description))
    });
  let close = Button::new(entry.close.clone())
    .theme(theme)
    .variant(ButtonVariant::Transparent)
    .size(ButtonSize::Small)
    .style(
      ButtonStyle::new()
        .width(px(26.0))
        .height(px(26.0))
        .horizontal_padding(px(5.0)),
    )
    .child(Icon::new("icons/window-close.svg").size(px(14.0)).theme(theme));

  div()
    .relative()
    .flex()
    .items_center()
    .h(TOAST_HEIGHT)
    .rounded(TOAST_CORNER_RADIUS)
    .border_1()
    .border_color(theme.border.primary)
    .bg(theme.background.secondary)
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
        .when_some(
          entry.toast.action_label.clone().zip(entry.action.clone()),
          |card, (label, action)| {
            card.child(
              Button::new(action)
                .theme(theme)
                .variant(ButtonVariant::Secondary)
                .size(ButtonSize::Small)
                .child(label),
            )
          },
        )
        .child(close),
    )
}

/// Returns the oldest notification that has not begun dismissal.
///
/// Entries are stored oldest-first, so the first non-dismissing entry is the
/// one displaced when the viewport reaches its capacity.
fn oldest_active_toast_id(entries: &VecDeque<ToastEntry>) -> Option<ToastId> {
  entries.iter().find(|entry| !entry.dismissing).map(|entry| entry.id)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn expanded_second_card_should_use_full_card_spacing() {
    let layout = toast_card_layout(1.0, 1.0);

    assert_eq!(layout.bottom, TOAST_HEIGHT + TOAST_GAP);
  }

  #[test]
  fn collapsed_three_card_stack_should_only_expose_two_peeks() {
    let height = toast_viewport_height(Some(2.0), 0.0);

    assert_eq!(height, TOAST_HEIGHT + STACK_PEEK * 2.0);
  }

  #[test]
  fn stack_transition_easing_should_preserve_start() {
    assert_eq!(stack_transition_easing(0.0), 0.0);
  }

  #[test]
  fn stack_transition_easing_should_preserve_end() {
    assert_eq!(stack_transition_easing(1.0), 1.0);
  }
}
