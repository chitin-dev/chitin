//! Pointer rectangle selection with measured bounds and a non-interactive overlay.

use gpui_kit::component::ElementExt as _;
use std::rc::Rc;

use gpui::{
  App, Bounds, DispatchPhase, Entity, Hsla, InteractiveElement, IntoElement, MouseButton, MouseMoveEvent, MouseUpEvent,
  ParentElement, Pixels, Point, RenderOnce, Styled, Window, div, px, rgba,
};

/// A rectangle preview or completed gesture, expressed in window coordinates.
#[derive(Clone, Copy, Debug)]
pub enum MarqueeEvent {
  /// Update temporary selection without committing application state.
  Preview(Bounds<Pixels>),
  /// Commit the final selection once, including a stationary click.
  Commit(Bounds<Pixels>),
  /// Discard the temporary selection after pointer tracking is interrupted.
  Cancel,
}

/// Persistent pointer state; application data and item hit testing remain caller-owned.
#[derive(Default)]
pub struct MarqueeState {
  bounds: Bounds<Pixels>,
  origin: Option<Point<Pixels>>,
  current: Point<Pixels>,
  /// The owning view is invalidated locally instead of refreshing every window view.
  owner: Option<gpui::EntityId>,
  preview_pending: bool,
  generation: u64,
}

impl MarqueeState {
  /// Cancels a gesture when its content or coordinate system is replaced.
  pub fn cancel(&mut self) {
    self.origin = None;
    self.preview_pending = false;
    self.generation = self.generation.wrapping_add(1);
  }

  fn rectangle(&self) -> Option<Bounds<Pixels>> {
    let a = self.origin?;
    let b = self.current;
    Some(Bounds::from_corners(
      gpui::point(a.x.min(b.x), a.y.min(b.y)),
      gpui::point(a.x.max(b.x), a.y.max(b.y)),
    ))
  }

  /// A queued frame consumes the newest pointer, but cannot revive a finished gesture.
  fn take_preview(&mut self, generation: u64) -> Option<Bounds<Pixels>> {
    if self.generation != generation {
      return None;
    }
    self.preview_pending = false;
    self.rectangle()
  }

  fn clamp(&self, point: Point<Pixels>) -> Point<Pixels> {
    gpui::point(
      point.x.clamp(self.bounds.left(), self.bounds.right()),
      point.y.clamp(self.bounds.top(), self.bounds.bottom()),
    )
  }
}

/// Visual properties independent of gesture handling and item hit testing.
#[derive(Clone, Copy)]
pub struct MarqueeStyle {
  /// Translucent fill of the selection rectangle.
  pub fill: Hsla,
  /// Outline of the selection rectangle.
  pub border: Hsla,
  /// Corner radius of the selection rectangle.
  pub radius: Pixels,
}

impl Default for MarqueeStyle {
  fn default() -> Self {
    Self {
      fill: rgba(0xe4e4e726).into(),
      border: rgba(0xe4e4e7b3).into(),
      radius: px(6.0),
    }
  }
}

type ChangeHandler = dyn Fn(MarqueeEvent, &mut Window, &mut App);

/// Left-button selection region owning pointer capture and rectangle feedback.
#[derive(IntoElement)]
pub struct Marquee {
  state: Entity<MarqueeState>,
  child: gpui::AnyElement,
  style: MarqueeStyle,
  on_change: Rc<ChangeHandler>,
}

impl Marquee {
  /// Wraps content in a selection region and emits semantic preview/commit events.
  pub fn new(
    state: Entity<MarqueeState>,
    child: impl IntoElement,
    on_change: impl Fn(MarqueeEvent, &mut Window, &mut App) + 'static,
  ) -> Self {
    Self {
      state,
      child: child.into_any_element(),
      style: MarqueeStyle::default(),
      on_change: Rc::new(on_change),
    }
  }

  /// Overrides appearance without changing selection behavior.
  pub fn style(mut self, style: MarqueeStyle) -> Self {
    self.style = style;
    self
  }
}

impl RenderOnce for Marquee {
  fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
    let pointer_state = self.state.clone();
    let pointer_change = self.on_change.clone();
    // GPUI requires window listeners to be registered during paint. They keep
    // tracking after the pointer leaves the region or a child button.
    let listeners = gpui::canvas(
      |_, _, _| (),
      move |_, _, window, _| {
        let owner = window.current_view();
        let state = pointer_state.clone();
        let on_change = pointer_change.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
          if phase != DispatchPhase::Capture || state.read(cx).origin.is_none() {
            return;
          }
          let generation = state.update(cx, |state, _| {
            if event.pressed_button != Some(MouseButton::Left) {
              state.cancel();
              return None;
            }
            state.current = state.clamp(event.position);
            if state.preview_pending {
              return None;
            }
            state.preview_pending = true;
            Some(state.generation)
          });
          if event.pressed_button != Some(MouseButton::Left) {
            on_change(MarqueeEvent::Cancel, window, cx);
          } else if let Some(generation) = generation {
            let state = state.clone();
            let on_change = on_change.clone();
            // Consume the newest pointer rectangle once per frame. Generation
            // guards prevent a queued preview from running after release/cancel.
            window.on_next_frame(move |window, cx| {
              let rectangle = state.update(cx, |state, _| state.take_preview(generation));
              if let Some(rectangle) = rectangle {
                on_change(MarqueeEvent::Preview(rectangle), window, cx);
                cx.notify(owner);
              }
            });
          }
          cx.notify(owner);
          cx.stop_propagation();
        });
        let state = pointer_state;
        let on_change = pointer_change;
        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
          if phase != DispatchPhase::Capture || event.button != MouseButton::Left || state.read(cx).origin.is_none() {
            return;
          }
          let update = state.update(cx, |state, _| {
            state.current = state.clamp(event.position);
            let result = state
              .rectangle()
              .map(MarqueeEvent::Commit)
              .unwrap_or(MarqueeEvent::Cancel);
            state.cancel();
            result
          });
          on_change(update, window, cx);
          cx.notify(owner);
          // A dragged button must not also submit its single-residue click on release.
          cx.stop_propagation();
        });
      },
    )
    .absolute()
    .inset_0();

    let bounds = self.state.read(cx).bounds;
    let rectangle = self.state.read(cx).rectangle();
    let measure_state = self.state.clone();
    let press_state = self.state;
    let on_change = self.on_change;
    let mut region = div()
      .relative()
      .flex()
      .flex_col()
      .flex_1()
      .min_h_0()
      .min_w_0()
      .overflow_hidden()
      .on_prepaint(move |bounds, window, cx| {
        measure_state.update(cx, |state, _| {
          state.bounds = bounds;
          state.owner = Some(window.current_view());
        });
      })
      .capture_any_mouse_down(move |event, window, cx| {
        if event.button != MouseButton::Left || !press_state.read(cx).bounds.contains(&event.position) {
          return;
        }
        press_state.update(cx, |state, _| {
          state.cancel();
          state.origin = Some(event.position);
          state.current = event.position;
        });
        on_change(
          MarqueeEvent::Preview(Bounds::new(event.position, gpui::size(px(0.0), px(0.0)))),
          window,
          cx,
        );
        if let Some(owner) = press_state.read(cx).owner {
          cx.notify(owner);
        }
        cx.stop_propagation();
      })
      .child(self.child)
      .child(listeners);
    if let Some(rectangle) =
      rectangle.filter(|rectangle| rectangle.size.width > px(3.0) || rectangle.size.height > px(3.0))
    {
      region = region.child(
        div()
          .absolute()
          .left(rectangle.origin.x - bounds.origin.x)
          .top(rectangle.origin.y - bounds.origin.y)
          .w(rectangle.size.width.max(px(2.0)))
          .h(rectangle.size.height.max(px(2.0)))
          .rounded(self.style.radius)
          .border_1()
          .border_color(self.style.border)
          .bg(self.style.fill),
      );
    }
    region
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gpui::{AppContext, Context, Render};
  use gpui_kit::component::button::Button;
  use std::cell::{Cell, RefCell};

  struct Probe {
    state: Entity<MarqueeState>,
    events: Rc<RefCell<Vec<MarqueeEvent>>>,
    clicks: Rc<Cell<usize>>,
  }

  impl Render for Probe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
      let events = self.events.clone();
      let clicks = self.clicks.clone();
      div().flex().flex_col().w(px(300.0)).h(px(200.0)).child(Marquee::new(
        self.state.clone(),
        Button::new("marquee-test-cell")
          .w(px(100.0))
          .h(px(100.0))
          .child("Residue")
          .on_click(move |_, _, _| clicks.set(clicks.get() + 1)),
        move |event, _, _| events.borrow_mut().push(event),
      ))
    }
  }

  #[gpui::test]
  fn dragging_over_a_button_commits_once_even_when_released_outside(cx: &mut gpui::TestAppContext) {
    cx.update(crate::init);
    let events = Rc::new(RefCell::new(Vec::new()));
    let clicks = Rc::new(Cell::new(0));
    let (_, cx) = cx.add_window_view(|_, cx| Probe {
      state: cx.new(|_| MarqueeState::default()),
      events: events.clone(),
      clicks: clicks.clone(),
    });
    cx.simulate_mouse_move(gpui::point(px(40.0), px(40.0)), None, Default::default());
    cx.simulate_mouse_down(gpui::point(px(40.0), px(40.0)), MouseButton::Left, Default::default());
    cx.simulate_mouse_move(
      gpui::point(px(500.0), px(250.0)),
      Some(MouseButton::Left),
      Default::default(),
    );
    cx.simulate_mouse_up(gpui::point(px(500.0), px(250.0)), MouseButton::Left, Default::default());
    let commits: Vec<_> = events
      .borrow()
      .iter()
      .filter_map(|event| match event {
        MarqueeEvent::Commit(bounds) => Some(*bounds),
        _ => None,
      })
      .collect();
    assert_eq!(
      commits,
      [Bounds::from_corners(
        gpui::point(px(40.0), px(40.0)),
        gpui::point(px(300.0), px(200.0))
      )]
    );
    assert_eq!(clicks.get(), 0);
  }

  #[test]
  fn reverse_drag_normalizes_the_selection_rectangle() {
    let state = MarqueeState {
      origin: Some(gpui::point(px(70.0), px(50.0))),
      current: gpui::point(px(10.0), px(20.0)),
      ..Default::default()
    };
    assert_eq!(
      state.rectangle(),
      Some(Bounds::new(
        gpui::point(px(10.0), px(20.0)),
        gpui::size(px(60.0), px(30.0))
      ))
    );
  }

  #[test]
  fn frame_preview_uses_latest_pointer_and_cancel_invalidates_queued_work() {
    let mut state = MarqueeState {
      origin: Some(gpui::point(px(10.0), px(10.0))),
      current: gpui::point(px(20.0), px(20.0)),
      preview_pending: true,
      generation: 7,
      ..Default::default()
    };
    state.current = gpui::point(px(80.0), px(40.0));
    assert_eq!(
      state.take_preview(7),
      Some(Bounds::new(
        gpui::point(px(10.0), px(10.0)),
        gpui::size(px(70.0), px(30.0))
      ))
    );
    state.cancel();
    state.origin = Some(gpui::point(px(5.0), px(5.0)));
    state.preview_pending = true;
    assert_eq!(state.take_preview(7), None);
    assert!(
      state.preview_pending,
      "old frame must not consume the next gesture's pending preview"
    );
  }
}
