//! Interactive gallery for Chitin composite components.
//!
//! Run with `cargo run -p chitin-ui --example composite-showcase`.

use std::{borrow::Cow, fs, io, path::PathBuf};

use chitin_ui::{
  composite::toast::{Toast, ToastHost, ToastVariant, ToastViewport},
  primitive::button::{Button, ButtonEvent, ButtonState, ButtonVariant},
  themes::builtins,
};
use gpui::{
  App, AppContext, Application, AssetSource, Bounds, Context, Entity, ParentElement, Render, Result, SharedString,
  Subscription, Window, WindowBounds, WindowOptions, div, prelude::*, px, size,
};

/// Filesystem-backed assets used by standalone composite examples.
struct CompositeShowcaseAssets {
  base: PathBuf,
}

impl AssetSource for CompositeShowcaseAssets {
  /// Loads an asset from the workspace asset directory.
  ///
  /// # Parameters
  ///
  /// * `path` is the asset-relative path requested by a component.
  ///
  /// # Returns
  ///
  /// The asset bytes, `None` for a missing asset, or the original I/O error.
  fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
    match fs::read(self.base.join(path)) {
      Ok(data) => Ok(Some(Cow::Owned(data))),
      Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
      Err(error) => Err(error.into()),
    }
  }

  /// Lists direct child asset names under an asset-relative directory.
  ///
  /// # Parameters
  ///
  /// * `path` is the asset-relative directory requested by GPUI.
  ///
  /// # Returns
  ///
  /// UTF-8 child names, or the directory enumeration error.
  fn list(&self, path: &str) -> Result<Vec<SharedString>> {
    fs::read_dir(self.base.join(path))
      .map(|entries| {
        entries
          .filter_map(|entry| {
            entry
              .ok()
              .and_then(|entry| entry.file_name().into_string().ok())
              .map(SharedString::from)
          })
          .collect()
      })
      .map_err(Into::into)
  }
}

/// One persistent button that creates a specific semantic Toast variant.
struct ToastVariantControl {
  label: &'static str,
  variant: ToastVariant,
  button: Entity<ButtonState>,
}

/// Showcase state connecting variant controls to the Toast composite.
struct CompositeShowcase {
  toast_controls: Vec<ToastVariantControl>,
  toast_viewport: Entity<ToastViewport>,
  created_count: usize,
  _subscriptions: Vec<Subscription>,
}

impl CompositeShowcase {
  /// Creates the showcase controls and routes button activation into the queue.
  ///
  /// # Parameters
  ///
  /// * `cx` creates the persistent button and Toast viewport entities.
  ///
  /// # Returns
  ///
  /// A showcase whose generated Toasts remain visible for stack inspection.
  fn new(cx: &mut Context<Self>) -> Self {
    let toast_viewport = cx.new(|_| ToastViewport::new());
    let variants = [
      ("Default", ToastVariant::Default),
      ("Success", ToastVariant::Success),
      ("Info", ToastVariant::Info),
      ("Warning", ToastVariant::Warning),
      ("Error", ToastVariant::Error),
    ];
    let mut toast_controls = Vec::with_capacity(variants.len());
    let mut subscriptions = Vec::with_capacity(variants.len());

    for (label, variant) in variants {
      let button = cx.new(ButtonState::new);
      subscriptions.push(cx.subscribe(&button, move |this, _, event, cx| {
        if matches!(event, ButtonEvent::Click) {
          this.push_toast(label, variant, cx);
        }
      }));
      toast_controls.push(ToastVariantControl { label, variant, button });
    }

    Self {
      toast_controls,
      toast_viewport,
      created_count: 0,
      _subscriptions: subscriptions,
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
  fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl gpui::IntoElement {
    let theme = builtins::dark();
    div()
      .relative()
      .flex()
      .size_full()
      .items_center()
      .justify_center()
      .bg(theme.background.primary)
      .text_color(theme.text.primary)
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
              .text_color(theme.text.secondary)
              .child("Create each semantic variant, then hover the stack to inspect it."),
          )
          .child(
            div()
              .flex()
              .items_center()
              .gap_2()
              .children(self.toast_controls.iter().map(|control| {
                Button::new(control.button.clone())
                  .theme(theme)
                  .variant(if control.variant == ToastVariant::Default {
                    ButtonVariant::Primary
                  } else {
                    ButtonVariant::Secondary
                  })
                  .child(control.label)
              })),
          ),
      )
      .child(ToastHost::new(self.toast_viewport.clone()))
  }
}

/// Opens the GPUI composite component gallery.
fn main() {
  env_logger::init();
  Application::new()
    .with_assets(CompositeShowcaseAssets {
      base: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
    })
    .run(|cx: &mut App| {
      let result = cx.open_window(
        WindowOptions {
          window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(900.0), px(600.0)),
            cx,
          ))),
          app_id: Some("dev.chitin.CompositeShowcase".to_string()),
          ..Default::default()
        },
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
