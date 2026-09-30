//! Interactive gallery for Chitin composite components.
//!
//! Run with `cargo run -p chitin-ui --example composite-showcase`.

use chitin_ui::{
  assets::ChitinAssets,
  composite::toast::{Toast, ToastHost, ToastVariant, ToastViewport},
};
use gpui::{
  App, AppContext, Application, Bounds, Context, Entity, ParentElement, Render, WeakEntity, Window, WindowBounds,
  WindowOptions, div, prelude::*, px, size,
};
use gpui_kit::component::button::{Button, ButtonVariant, ButtonVariants as _};

/// One control that creates a specific semantic Toast variant.
struct ToastVariantControl {
  label: &'static str,
  variant: ToastVariant,
}

/// Showcase state connecting variant controls to the Toast composite.
struct CompositeShowcase {
  toast_controls: Vec<ToastVariantControl>,
  toast_viewport: Entity<ToastViewport>,
  created_count: usize,
}

impl CompositeShowcase {
  /// Creates the showcase controls.
  ///
  /// Each control carries its own variant, so nothing has to be subscribed or
  /// retained between frames.
  ///
  /// # Parameters
  ///
  /// * `cx` creates the Toast viewport entity.
  ///
  /// # Returns
  ///
  /// A showcase whose generated Toasts remain visible for stack inspection.
  fn new(cx: &mut Context<Self>) -> Self {
    let toast_viewport = cx.new(|_| ToastViewport::new());
    let toast_controls = [
      ("Default", ToastVariant::Default),
      ("Success", ToastVariant::Success),
      ("Info", ToastVariant::Info),
      ("Warning", ToastVariant::Warning),
      ("Error", ToastVariant::Error),
    ]
    .into_iter()
    .map(|(label, variant)| ToastVariantControl { label, variant })
    .collect();

    Self {
      toast_controls,
      toast_viewport,
      created_count: 0,
    }
  }

  /// Adds a persistent example notification for one semantic variant.
  ///
  /// # Parameters
  ///
  /// * `label` names the variant in the generated notification text.
  /// * `variant` selects the Toast's semantic accent treatment.
  /// * `cx` updates the viewport entity and schedules the showcase repaint.
  fn push_toast(&mut self, label: &'static str, variant: ToastVariant, cx: &mut Context<Self>) {
    self.created_count += 1;
    let sequence = self.created_count;
    self.toast_viewport.update(cx, |viewport, cx| {
      viewport.push(
        Toast::new(format!("{label} toast {sequence}"))
          .description(format!("This notification uses the {label} variant."))
          .action("Dismiss")
          .variant(variant)
          .duration(None),
        cx,
      );
    });
    cx.notify();
  }
}

impl Render for CompositeShowcase {
  /// Renders the Toast trigger and window-root notification host.
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
    let theme = gpui_kit::component::Theme::global(cx).colors;
    let showcase: WeakEntity<Self> = cx.weak_entity();
    div()
      .relative()
      .flex()
      .size_full()
      .items_center()
      .justify_center()
      .bg(theme.background)
      .text_color(theme.foreground)
      .child(
        div()
          .flex()
          .flex_col()
          .items_center()
          .gap_3()
          .child(div().text_lg().child("Toast stack"))
          .child(
            div()
              .text_sm()
              .text_color(theme.muted_foreground)
              .child("Create each semantic variant, then hover the stack to inspect it."),
          )
          .child(
            div()
              .flex()
              .items_center()
              .gap_2()
              .children(self.toast_controls.iter().map(|control| {
                let showcase = showcase.clone();
                let label = control.label;
                let variant = control.variant;
                Button::new(format!("showcase-toast-{label}"))
                  .with_variant(if variant == ToastVariant::Default {
                    ButtonVariant::Primary
                  } else {
                    ButtonVariant::Secondary
                  })
                  .on_click(move |_, _, cx| {
                    let _ = showcase.update(cx, |this, cx| this.push_toast(label, variant, cx));
                  })
                  .child(label)
              })),
          ),
      )
      .child(ToastHost::new(self.toast_viewport.clone()))
  }
}

/// Opens the GPUI composite component gallery.
fn main() {
  env_logger::init();
  Application::new().with_assets(ChitinAssets).run(|cx: &mut App| {
    // Every layer this gallery draws from, and the theme global each of them
    // reads. The controls are GPUI Kit buttons, so the global has to exist
    // before the first one renders.
    chitin_ui::init(cx);

    let result = gpui_kit::open_window(
      WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
          None,
          size(px(900.0), px(600.0)),
          cx,
        ))),
        app_id: Some("dev.chitin.CompositeShowcase".to_string()),
        ..Default::default()
      },
      cx,
      |window, cx| {
        window.activate_window();
        cx.new(CompositeShowcase::new)
      },
    );

    if let Err(error) = result {
      eprintln!("failed to open composite showcase: {error}");
      cx.quit();
      return;
    }
    cx.activate(true);
  });
}
