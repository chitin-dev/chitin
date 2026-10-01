#![forbid(unsafe_code)]
//! Native PTY and VT-emulation integration example.
//!
//! Run with `cargo run -p chitin-desktop --example terminal-debug -- [WORKING_DIRECTORY]`.

use std::{path::PathBuf, sync::Arc, time::Duration};

use chitin_desktop::fonts::{TERMINAL_FONT_FAMILY, register_terminal_fonts};
use chitin_terminal::{
  TerminalCell, TerminalCellAttributes, TerminalColor, TerminalEvent, TerminalNamedColor, TerminalSession,
  TerminalSize, TerminalSnapshot,
};
use gpui::{
  App, AppContext, Application, AsyncApp, Bounds, Context, FocusHandle, Font, FontFallbacks, FontFeatures, FontWeight,
  Hsla, InteractiveElement, IntoElement, KeyDownEvent, MouseButton, ParentElement, Pixels, Render, SharedString,
  Styled, Task, TextRun, Timer, WeakEntity, Window, WindowBounds, WindowOptions, div, font, prelude::*, px, rgb, size,
};

const FONT_SIZE: f32 = 15.0;
const LINE_HEIGHT_MULTIPLIER: f32 = 1.4;
const VIEWPORT_PADDING: f32 = 24.0;
const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(16);

/// Font-derived geometry shared by layout, painting, and the native PTY.
#[derive(Clone, Copy)]
struct TerminalMetrics {
  cell_width: Pixels,
  cell_height: Pixels,
  font_size: Pixels,
}

/// Standalone GPUI host for a native terminal session.
struct TerminalDebugView {
  session: TerminalSession,
  focus: FocusHandle,
  size: TerminalSize,
  title: String,
  status: Option<String>,
  _event_task: Task<()>,
}

impl TerminalDebugView {
  /// Spawns a native shell and starts repainting when the VT grid changes.
  ///
  /// # Parameters
  ///
  /// * `working_directory` is the initial directory for the system shell.
  /// * `size` is the measured initial PTY grid, before the shell emits its prompt.
  /// * `cx` creates the focus handle and terminal-event polling task.
  ///
  /// # Returns
  ///
  /// A debug view backed by a real pseudo-terminal.
  fn new(working_directory: PathBuf, size: TerminalSize, cx: &mut Context<Self>) -> Result<Self, String> {
    let session = TerminalSession::spawn_default_shell(&working_directory, size).map_err(|error| error.to_string())?;
    let event_task = cx.spawn(|this: WeakEntity<Self>, async_cx: &mut AsyncApp| {
      let mut async_cx = async_cx.clone();
      async move {
        loop {
          Timer::after(EVENT_POLL_INTERVAL).await;
          let should_continue = this
            .update(&mut async_cx, |view, cx| {
              view.process_events(cx);
              true
            })
            .unwrap_or(false);
          if !should_continue {
            break;
          }
        }
      }
    });

    Ok(Self {
      session,
      focus: cx.focus_handle(),
      size,
      title: "Chitin native terminal".into(),
      status: None,
      _event_task: event_task,
    })
  }

  /// Applies pending session events and schedules a repaint when necessary.
  fn process_events(&mut self, cx: &mut Context<Self>) {
    let events = self.session.drain_events();
    if events.is_empty() {
      return;
    }

    for event in events {
      match event {
        TerminalEvent::Title(title) => self.title = title,
        TerminalEvent::Exited(code) => self.status = Some(format!("process exited with code {code}")),
        TerminalEvent::Error(error) => self.status = Some(error),
        TerminalEvent::Render | TerminalEvent::Bell => {}
      }
    }
    cx.notify();
  }

  /// Resizes the native PTY when the GPUI window's character capacity changes.
  fn resize_for_window(&mut self, window: &Window, metrics: TerminalMetrics) {
    let next = terminal_size_for_window(window, metrics);
    if next == self.size {
      return;
    }

    if let Err(error) = self.session.resize(next) {
      self.status = Some(error.to_string());
    } else {
      self.size = next;
    }
  }

  /// Encodes one GPUI key event and writes it directly to the PTY.
  fn handle_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
    let Some(bytes) = encode_key(event) else {
      return;
    };
    if let Err(error) = self.session.write(&bytes) {
      self.status = Some(error.to_string());
      cx.notify();
    }
    cx.stop_propagation();
  }
}

impl Render for TerminalDebugView {
  /// Renders the current VT character grid and a small diagnostic status line.
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let metrics = measure_terminal_metrics(window);
    self.resize_for_window(window, metrics);
    let snapshot = self.session.snapshot();
    let rows = (0..snapshot.rows).map(|row| {
      div().flex().h(metrics.cell_height).children(
        build_row_runs(&snapshot, row)
          .into_iter()
          .map(move |run| render_run(run, metrics)),
      )
    });
    let focus = self.focus.clone();

    div()
      .flex()
      .flex_col()
      .size_full()
      .overflow_hidden()
      .track_focus(&self.focus)
      .on_mouse_down(MouseButton::Left, move |_, window, cx| window.focus(&focus, cx))
      .on_key_down(cx.listener(|this, event, _, cx| this.handle_key(event, cx)))
      .bg(rgb(0x101216))
      .font(terminal_font())
      .text_size(metrics.font_size)
      .line_height(metrics.cell_height)
      .child(
        div()
          .flex()
          .items_center()
          .h(metrics.cell_height + px(6.0))
          .px_3()
          .text_color(rgb(0x8b949e))
          .child(self.status.clone().unwrap_or_else(|| self.title.clone())),
      )
      .child(div().flex().flex_col().px_3().children(rows))
  }
}

/// Measures the bundled terminal font and derives one fixed character grid.
fn measure_terminal_metrics(window: &mut Window) -> TerminalMetrics {
  let font_size = px(FONT_SIZE);
  let regular_width = measure_terminal_cell_width(window, font_size, FontWeight::NORMAL);
  let bold_width = measure_terminal_cell_width(window, font_size, FontWeight::BOLD);

  TerminalMetrics {
    cell_width: regular_width.max(bold_width).max(px(1.0)),
    cell_height: px((FONT_SIZE * LINE_HEIGHT_MULTIPLIER).round()).max(px(1.0)),
    font_size,
  }
}

/// Measures one upright terminal cell at the requested font weight.
fn measure_terminal_cell_width(window: &mut Window, font_size: Pixels, weight: FontWeight) -> Pixels {
  let mut terminal_font = terminal_font();
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

/// Derives the PTY grid from the current drawable window and font metrics.
fn terminal_size_for_window(window: &Window, metrics: TerminalMetrics) -> TerminalSize {
  let bounds = window.bounds();
  let cell_width = f32::from(metrics.cell_width);
  let cell_height = f32::from(metrics.cell_height);
  let width = (f32::from(bounds.size.width) - VIEWPORT_PADDING).max(cell_width);
  let height = (f32::from(bounds.size.height) - VIEWPORT_PADDING - cell_height).max(cell_height);
  let columns = (width / cell_width).floor().max(1.0) as usize;
  let rows = (height / cell_height).floor().max(1.0) as usize;

  TerminalSize::new(columns, rows, cell_width.ceil() as u16, cell_height.ceil() as u16)
}

/// Disables contextual and standard ligatures so one glyph cannot span several cells.
fn terminal_font_features() -> FontFeatures {
  FontFeatures(Arc::new(vec![
    ("calt".into(), 0),
    ("liga".into(), 0),
    ("clig".into(), 0),
  ]))
}

/// Builds the upright terminal font stack, including symbol and emoji fallbacks.
fn terminal_font() -> Font {
  let mut terminal_font = font(TERMINAL_FONT_FAMILY);
  terminal_font.features = terminal_font_features();
  terminal_font.fallbacks = Some(FontFallbacks::from_fonts(vec![
    "Symbols Nerd Font".into(),
    "Noto Color Emoji".into(),
    "Segoe UI Emoji".into(),
    "Apple Color Emoji".into(),
    "Noto Sans Symbols 2".into(),
  ]));
  terminal_font
}

/// A consecutive sequence of cells that share one paint style.
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
  /// Starts a run with the visual state of one terminal cell.
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

  /// Returns whether another cell can share this run's shaping and paint operation.
  fn matches(&self, cell: &TerminalCell, cursor: bool) -> bool {
    self.foreground == cell.foreground
      && self.background == cell.background
      && self.attributes == run_attributes(cell.attributes)
      && self.wide == cell.attributes.wide
      && self.cursor == cursor
  }

  /// Appends one cell while retaining its fixed grid-column budget.
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

/// Groups one row into the smallest set of consecutive equal-style text runs.
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

/// Returns whether a cell is already represented by the viewport background.
fn is_default_blank(cell: &TerminalCell) -> bool {
  cell.character == ' '
    && cell.combining_characters.is_empty()
    && cell.foreground == TerminalColor::Named(TerminalNamedColor::Foreground)
    && cell.background == TerminalColor::Named(TerminalNamedColor::Background)
    && upright_attributes(cell.attributes) == TerminalCellAttributes::default()
}

/// Removes font slant while preserving the remaining terminal text attributes.
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

/// Renders one equal-style text run using a fixed terminal-column width.
fn render_run(run: TerminalRun, metrics: TerminalMetrics) -> impl IntoElement {
  let (mut foreground, mut background) = (resolve_color(run.foreground), resolve_color(run.background));
  if run.attributes.inverse {
    std::mem::swap(&mut foreground, &mut background);
  }
  if run.attributes.dim {
    foreground = foreground.opacity(0.62);
  }
  if run.cursor {
    std::mem::swap(&mut foreground, &mut background);
    if background == resolve_color(TerminalColor::Named(TerminalNamedColor::Background)) {
      background = rgb(0xd0d7de).into();
    }
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

/// Resolves a VT color against the debug terminal's xterm-compatible palette.
fn resolve_color(color: TerminalColor) -> Hsla {
  match color {
    TerminalColor::Rgb { red, green, blue } => {
      rgb((u32::from(red) << 16) | (u32::from(green) << 8) | u32::from(blue)).into()
    }
    TerminalColor::Indexed(index) => indexed_color(index),
    TerminalColor::Named(named) => named_color(named),
  }
}

/// Resolves semantic and ANSI color names used by the VT state machine.
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
    TerminalNamedColor::Foreground => 0xd0d7de,
    TerminalNamedColor::Background => 0x101216,
    TerminalNamedColor::Cursor => 0xd0d7de,
    TerminalNamedColor::DimForeground => 0x8b949e,
  };
  rgb(value).into()
}

/// Converts one xterm-256 palette index into an RGB color.
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

/// Converts a GPUI keystroke into the byte sequence expected by a terminal application.
fn encode_key(event: &KeyDownEvent) -> Option<Vec<u8>> {
  let key = event.keystroke.key.as_str();
  let modifiers = event.keystroke.modifiers;
  let special = match key {
    "enter" => Some(b"\r".as_slice()),
    "backspace" => Some(b"\x7f".as_slice()),
    "tab" if modifiers.shift => Some(b"\x1b[Z".as_slice()),
    "tab" => Some(b"\t".as_slice()),
    "escape" => Some(b"\x1b".as_slice()),
    "up" => Some(b"\x1b[A".as_slice()),
    "down" => Some(b"\x1b[B".as_slice()),
    "right" => Some(b"\x1b[C".as_slice()),
    "left" => Some(b"\x1b[D".as_slice()),
    "home" => Some(b"\x1b[H".as_slice()),
    "end" => Some(b"\x1b[F".as_slice()),
    "pageup" => Some(b"\x1b[5~".as_slice()),
    "pagedown" => Some(b"\x1b[6~".as_slice()),
    "delete" => Some(b"\x1b[3~".as_slice()),
    "insert" => Some(b"\x1b[2~".as_slice()),
    _ => None,
  };
  if let Some(special) = special {
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

/// Opens the native terminal integration example.
fn main() {
  let working_directory = std::env::args_os()
    .nth(1)
    .map(PathBuf::from)
    .or_else(|| std::env::current_dir().ok())
    .unwrap_or_else(|| PathBuf::from("."));

  Application::new()
    .with_assets(chitin_ui::assets::ChitinAssets)
    .run(move |cx: &mut App| {
      chitin_ui::init(cx);
      if let Err(error) = register_terminal_fonts(cx) {
        eprintln!("failed to register bundled terminal fonts: {error}");
        cx.quit();
        return;
      }
      let result = gpui_kit::open_window(
        WindowOptions {
          window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(960.0), px(640.0)),
            cx,
          ))),
          app_id: Some("dev.chitin.TerminalDebug".into()),
          ..Default::default()
        },
        cx,
        move |window, cx| {
          window.activate_window();
          let metrics = measure_terminal_metrics(window);
          let terminal_size = terminal_size_for_window(window, metrics);
          cx.new(
            |cx| match TerminalDebugView::new(working_directory, terminal_size, cx) {
              Ok(view) => view,
              Err(error) => panic!("failed to start native terminal example: {error}"),
            },
          )
        },
      );

      if let Err(error) = result {
        eprintln!("failed to open native terminal example: {error}");
        cx.quit();
        return;
      }
      cx.activate(true);
    });
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn row_builder_should_merge_equal_style_cells() {
    let cell = TerminalCell {
      character: 'x',
      ..TerminalCell::default()
    };
    let snapshot = TerminalSnapshot {
      columns: 120,
      rows: 1,
      cells: vec![cell; 120],
      cursor: None,
      display_offset: 0,
      history_size: 0,
    };

    let runs = build_row_runs(&snapshot, 0);

    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].columns, 120);
  }

  #[test]
  fn row_builder_should_skip_trailing_default_background_cells() {
    let mut cells = vec![TerminalCell::default(); 120];
    cells[0].character = 'x';
    let snapshot = TerminalSnapshot {
      columns: 120,
      rows: 1,
      cells,
      cursor: None,
      display_offset: 0,
      history_size: 0,
    };

    let runs = build_row_runs(&snapshot, 0);

    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].columns, 1);
  }

  #[test]
  fn cursor_should_split_only_its_own_cell_from_a_run() {
    let cell = TerminalCell {
      character: 'x',
      ..TerminalCell::default()
    };
    let snapshot = TerminalSnapshot {
      columns: 120,
      rows: 1,
      cells: vec![cell; 120],
      cursor: Some((0, 60)),
      display_offset: 0,
      history_size: 0,
    };

    let runs = build_row_runs(&snapshot, 0);

    assert_eq!(runs.len(), 3);
    assert_eq!(runs.iter().map(|run| run.columns).collect::<Vec<_>>(), [60, 1, 59]);
  }

  #[test]
  fn row_builder_should_render_italic_cells_upright() {
    let cell = TerminalCell {
      character: 'x',
      attributes: TerminalCellAttributes {
        italic: true,
        ..TerminalCellAttributes::default()
      },
      ..TerminalCell::default()
    };
    let snapshot = TerminalSnapshot {
      columns: 1,
      rows: 1,
      cells: vec![cell],
      cursor: None,
      display_offset: 0,
      history_size: 0,
    };

    let runs = build_row_runs(&snapshot, 0);

    assert_eq!(runs.len(), 1);
    assert!(!runs[0].attributes.italic);
  }

  #[test]
  fn row_builder_should_merge_upright_and_italic_cells() {
    let upright = TerminalCell {
      character: 'a',
      ..TerminalCell::default()
    };
    let italic = TerminalCell {
      character: 'b',
      attributes: TerminalCellAttributes {
        italic: true,
        ..TerminalCellAttributes::default()
      },
      ..TerminalCell::default()
    };
    let snapshot = TerminalSnapshot {
      columns: 2,
      rows: 1,
      cells: vec![upright, italic],
      cursor: None,
      display_offset: 0,
      history_size: 0,
    };

    let runs = build_row_runs(&snapshot, 0);

    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].text, "ab");
  }
}
