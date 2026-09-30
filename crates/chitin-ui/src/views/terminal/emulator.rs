//! GPUI rendering and input handling for a VT terminal session.

use std::{cell::Cell, ops::Range, rc::Rc, sync::Arc};

use chitin_terminal::{
  TerminalCell, TerminalCellAttributes, TerminalColor, TerminalEvent, TerminalNamedColor, TerminalScroll,
  TerminalScrollState, TerminalSession, TerminalSessionError, TerminalSize, TerminalSnapshot,
};
use gpui_kit::component::theme::ThemeColor;

use gpui::{
  AnyElement, App, AsyncApp, Bounds, Context, Div, Element, ElementId, ElementInputHandler, Entity, EntityInputHandler,
  EventEmitter, FocusHandle, Font, FontFallbacks, FontFeatures, FontWeight, GlobalElementId, Hsla, InspectorElementId,
  InteractiveElement, IntoElement, KeyDownEvent, Keystroke, LayoutId, MouseButton, ParentElement, Pixels, Point,
  RenderOnce, ScrollDelta, ScrollWheelEvent, SharedString, Size, Styled, Task, TextRun, UTF16Selection, WeakEntity,
  Window, div, font, point, prelude::*, px, rgb, size,
};
use gpui_kit::component::scroll::{Scrollbar, ScrollbarHandle, ScrollbarMode};

const FONT_SIZE: f32 = 12.0;
const LINE_HEIGHT_MULTIPLIER: f32 = 1.4;
/// Largest viewport motion one wheel event may ask for.
///
/// A platform is free to report a very large pixel delta, and one such event
/// should not throw the viewport across the whole scrollback.
const MAX_WHEEL_LINES_PER_EVENT: i32 = 10;

/// Semantic events emitted by the terminal emulator primitive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalEmulatorEvent {
  /// Input bytes were delivered to the active terminal backend.
  InputWritten,
  /// The terminal backend stopped and reported its exit code.
  Exited {
    /// Code the backend reported for the program that ended.
    code: u32,
  },
}

/// Persistent focus, backend, and VT state for a rendered terminal emulator.
pub struct TerminalEmulatorState {
  session: TerminalSession,
  focus: FocusHandle,
  size: TerminalSize,
  metrics: TerminalMetrics,
  /// Message painted below the grid, describing a backend that failed or stopped.
  status: Option<String>,
  /// Exit code once the backend has stopped, which is also what ends its input.
  ended: Option<u32>,
  preedit: Option<TerminalPreedit>,
  /// Where the viewport sits in the scrollback, refreshed when events arrive.
  scroll_state: TerminalScrollState,
  /// Sub-line part of a pixel scroll, carried so a slow trackpad still moves.
  scroll_remainder: f32,
  /// The numbers the rendered scrollbar reads, and the offset it asks for back.
  scrollbar: TerminalScrollbarHandle,
  _event_task: Task<()>,
}

/// Composition text an input method is still editing.
///
/// The composition is deliberately kept out of the session. Writing half-finished
/// bytes to a PTY or an in-process shell would let the child program echo and
/// interpret them, so the text stays here and is painted over the grid until the
/// input method commits it.
#[derive(Clone, Debug)]
struct TerminalPreedit {
  /// Composition text the input method is currently showing.
  text: String,
  /// Grid row the composition started on.
  row: usize,
  /// Grid column the composition started at.
  column: usize,
}

impl TerminalEmulatorState {
  /// Creates state around an existing terminal session.
  pub fn new(session: TerminalSession, size: TerminalSize, cx: &mut Context<Self>) -> Self {
    let wakeups = session.event_wakeups();
    let event_task = cx.spawn(|this: WeakEntity<Self>, async_cx: &mut AsyncApp| {
      let mut async_cx = async_cx.clone();
      async move {
        while wakeups.recv().await.is_ok() {
          let should_continue = this
            .update(&mut async_cx, |state, cx| {
              state.process_events(cx);
              true
            })
            .unwrap_or(false);
          if !should_continue {
            break;
          }
        }
      }
    });
    let scroll_state = session.scroll_state();
    let this = Self {
      session,
      focus: cx.focus_handle(),
      size,
      metrics: TerminalMetrics {
        cell_width: px(1.0),
        cell_height: px(1.0),
        font_size: px(FONT_SIZE),
      },
      status: None,
      ended: None,
      preedit: None,
      scroll_state,
      scroll_remainder: 0.0,
      scrollbar: TerminalScrollbarHandle::default(),
      _event_task: event_task,
    };
    // The cell height is a placeholder until the first layout, but publishing here
    // leaves the bar and the viewport describing the same position from the start.
    this.publish_scrollbar_geometry();
    this
  }

  /// Hands the bar the numbers it reads for this viewport position.
  ///
  /// gpui-kit reads a scrollbar handle through `&self` methods that take no
  /// context, so the emulator pushes geometry into the shared cells instead of
  /// the bar reaching back into this entity.
  fn publish_scrollbar_geometry(&self) {
    self
      .scrollbar
      .publish(scrollbar_geometry(self.scroll_state, self.metrics.cell_height));
  }

  /// Returns the terminal backend profile.
  pub fn profile(&self) -> chitin_terminal::TerminalProfile {
    self.session.profile()
  }

  /// Returns the focus handle owned by the terminal surface.
  pub fn focus_handle(&self) -> &FocusHandle {
    &self.focus
  }

  /// Returns the input method composition currently painted over the grid.
  fn preedit(&self) -> Option<&TerminalPreedit> {
    self.preedit.as_ref()
  }

  /// Writes already encoded bytes to the active terminal backend.
  pub fn write(&self, bytes: &[u8]) -> Result<(), TerminalSessionError> {
    self.session.write(bytes)
  }

  fn process_events(&mut self, cx: &mut Context<Self>) {
    // Read before the events are drained: a child program entering or leaving the
    // alternate screen changes what the wheel means, and that is observed here rather
    // than at the next wheel event. The read is a few counters under the terminal lock.
    self.scroll_state = self.session.scroll_state();
    let events = self.session.drain_events();
    if events.is_empty() {
      return;
    }
    for event in events {
      // A backend that has already stopped has nothing further to report: what arrives
      // after its exit is a consequence of the shutdown, and must not overwrite the
      // exit status the surface is showing.
      if self.ended.is_some() {
        break;
      }
      match event {
        TerminalEvent::Exited(code) => self.end(code, cx),
        TerminalEvent::Error(error) => self.status = Some(error),
        TerminalEvent::Render | TerminalEvent::Title(_) | TerminalEvent::Bell => {}
      }
    }
    cx.notify();
  }

  /// Records a backend that stopped and hands its exit code to the host.
  ///
  /// The session itself stays in place. Whether a stopped backend should also take its
  /// session down depends on what else the host has open, so that decision belongs to
  /// the host and travels there as [`TerminalEmulatorEvent::Exited`].
  fn end(&mut self, code: u32, cx: &mut Context<Self>) {
    self.ended = Some(code);
    self.status = Some(format!("process exited with code {code}"));
    cx.emit(TerminalEmulatorEvent::Exited { code });
  }

  fn resize(&mut self, bounds: Bounds<Pixels>, metrics: TerminalMetrics) {
    // Kept so the input method can be pointed at the same cell geometry the grid is painted
    // with, which is finer than the whole-pixel cell size reported to the backend.
    self.metrics = metrics;
    // The bar measures in cells, so a new cell height moves it even when the grid
    // the backend was told about is unchanged.
    self.publish_scrollbar_geometry();
    let next = terminal_size_for_bounds(bounds, metrics);
    // A stopped backend has no grid left to resize, and the failure it would report for
    // one would replace the exit status the surface is showing.
    if next == self.size || self.ended.is_some() {
      return;
    }
    match self.session.resize(next) {
      Ok(()) => {
        self.size = next;
        // A resize reflows the grid and can shorten the history the viewport sits in,
        // which the backend answers by clamping the position it had.
        self.refresh_scroll_state();
        // The cell height the wheel was measured against has changed, so a fraction
        // carried over from before it no longer names the same distance.
        self.scroll_remainder = 0.0;
      }
      Err(error) => self.status = Some(error.to_string()),
    }
  }

  fn handle_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
    // Scrolling reaches back through whatever a stopped program left on screen, which
    // is exactly when its last screen is worth reading, and it writes no bytes for the
    // guards below to withhold. A live composition is the one case it waits for: the
    // composition is anchored to the row it captured when it started, so moving the
    // viewport would paint it over different content. The guard below then swallows
    // the keystroke, exactly as the wheel does.
    if self.preedit.is_none()
      && let Some(scroll) = scroll_keystroke(&event.keystroke)
    {
      self.scroll_viewport(scroll, cx);
      cx.stop_propagation();
      return;
    }
    // A stopped backend has nowhere to put input, but the surface still swallows the
    // keystroke so it cannot reach a window shortcut behind the frozen grid.
    if self.ended.is_some() {
      cx.stop_propagation();
      return;
    }
    // A live composition owns every keystroke: Enter commits it, Escape cancels it,
    // and the arrow keys pick a candidate. Forwarding those bytes would send them to
    // the child program as well, which is what the composition has not done yet.
    if self.preedit.is_some() {
      cx.stop_propagation();
      return;
    }
    let Some(bytes) = encode_key(event) else {
      return;
    };
    self.write_input(&bytes, cx);
    cx.stop_propagation();
  }

  /// Delivers encoded input to the active backend and repaints the surface.
  fn write_input(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
    // Committed composition text arrives here too, and a stopped backend would answer
    // both with an error rather than with the child program's own echo.
    if self.ended.is_some() {
      return;
    }
    // Input is aimed at the live edge. Echoing into a viewport that has been scrolled
    // back would put the child program's answer somewhere the user cannot see.
    if self.scroll_state.display_offset != 0 {
      self.session.scroll(TerminalScroll::Bottom);
      self.refresh_scroll_state();
    }
    match self.session.write(bytes) {
      Ok(()) => cx.emit(TerminalEmulatorEvent::InputWritten),
      Err(error) => self.status = Some(error.to_string()),
    }
    cx.notify();
  }

  /// Applies one wheel event to the viewport, or forwards it to a full-screen program.
  fn handle_scroll_wheel(&mut self, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
    // A live composition anchors to a row captured when it started, and scrolling
    // would paint it over different content. A stopped backend has nothing to scroll.
    if self.ended.is_some() || self.preedit.is_some() {
      cx.stop_propagation();
      return;
    }

    let lines = wheel_lines(&event.delta, self.metrics.cell_height, &mut self.scroll_remainder);
    if lines == 0 {
      cx.stop_propagation();
      return;
    }
    // A full-screen program keeps no scrollback of its own, so the wheel means to it
    // what the arrow keys mean: the only viewport motion it understands.
    if self.scroll_state.alt_screen {
      self.write_arrows(lines, cx);
    } else {
      self.scroll_viewport(TerminalScroll::Lines(lines), cx);
    }
    cx.stop_propagation();
  }

  /// Forwards wheel motion to a full-screen program as the arrow keys it reads.
  fn write_arrows(&mut self, lines: i32, cx: &mut Context<Self>) {
    let key = if lines > 0 { "up" } else { "down" };
    let Some(bytes) = encode_special_key(key, false) else {
      return;
    };
    // One arrow per whole line, capped so a trackpad fling cannot flood the child.
    for _ in 0..lines.unsigned_abs().min(MAX_WHEEL_LINES_PER_EVENT as u32) {
      self.write_input(bytes, cx);
    }
  }

  /// Moves the viewport and repaints the surface.
  fn scroll_viewport(&mut self, scroll: TerminalScroll, cx: &mut Context<Self>) {
    self.session.scroll(scroll);
    self.refresh_scroll_state();
    // The backend reports a viewport motion as a cursor-dirty event and coalesces it,
    // so the repaint is asked for here rather than waited for.
    cx.notify();
  }

  /// Moves the viewport to the position a scrollbar drag described.
  fn scroll_to_scrollbar_offset(&mut self, offset: Pixels, cx: &mut Context<Self>) {
    let display_offset =
      display_offset_for_scrollbar_offset(offset, self.metrics.cell_height, self.scroll_state.history_size);
    self.scroll_viewport(TerminalScroll::Offset(display_offset), cx);
  }

  /// Reads the viewport position back from the session.
  ///
  /// The wheel remainder is deliberately left alone: it is the sub-line part of a
  /// gesture still in progress, and clearing it here would drop that fraction every
  /// time the viewport crossed a whole line.
  fn refresh_scroll_state(&mut self) {
    self.scroll_state = self.session.scroll_state();
    self.publish_scrollbar_geometry();
  }
}

impl EventEmitter<TerminalEmulatorEvent> for TerminalEmulatorState {}

/// Routes platform text input, input method composition included, into the session.
///
/// A terminal owns no editable text buffer: committed text is written to the active
/// backend, and every glyph on the grid arrives back from that backend's own output.
/// The composition is the single piece of text that has to live here, because the
/// child program must not see it until the input method commits it.
impl EntityInputHandler for TerminalEmulatorState {
  fn text_for_range(
    &mut self,
    _: Range<usize>,
    _: &mut Option<Range<usize>>,
    _: &mut Window,
    _: &mut Context<Self>,
  ) -> Option<String> {
    // Nothing to hand back: the grid is the child program's output, not a buffer to read.
    None
  }

  fn selected_text_range(&mut self, _: bool, _: &mut Window, _: &mut Context<Self>) -> Option<UTF16Selection> {
    // A terminal holds no selection. Reporting a collapsed range still presents the
    // surface as a live text input target, which is what wakes the platform input method.
    Some(UTF16Selection {
      range: 0..0,
      reversed: false,
    })
  }

  fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
    self
      .preedit
      .as_ref()
      .map(|preedit| 0..preedit.text.encode_utf16().count())
  }

  fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
    if self.preedit.take().is_some() {
      cx.notify();
    }
  }

  fn replace_text_in_range(&mut self, _: Option<Range<usize>>, text: &str, _: &mut Window, cx: &mut Context<Self>) {
    // A commit ends the composition and hands the finished text to the child program,
    // exactly as if the keystrokes had been typed out one by one.
    self.preedit = None;
    if text.is_empty() {
      cx.notify();
      return;
    }
    self.write_input(text.as_bytes(), cx);
  }

  fn replace_and_mark_text_in_range(
    &mut self,
    _: Option<Range<usize>>,
    text: &str,
    _: Option<Range<usize>>,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    // An empty composition is how the platform reports a cancelled one.
    if text.is_empty() {
      if self.preedit.take().is_some() {
        cx.notify();
      }
      return;
    }
    // Keep the anchor captured when the composition started, so shell output arriving
    // mid-edit cannot drag the composition to another cell.
    let anchor = self
      .preedit
      .as_ref()
      .map(|preedit| (preedit.row, preedit.column))
      .or_else(|| self.session.snapshot().cursor)
      .unwrap_or((0, 0));
    self.preedit = Some(TerminalPreedit {
      text: text.to_string(),
      row: anchor.0,
      column: anchor.1,
    });
    // The platform holds no cursor of its own for this surface, so the candidate window
    // stays wherever it was last told until it is handed the composition anchor again.
    window.invalidate_character_coordinates();
    cx.notify();
  }

  fn bounds_for_range(
    &mut self,
    _: Range<usize>,
    element_bounds: Bounds<Pixels>,
    _: &mut Window,
    _: &mut Context<Self>,
  ) -> Option<Bounds<Pixels>> {
    // Places the input method's candidate window over the composition anchor instead of
    // leaving it pinned to the corner of the window.
    let (row, column) = self
      .preedit
      .as_ref()
      .map(|preedit| (preedit.row, preedit.column))
      .or_else(|| self.session.snapshot().cursor)?;
    let cell_width = self.metrics.cell_width;
    let cell_height = self.metrics.cell_height;
    Some(Bounds::new(
      point(
        element_bounds.left() + cell_width * column as f32,
        element_bounds.top() + cell_height * row as f32,
      ),
      size(cell_width, cell_height),
    ))
  }

  fn character_index_for_point(&mut self, _: Point<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
    // The terminal hit-tests its own pointer input, so there is no index to report.
    None
  }

  fn accepts_text_input(&self, _: &mut Window, _: &mut Context<Self>) -> bool {
    true
  }
}

/// Interactive GPUI surface for either a PTY or in-process terminal backend.
#[derive(IntoElement)]
pub struct TerminalEmulator {
  state: Entity<TerminalEmulatorState>,
  theme: ThemeColor,
  font_family: SharedString,
}

impl TerminalEmulator {
  /// Creates a terminal emulator bound to persistent session state.
  pub fn new(state: Entity<TerminalEmulatorState>) -> Self {
    Self {
      state,
      theme: *ThemeColor::dark(),
      font_family: "monospace".into(),
    }
  }

  /// Sets the semantic UI theme.
  pub fn theme(mut self, theme: ThemeColor) -> Self {
    self.theme = theme;
    self
  }

  /// Sets the upright monospace font used by the character grid.
  pub fn font_family(mut self, font_family: impl Into<SharedString>) -> Self {
    self.font_family = font_family.into();
    self
  }
}

impl RenderOnce for TerminalEmulator {
  fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
    let metrics = measure_terminal_metrics(window, self.font_family.clone());
    let status_font = self.font_family.clone();
    let status = self.state.read(cx).status.clone();
    let preedit = self.state.read(cx).preedit().cloned();
    let mut snapshot = self.state.read(cx).session.snapshot();
    // A stopped backend leaves its last frame on the grid, so the cursor is what
    // separates a session that has ended from one that is merely waiting.
    if self.state.read(cx).ended.is_some() {
      snapshot.cursor = None;
    }
    let focus = self.state.read(cx).focus.clone();
    let scrollbar = self.state.read(cx).scrollbar.clone();
    // A drag or a track click on the bar leaves an offset behind rather than calling
    // into this entity, because gpui-kit's handle is reached through `&self` methods
    // that carry no context. The repaint the bar asks for is what brings it here.
    if let Some(offset) = scrollbar.take_requested() {
      self
        .state
        .update(cx, |state, cx| state.scroll_to_scrollbar_offset(offset, cx));
    }
    let state_for_mouse = self.state.clone();
    let state_for_keys = self.state.clone();
    let state_for_layout = self.state.clone();
    let state_for_scroll = self.state.clone();
    let theme = self.theme;
    let rows = (0..snapshot.rows).map(move |row| {
      let cells = build_row_runs(&snapshot, row)
        .into_iter()
        .map(move |run| render_run(run, metrics, theme));
      let grid_row = div().flex().h(metrics.cell_height).whitespace_nowrap().children(cells);
      // The composition belongs to exactly one row, and only that row becomes a
      // containing block for its overlay.
      match preedit.as_ref().filter(|preedit| preedit.row == row) {
        Some(preedit) => grid_row.relative().child(render_preedit(preedit, metrics, theme)),
        None => grid_row,
      }
    });

    let grid = div()
      .flex()
      .flex_col()
      .size_full()
      .overflow_hidden()
      .track_focus(&focus)
      .on_mouse_down(MouseButton::Left, move |_, window, cx| {
        let focus = state_for_mouse.read(cx).focus.clone();
        window.focus(&focus, cx);
      })
      .on_key_down(move |event, _, cx| {
        state_for_keys.update(cx, |state, cx| state.handle_key(event, cx));
      })
      .on_children_prepainted(move |bounds, _, cx| {
        let Some(bounds) = bounds.first().copied() else {
          return;
        };
        state_for_layout.update(cx, |state, _| state.resize(bounds, metrics));
      })
      .bg(theme.background)
      .font(terminal_font(self.font_family))
      .text_size(metrics.font_size)
      .line_height(metrics.cell_height)
      .child(div().flex().flex_col().size_full().children(rows));

    // The grid keeps the whole surface to itself until the backend has something to
    // report, so the input method anchor and the painted grid stay on the same bounds.
    // The wheel listener sits on this outer surface rather than on the grid so that it
    // also covers the scrollbar, whose lane is a sibling of the surface below.
    div()
      .flex()
      .flex_col()
      .size_full()
      .on_scroll_wheel(move |event, _, cx| {
        state_for_scroll.update(cx, |state, cx| state.handle_scroll_wheel(event, cx));
      })
      .child(
        div()
          .relative()
          .flex_1()
          .min_h_0()
          .child(TerminalSurface {
            content: grid.into_any_element(),
            state: self.state,
          })
          // The bar is an overlay rather than a column of its own, so gaining and
          // losing scrollback cannot change the width the backend is laid out for.
          // It takes the sibling container's bounds as the viewport it moves through,
          // which is the same region the surface above paints into.
          .child(
            Scrollbar::vertical(&scrollbar)
              .viewport_from_layout()
              .mode(ScrollbarMode::Always),
          ),
      )
      .when_some(status, move |surface, status| {
        surface.child(render_status(&status, metrics, theme, status_font))
      })
  }
}

/// Paints a backend's final or failed state below the grid.
fn render_status(
  status: &str,
  metrics: TerminalMetrics,
  theme: ThemeColor,
  font_family: SharedString,
) -> impl IntoElement {
  div()
    .flex_none()
    .w_full()
    .h(metrics.cell_height)
    .px(metrics.cell_width)
    .border_t_1()
    .border_color(theme.border)
    .whitespace_nowrap()
    .overflow_hidden()
    .font(terminal_font(font_family))
    .text_size(metrics.font_size)
    .text_color(theme.muted_foreground)
    .child(status.to_owned())
}

/// Wraps the terminal grid so the surface can register itself as a text input target.
///
/// GPUI enables the platform input method only for the element that calls
/// [`Window::handle_input`], and only during paint. A `Div` has no paint-phase hook, so
/// this element delegates the entire layout, prepaint, and paint pass to the grid and
/// then registers the handler against the grid's own bounds.
struct TerminalSurface {
  content: AnyElement,
  state: Entity<TerminalEmulatorState>,
}

impl IntoElement for TerminalSurface {
  type Element = Self;

  fn into_element(self) -> Self::Element {
    self
  }
}

impl Element for TerminalSurface {
  type RequestLayoutState = ();
  type PrepaintState = ();

  fn id(&self) -> Option<ElementId> {
    None
  }

  fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
    None
  }

  fn request_layout(
    &mut self,
    id: Option<&GlobalElementId>,
    inspector_id: Option<&InspectorElementId>,
    window: &mut Window,
    cx: &mut App,
  ) -> (LayoutId, Self::RequestLayoutState) {
    Element::request_layout(&mut self.content, id, inspector_id, window, cx)
  }

  fn prepaint(
    &mut self,
    id: Option<&GlobalElementId>,
    inspector_id: Option<&InspectorElementId>,
    bounds: Bounds<Pixels>,
    request_layout: &mut Self::RequestLayoutState,
    window: &mut Window,
    cx: &mut App,
  ) -> Self::PrepaintState {
    Element::prepaint(&mut self.content, id, inspector_id, bounds, request_layout, window, cx)
  }

  fn paint(
    &mut self,
    id: Option<&GlobalElementId>,
    inspector_id: Option<&InspectorElementId>,
    bounds: Bounds<Pixels>,
    request_layout: &mut Self::RequestLayoutState,
    prepaint: &mut Self::PrepaintState,
    window: &mut Window,
    cx: &mut App,
  ) {
    Element::paint(
      &mut self.content,
      id,
      inspector_id,
      bounds,
      request_layout,
      prepaint,
      window,
      cx,
    );
    // Registering a handler is also what turns the platform input method on; GPUI
    // disables it for any frame that registers none. Losing focus therefore cancels a
    // composition through the platform's own `unmark_text` path.
    let focus = self.state.read(cx).focus.clone();
    window.handle_input(&focus, ElementInputHandler::new(bounds, self.state.clone()), cx);
  }
}

/// Paints the active input method composition over the grid at its anchor cell.
///
/// The composition is not part of the grid, so it covers the cells underneath instead of
/// replacing them. It disappears the moment the input method commits, at which point the
/// child program echoes the committed glyphs itself.
fn render_preedit(preedit: &TerminalPreedit, metrics: TerminalMetrics, theme: ThemeColor) -> Div {
  div()
    .absolute()
    .top_0()
    .left(metrics.cell_width * preedit.column as f32)
    .h(metrics.cell_height)
    .whitespace_nowrap()
    .bg(theme.background)
    .text_color(theme.foreground)
    .underline()
    .child(preedit.text.clone())
}

#[derive(Clone, Copy)]
struct TerminalMetrics {
  cell_width: Pixels,
  cell_height: Pixels,
  font_size: Pixels,
}

fn measure_terminal_metrics(window: &mut Window, family: SharedString) -> TerminalMetrics {
  let font_size = px(FONT_SIZE);
  let regular_width = measure_cell_width(window, family.clone(), font_size, FontWeight::NORMAL);
  let bold_width = measure_cell_width(window, family, font_size, FontWeight::BOLD);
  TerminalMetrics {
    cell_width: regular_width.max(bold_width).max(px(1.0)),
    cell_height: px((FONT_SIZE * LINE_HEIGHT_MULTIPLIER).round()).max(px(1.0)),
    font_size,
  }
}

fn measure_cell_width(window: &mut Window, family: SharedString, font_size: Pixels, weight: FontWeight) -> Pixels {
  let mut terminal_font = terminal_font(family);
  terminal_font.weight = weight;
  window
    .text_system()
    .shape_line(
      SharedString::new_static("M"),
      font_size,
      &[TextRun {
        len: 1,
        font: terminal_font,
        color: Hsla::default().into(),
        ..TextRun::default()
      }],
      None,
    )
    .width
}

fn terminal_size_for_bounds(bounds: Bounds<Pixels>, metrics: TerminalMetrics) -> TerminalSize {
  let cell_width = f32::from(metrics.cell_width);
  let cell_height = f32::from(metrics.cell_height);
  TerminalSize::new(
    (f32::from(bounds.size.width) / cell_width).floor().max(1.0) as usize,
    (f32::from(bounds.size.height) / cell_height).floor().max(1.0) as usize,
    cell_width.ceil() as u16,
    cell_height.ceil() as u16,
  )
}

fn terminal_font(family: SharedString) -> Font {
  let mut terminal_font = font(family);
  terminal_font.features = FontFeatures(Arc::new(vec![
    ("calt".into(), 0),
    ("liga".into(), 0),
    ("clig".into(), 0),
  ]));
  terminal_font.fallbacks = Some(FontFallbacks::from_fonts(vec![
    "Symbols Nerd Font".into(),
    "Noto Color Emoji".into(),
    "Segoe UI Emoji".into(),
    "Apple Color Emoji".into(),
    "Noto Sans Symbols 2".into(),
  ]));
  terminal_font
}

struct TerminalRun {
  text: String,
  columns: usize,
  foreground: TerminalColor,
  background: TerminalColor,
  attributes: TerminalCellAttributes,
  /// Whether the run holds a single glyph that is painted across two grid columns.
  wide: bool,
  cursor: bool,
}

impl TerminalRun {
  fn new(cell: &TerminalCell, cursor: bool) -> Self {
    let mut run = Self {
      text: String::new(),
      columns: 0,
      foreground: cell.foreground,
      background: cell.background,
      attributes: run_attributes(cell.attributes),
      wide: cell.attributes.wide,
      cursor,
    };
    run.push(cell);
    run
  }

  fn matches(&self, cell: &TerminalCell, cursor: bool) -> bool {
    self.foreground == cell.foreground
      && self.background == cell.background
      && self.attributes == run_attributes(cell.attributes)
      && self.wide == cell.attributes.wide
      && self.cursor == cursor
  }

  fn push(&mut self, cell: &TerminalCell) {
    // A wide cell is the only cell of its run, and it is the one that accounts for both of
    // the columns the glyph covers.
    self.columns += if cell.attributes.wide { 2 } else { 1 };
    if cell.attributes.hidden {
      self.text.push(' ');
      return;
    }
    self.text.push(cell.character);
    self.text.extend(cell.combining_characters.iter());
  }
}

fn build_row_runs(snapshot: &TerminalSnapshot, row: usize) -> Vec<TerminalRun> {
  let mut runs: Vec<TerminalRun> = Vec::new();
  let visible_columns = (0..snapshot.columns)
    .rfind(|&column| {
      snapshot.cursor == Some((row, column)) || snapshot.cell(row, column).is_some_and(|cell| !is_default_blank(cell))
    })
    .map_or(0, |column| column + 1);
  for column in 0..visible_columns {
    let Some(cell) = snapshot.cell(row, column) else {
      continue;
    };
    // The cell that continues a wide glyph is a placeholder: the wide glyph's own run
    // already covers its column, and painting it as a space would cover the right half of
    // the glyph that precedes it.
    if cell.attributes.wide_spacer {
      continue;
    }
    let cursor = snapshot.cursor == Some((row, column));
    // A wide glyph is placed by the grid rather than by the text flow, because its advance
    // is not exactly two cell widths. Keeping it in a run of its own leaves every following
    // run starting exactly where the grid says the next column starts.
    let extended = runs.last_mut().filter(|run| !run.wide && run.matches(cell, cursor));
    match extended {
      Some(run) => run.push(cell),
      None => runs.push(TerminalRun::new(cell, cursor)),
    }
  }
  runs
}

fn is_default_blank(cell: &TerminalCell) -> bool {
  cell.character == ' '
    && cell.combining_characters.is_empty()
    && cell.foreground == TerminalColor::Named(TerminalNamedColor::Foreground)
    && cell.background == TerminalColor::Named(TerminalNamedColor::Background)
    && upright_attributes(cell.attributes) == TerminalCellAttributes::default()
}

fn upright_attributes(mut attributes: TerminalCellAttributes) -> TerminalCellAttributes {
  attributes.italic = false;
  attributes
}

/// Attributes that decide where one painted run ends and the next one begins.
///
/// Slant is dropped because the grid font ships upright only, and the wide-continuation
/// marker is dropped because it describes the cell's role in the grid rather than how the
/// cell looks.
fn run_attributes(mut attributes: TerminalCellAttributes) -> TerminalCellAttributes {
  attributes.italic = false;
  attributes.wide_spacer = false;
  attributes
}

fn render_run(run: TerminalRun, metrics: TerminalMetrics, theme: ThemeColor) -> impl IntoElement {
  let (mut foreground, mut background) = (
    resolve_color(run.foreground, theme),
    resolve_color(run.background, theme),
  );
  if run.attributes.inverse {
    std::mem::swap(&mut foreground, &mut background);
  }
  if run.attributes.dim {
    foreground = foreground.opacity(0.62);
  }
  if run.cursor {
    std::mem::swap(&mut foreground, &mut background);
  }
  div()
    .flex_none()
    .w(metrics.cell_width * run.columns as f32)
    .h(metrics.cell_height)
    .whitespace_nowrap()
    .bg(background)
    .text_color(foreground)
    .when(run.attributes.bold, |element| element.font_weight(FontWeight::BOLD))
    .when(run.attributes.underline, |element| element.underline())
    .child(run.text)
}

fn resolve_color(color: TerminalColor, theme: ThemeColor) -> Hsla {
  match color {
    TerminalColor::Rgb { red, green, blue } => {
      rgb((u32::from(red) << 16) | (u32::from(green) << 8) | u32::from(blue)).into()
    }
    TerminalColor::Indexed(index) => indexed_color(index),
    TerminalColor::Named(TerminalNamedColor::Foreground) => theme.foreground,
    TerminalColor::Named(TerminalNamedColor::Background) => theme.background,
    TerminalColor::Named(named) => named_color(named),
  }
}

fn named_color(color: TerminalNamedColor) -> Hsla {
  let value = match color {
    TerminalNamedColor::Black | TerminalNamedColor::DimBlack => 0x1f2329,
    TerminalNamedColor::Red | TerminalNamedColor::DimRed => 0xff5f56,
    TerminalNamedColor::Green | TerminalNamedColor::DimGreen => 0x63d17c,
    TerminalNamedColor::Yellow | TerminalNamedColor::DimYellow => 0xe5c07b,
    TerminalNamedColor::Blue | TerminalNamedColor::DimBlue => 0x61afef,
    TerminalNamedColor::Magenta | TerminalNamedColor::DimMagenta => 0xc678dd,
    TerminalNamedColor::Cyan | TerminalNamedColor::DimCyan => 0x56b6c2,
    TerminalNamedColor::White | TerminalNamedColor::DimWhite => 0xabb2bf,
    TerminalNamedColor::BrightBlack => 0x5c6370,
    TerminalNamedColor::BrightRed => 0xff7b72,
    TerminalNamedColor::BrightGreen => 0x7ee787,
    TerminalNamedColor::BrightYellow => 0xf2cc60,
    TerminalNamedColor::BrightBlue => 0x79c0ff,
    TerminalNamedColor::BrightMagenta => 0xd2a8ff,
    TerminalNamedColor::BrightCyan => 0x76e3ea,
    TerminalNamedColor::BrightWhite | TerminalNamedColor::BrightForeground => 0xffffff,
    TerminalNamedColor::Foreground | TerminalNamedColor::Cursor => 0xd0d7de,
    TerminalNamedColor::Background => 0x101216,
    TerminalNamedColor::DimForeground => 0x8b949e,
  };
  rgb(value).into()
}

fn indexed_color(index: u8) -> Hsla {
  const ANSI: [u32; 16] = [
    0x1f2329, 0xff5f56, 0x63d17c, 0xe5c07b, 0x61afef, 0xc678dd, 0x56b6c2, 0xabb2bf, 0x5c6370, 0xff7b72, 0x7ee787,
    0xf2cc60, 0x79c0ff, 0xd2a8ff, 0x76e3ea, 0xffffff,
  ];
  let value = match index {
    0..=15 => ANSI[index as usize],
    16..=231 => {
      let cube = index - 16;
      let component = |value: u8| if value == 0 { 0 } else { 55 + value * 40 };
      let red = component(cube / 36);
      let green = component((cube % 36) / 6);
      let blue = component(cube % 6);
      (u32::from(red) << 16) | (u32::from(green) << 8) | u32::from(blue)
    }
    232..=255 => {
      let gray = 8 + (index - 232) * 10;
      (u32::from(gray) << 16) | (u32::from(gray) << 8) | u32::from(gray)
    }
  };
  rgb(value).into()
}

/// Returns the bytes a terminal program expects for a named special key.
///
/// The table is shared by key encoding and by the wheel, which forwards motion to a
/// full-screen program as arrow keys, so the two cannot disagree about what an arrow is.
fn encode_special_key(key: &str, shift: bool) -> Option<&'static [u8]> {
  match key {
    "enter" => Some(b"\r"),
    "backspace" => Some(b"\x7f"),
    "tab" if shift => Some(b"\x1b[Z"),
    "tab" => Some(b"\t"),
    "escape" => Some(b"\x1b"),
    "up" => Some(b"\x1b[A"),
    "down" => Some(b"\x1b[B"),
    "right" => Some(b"\x1b[C"),
    "left" => Some(b"\x1b[D"),
    "home" => Some(b"\x1b[H"),
    "end" => Some(b"\x1b[F"),
    "pageup" => Some(b"\x1b[5~"),
    "pagedown" => Some(b"\x1b[6~"),
    "delete" => Some(b"\x1b[3~"),
    "insert" => Some(b"\x1b[2~"),
    _ => None,
  }
}

/// Returns the viewport motion a keystroke asks for.
///
/// Only the shifted forms scroll. Unmodified keys have to keep reaching the child
/// program as the bytes it expects, and the combinations that select text elsewhere
/// are left alone rather than guessed at.
fn scroll_keystroke(keystroke: &Keystroke) -> Option<TerminalScroll> {
  let modifiers = keystroke.modifiers;
  if !modifiers.shift || modifiers.control || modifiers.alt || modifiers.platform {
    return None;
  }

  match keystroke.key.as_str() {
    "pageup" => Some(TerminalScroll::PageUp),
    "pagedown" => Some(TerminalScroll::PageDown),
    "home" => Some(TerminalScroll::Top),
    "end" => Some(TerminalScroll::Bottom),
    _ => None,
  }
}

/// Converts one wheel event into whole lines of viewport motion.
///
/// # Parameters
///
/// * `delta` is the platform's scroll delta for this event.
/// * `cell_height` measures a pixel delta in lines.
/// * `remainder` carries the sub-line part of earlier events. A precision trackpad
///   reports fractions of a line per event, and discarding each one separately would
///   make a slow gesture scroll nothing at all.
///
/// # Returns
///
/// Whole lines to scroll, positive toward older output, capped at
/// [`MAX_WHEEL_LINES_PER_EVENT`]. Zero when this event only accumulated a fraction.
fn wheel_lines(delta: &ScrollDelta, cell_height: Pixels, remainder: &mut f32) -> i32 {
  let lines = match delta {
    ScrollDelta::Lines(lines) => lines.y,
    ScrollDelta::Pixels(pixels) => f32::from(pixels.y) / f32::from(cell_height.max(px(1.0))),
  };

  *remainder += lines;
  let whole = remainder.trunc();
  *remainder -= whole;
  (whole as i32).clamp(-MAX_WHEEL_LINES_PER_EVENT, MAX_WHEEL_LINES_PER_EVENT)
}

/// Describes a viewport position in scrollbar geometry.
///
/// The bar puts the oldest output at the top of the content and the live edge at the
/// bottom, and it measures downward from zero the way GPUI scroll offsets do.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct TerminalScrollbarGeometry {
  /// Total height of the retained scrollback plus the screen.
  content_height: Pixels,
  /// Height of the visible screen.
  viewport_height: Pixels,
  /// Distance from the top of the content to the top of the viewport, negative downward.
  offset: Pixels,
}

fn scrollbar_geometry(scroll: TerminalScrollState, cell_height: Pixels) -> TerminalScrollbarGeometry {
  TerminalScrollbarGeometry {
    viewport_height: cell_height * scroll.screen_lines as f32,
    content_height: cell_height * (scroll.screen_lines + scroll.history_size) as f32,
    offset: -(cell_height * scroll.lines_above() as f32),
  }
}

/// The terminal's [`ScrollbarHandle`], shared between the emulator and the bar.
///
/// The bar is an element of its own: it reads its handle during prepaint and writes
/// an offset back from a pointer handler, neither of which carries a context and so
/// neither of which can reach the emulator's entity. Both directions therefore pass
/// through the cells here, and the emulator empties the request on its next render.
#[derive(Clone, Default)]
struct TerminalScrollbarHandle {
  geometry: Rc<Cell<TerminalScrollbarGeometry>>,
  /// An offset the bar asked the viewport to move to, not yet applied.
  requested: Rc<Cell<Option<Pixels>>>,
}

impl TerminalScrollbarHandle {
  /// Publishes the geometry of the viewport the bar describes.
  fn publish(&self, geometry: TerminalScrollbarGeometry) {
    self.geometry.set(geometry);
  }

  /// Takes the pending viewport motion, if the bar requested one.
  fn take_requested(&self) -> Option<Pixels> {
    self.requested.take()
  }
}

impl ScrollbarHandle for TerminalScrollbarHandle {
  fn viewport_bounds(&self) -> Bounds<Pixels> {
    let geometry = self.geometry.get();
    // The bar sits in the same container as the surface it scrolls and takes that
    // container's layout bounds, so only the height is ever read from here.
    Bounds::new(point(px(0.0), px(0.0)), size(px(0.0), geometry.viewport_height))
  }

  fn offset(&self) -> Point<Pixels> {
    point(px(0.0), self.geometry.get().offset)
  }

  fn set_offset(&self, offset: Point<Pixels>) {
    // The local copy moves the thumb on the frame the pointer asked for it; the
    // request is what the emulator reads to move the viewport itself.
    let mut geometry = self.geometry.get();
    geometry.offset = offset.y;
    self.geometry.set(geometry);
    self.requested.set(Some(offset.y));
  }

  fn content_size(&self) -> Size<Pixels> {
    let geometry = self.geometry.get();
    size(px(0.0), geometry.content_height)
  }
}

/// Converts a position on the scrollbar back into a viewport display offset.
///
/// # Parameters
///
/// * `offset` is the bar's distance from the top of the content, negative downward.
/// * `cell_height` measures that distance in lines.
/// * `history_size` bounds the result to the lines that are actually retained.
///
/// # Returns
///
/// The number of lines the viewport should sit above the live edge.
fn display_offset_for_scrollbar_offset(offset: Pixels, cell_height: Pixels, history_size: usize) -> usize {
  let lines = f32::from(offset) / f32::from(cell_height.max(px(1.0)));
  let display_offset = history_size as f32 + lines.round();

  display_offset.clamp(0.0, history_size as f32) as usize
}

fn encode_key(event: &KeyDownEvent) -> Option<Vec<u8>> {
  let key = event.keystroke.key.as_str();
  let modifiers = event.keystroke.modifiers;
  if let Some(special) = encode_special_key(key, modifiers.shift) {
    return Some(special.to_vec());
  }
  if modifiers.platform {
    return None;
  }
  let text = event.keystroke.key_char.as_deref()?;
  let mut bytes = if modifiers.control && text.len() == 1 {
    let byte = text.as_bytes()[0].to_ascii_uppercase();
    if byte.is_ascii_uppercase() {
      vec![byte & 0x1f]
    } else {
      return None;
    }
  } else {
    text.as_bytes().to_vec()
  };
  if modifiers.alt {
    bytes.insert(0, 0x1b);
  }
  Some(bytes)
}

#[cfg(test)]
mod tests {
  use super::*;

  fn narrow(character: char) -> TerminalCell {
    TerminalCell {
      character,
      ..TerminalCell::default()
    }
  }

  fn wide(character: char) -> TerminalCell {
    TerminalCell {
      character,
      attributes: TerminalCellAttributes {
        wide: true,
        ..TerminalCellAttributes::default()
      },
      ..TerminalCell::default()
    }
  }

  fn continuation() -> TerminalCell {
    TerminalCell {
      attributes: TerminalCellAttributes {
        wide_spacer: true,
        ..TerminalCellAttributes::default()
      },
      ..TerminalCell::default()
    }
  }

  fn hidden(character: char) -> TerminalCell {
    TerminalCell {
      character,
      attributes: TerminalCellAttributes {
        hidden: true,
        ..TerminalCellAttributes::default()
      },
      ..TerminalCell::default()
    }
  }

  fn row_of(cells: Vec<TerminalCell>) -> TerminalSnapshot {
    TerminalSnapshot {
      columns: cells.len(),
      rows: 1,
      cells,
      cursor: None,
      display_offset: 0,
      history_size: 0,
    }
  }

  #[test]
  fn wide_character_should_span_two_columns_and_hide_its_continuation() {
    let runs = build_row_runs(&row_of(vec![wide('中'), continuation()]), 0);

    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].columns, 2);
    assert_eq!(runs[0].text, "中");
  }

  #[test]
  fn text_after_a_wide_character_should_start_at_its_own_grid_column() {
    let runs = build_row_runs(&row_of(vec![wide('中'), continuation(), narrow('a'), narrow('b')]), 0);

    assert_eq!(runs.len(), 2);
    assert_eq!((runs[0].columns, runs[0].text.as_str()), (2, "中"));
    assert_eq!((runs[1].columns, runs[1].text.as_str()), (2, "ab"));
  }

  #[test]
  fn adjacent_wide_characters_should_not_share_a_run() {
    let runs = build_row_runs(&row_of(vec![wide('中'), continuation(), wide('文'), continuation()]), 0);

    assert_eq!(runs.len(), 2);
    assert!(runs.iter().all(|run| run.wide && run.columns == 2));
    assert_eq!(runs[1].text, "文");
  }

  #[test]
  fn hidden_text_should_keep_its_columns_without_revealing_its_glyphs() {
    let runs = build_row_runs(&row_of(vec![narrow('a'), hidden('x'), narrow('b')]), 0);

    assert_eq!(
      runs.iter().map(|run| run.text.as_str()).collect::<Vec<_>>(),
      ["a", " ", "b"]
    );
    assert_eq!(runs.iter().map(|run| run.columns).sum::<usize>(), 3);
  }

  /// Builds a viewport position for the scrollbar helper tests.
  fn scroll_state(history: usize, offset: usize, lines: usize) -> TerminalScrollState {
    TerminalScrollState {
      history_size: history,
      display_offset: offset,
      screen_lines: lines,
      alt_screen: false,
    }
  }

  #[test]
  fn a_wheel_tick_toward_the_top_should_reveal_older_lines() {
    let mut remainder = 0.0;

    let lines = wheel_lines(&ScrollDelta::Lines(point(0.0, 1.0)), px(20.0), &mut remainder);

    assert_eq!(lines, 1);
  }

  #[test]
  fn a_wheel_tick_toward_the_bottom_should_move_toward_the_live_edge() {
    let mut remainder = 0.0;

    let lines = wheel_lines(&ScrollDelta::Lines(point(0.0, -1.0)), px(20.0), &mut remainder);

    assert_eq!(lines, -1);
  }

  #[test]
  fn pixel_scrolling_should_accumulate_until_a_whole_line_is_reached() {
    let mut remainder = 0.0;
    // Quarters rather than thirds: a third of a line is not exactly representable,
    // and three of them sum to just under one.
    let quarter = ScrollDelta::Pixels(point(px(0.0), px(5.0)));

    let steps = (0..4)
      .map(|_| wheel_lines(&quarter, px(20.0), &mut remainder))
      .collect::<Vec<_>>();

    // Four quarters of a line must scroll once between them, not four times zero.
    assert_eq!(steps, [0, 0, 0, 1]);
  }

  #[test]
  fn a_pixel_delta_smaller_than_a_line_should_not_scroll_yet() {
    let mut remainder = 0.0;

    let lines = wheel_lines(&ScrollDelta::Pixels(point(px(0.0), px(2.0))), px(20.0), &mut remainder);

    assert_eq!(lines, 0);
    assert!(
      remainder > 0.0,
      "the unused part of the delta should carry to the next event"
    );
  }

  #[test]
  fn a_wheel_event_should_not_scroll_more_than_the_per_event_cap() {
    let mut remainder = 0.0;

    let lines = wheel_lines(
      &ScrollDelta::Pixels(point(px(0.0), px(10_000.0))),
      px(20.0),
      &mut remainder,
    );

    assert_eq!(lines, MAX_WHEEL_LINES_PER_EVENT);
  }

  #[test]
  fn a_scroll_keystroke_should_require_shift_and_no_other_modifier() {
    let plain = Keystroke {
      key: "pageup".into(),
      ..Keystroke::default()
    };
    let shifted = Keystroke {
      modifiers: gpui::Modifiers {
        shift: true,
        ..gpui::Modifiers::default()
      },
      key: "pageup".into(),
      ..Keystroke::default()
    };

    assert_eq!(scroll_keystroke(&plain), None);
    assert_eq!(scroll_keystroke(&shifted), Some(TerminalScroll::PageUp));
  }

  #[test]
  fn a_scroll_keystroke_should_map_the_viewport_motions() {
    let keystroke = |key: &'static str| Keystroke {
      modifiers: gpui::Modifiers {
        shift: true,
        ..gpui::Modifiers::default()
      },
      key: key.into(),
      ..Keystroke::default()
    };

    assert_eq!(scroll_keystroke(&keystroke("pageup")), Some(TerminalScroll::PageUp));
    assert_eq!(scroll_keystroke(&keystroke("pagedown")), Some(TerminalScroll::PageDown));
    assert_eq!(scroll_keystroke(&keystroke("home")), Some(TerminalScroll::Top));
    assert_eq!(scroll_keystroke(&keystroke("end")), Some(TerminalScroll::Bottom));
    assert_eq!(scroll_keystroke(&keystroke("a")), None);
  }

  #[test]
  fn an_unmodified_page_key_should_still_reach_the_child_program() {
    // The scroll branch runs before key encoding, so it must decline the keystrokes
    // that the program is waiting for.
    let keystroke = Keystroke {
      key: "pageup".into(),
      ..Keystroke::default()
    };

    assert_eq!(scroll_keystroke(&keystroke), None);
    assert_eq!(encode_special_key("pageup", false), Some(b"\x1b[5~".as_slice()));
  }

  #[test]
  fn a_wheel_arrow_should_match_the_arrow_key_encoding() {
    // The wheel forwards motion to a full-screen program as arrow keys, so the bytes
    // it sends and the bytes the arrow keys send have to be the same ones.
    assert_eq!(encode_special_key("up", false), Some(b"\x1b[A".as_slice()));
    assert_eq!(encode_special_key("down", false), Some(b"\x1b[B".as_slice()));
  }

  #[test]
  fn a_shifted_tab_should_still_encode_as_a_back_tab() {
    assert_eq!(encode_special_key("tab", true), Some(b"\x1b[Z".as_slice()));
    assert_eq!(encode_special_key("tab", false), Some(b"\t".as_slice()));
  }

  #[test]
  fn the_scrollbar_should_measure_from_the_oldest_line_to_the_live_edge() {
    let geometry = scrollbar_geometry(scroll_state(100, 0, 24), px(20.0));

    // At the live edge the viewport sits at the bottom of the content.
    assert_eq!(geometry.viewport_height, px(480.0));
    assert_eq!(geometry.content_height, px(2480.0));
    assert_eq!(geometry.offset, px(-2000.0));
    assert_eq!(geometry.offset, geometry.viewport_height - geometry.content_height);
  }

  #[test]
  fn the_scrollbar_should_sit_at_the_top_when_scrolled_all_the_way_back() {
    let geometry = scrollbar_geometry(scroll_state(100, 100, 24), px(20.0));

    assert_eq!(geometry.offset, Pixels::ZERO);
  }

  #[test]
  fn a_published_geometry_should_reach_the_bar_through_its_handle() {
    let handle = TerminalScrollbarHandle::default();
    handle.publish(scrollbar_geometry(scroll_state(100, 37, 24), px(20.0)));

    assert_eq!(handle.offset().y, px(-1260.0));
    assert_eq!(handle.content_size().height, px(2480.0));
    assert_eq!(handle.viewport_bounds().size.height, px(480.0));
  }

  #[test]
  fn an_offset_the_bar_asked_for_should_survive_until_it_is_taken() {
    let handle = TerminalScrollbarHandle::default();

    assert_eq!(handle.take_requested(), None);
    handle.set_offset(point(px(0.0), px(-640.0)));
    assert_eq!(handle.take_requested(), Some(px(-640.0)));
    // Taking it clears it, so one drag lands as one motion rather than one per frame.
    assert_eq!(handle.take_requested(), None);
  }

  #[test]
  fn a_scrollbar_offset_should_round_trip_through_the_display_offset() {
    for display_offset in [0, 1, 37, 100] {
      let geometry = scrollbar_geometry(scroll_state(100, display_offset, 24), px(20.0));

      assert_eq!(
        display_offset_for_scrollbar_offset(geometry.offset, px(20.0), 100),
        display_offset,
      );
    }
  }

  #[test]
  fn a_display_offset_should_stay_inside_the_retained_history() {
    // The bar measures downward from the oldest line, so an offset above the top
    // of the content asks for more history than is retained and lands on the
    // oldest line, and one below the bottom lands on the live edge.
    assert_eq!(display_offset_for_scrollbar_offset(px(5_000.0), px(20.0), 100), 100);
    assert_eq!(display_offset_for_scrollbar_offset(px(-5_000.0), px(20.0), 100), 0);
  }
}
