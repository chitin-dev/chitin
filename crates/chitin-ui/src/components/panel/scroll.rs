//! Scroll state for panel tab bars.

use std::{
  cell::RefCell,
  fmt,
  rc::Rc,
  time::{Duration, Instant},
};

use gpui::{Pixels, Point, ScrollHandle};

use super::model::PanelId;

/// Duration that a tab-scroll indicator remains visible after scrolling.
pub const PANEL_TAB_SCROLL_INDICATOR_VISIBLE_DURATION: Duration = Duration::from_secs(2);
/// Duration used to fade out the tab-scroll indicator before it disappears.
pub const PANEL_TAB_SCROLL_INDICATOR_FADE_DURATION: Duration = Duration::from_secs(1);
/// Redraw cadence used while the tab-scroll indicator is fading.
pub const PANEL_TAB_SCROLL_INDICATOR_FADE_FRAME_DURATION: Duration = Duration::from_millis(16);

/// Per-panel presentation state for one tab bar scroll lane.
struct PanelTabScrollEntry {
  panel_id: PanelId,
  handle: ScrollHandle,
  last_offset: Option<Point<Pixels>>,
  last_revealed_tab_index: Option<usize>,
  indicator_visible_until: Option<Instant>,
  scheduled_redraw_at: Option<Instant>,
}

impl PanelTabScrollEntry {
  /// Creates scroll state for one panel tab bar.
  ///
  /// # Parameters
  ///
  /// `panel_id` identifies the panel whose tab bar owns the scroll handle.
  ///
  /// # Returns
  ///
  /// A [`PanelTabScrollEntry`] with no recorded scroll activity.
  fn new(panel_id: PanelId) -> Self {
    Self {
      panel_id,
      handle: ScrollHandle::new(),
      last_offset: None,
      last_revealed_tab_index: None,
      indicator_visible_until: None,
      scheduled_redraw_at: None,
    }
  }

  /// Marks the scroll indicator visible from a scroll activity timestamp.
  ///
  /// # Parameters
  ///
  /// `now` is the current UI clock timestamp.
  ///
  /// # Returns
  ///
  /// The timestamp when the indicator should be hidden.
  fn mark_scroll_activity(&mut self, now: Instant) -> Instant {
    let visible_until = now + PANEL_TAB_SCROLL_INDICATOR_VISIBLE_DURATION;
    self.indicator_visible_until = Some(visible_until);
    self.scheduled_redraw_at = None;
    visible_until
  }
}

/// Persistent scroll handles for panel tab bars.
///
/// Panel tab scrolling is presentation state: it should survive GPUI render
/// passes, but it should not be stored in the logical [`super::PanelTree`].
#[derive(Clone, Default)]
pub struct PanelTabScrollState {
  entries: Rc<RefCell<Vec<PanelTabScrollEntry>>>,
}

impl PanelTabScrollState {
  /// Creates empty tab scroll state.
  ///
  /// # Parameters
  ///
  /// This function takes no parameters.
  ///
  /// # Returns
  ///
  /// A [`PanelTabScrollState`] with no tracked panel handles.
  pub fn new() -> Self {
    Self::default()
  }

  /// Returns the scroll handle associated with one panel tab bar.
  ///
  /// # Parameters
  ///
  /// `panel_id` identifies the panel whose tab bar should be scrolled.
  ///
  /// # Returns
  ///
  /// A persistent [`ScrollHandle`] reused across render passes for `panel_id`.
  pub fn scroll_handle(&self, panel_id: PanelId) -> ScrollHandle {
    if let Some(entry) = self
      .entries
      .borrow()
      .iter()
      .find(|entry| entry.panel_id == panel_id)
    {
      return entry.handle.clone();
    }

    let entry = PanelTabScrollEntry::new(panel_id);
    let handle = entry.handle.clone();
    self.entries.borrow_mut().push(entry);
    handle
  }

  /// Queues a scroll request that reveals one tab in its panel tab bar.
  ///
  /// # Parameters
  ///
  /// `panel_id` identifies the panel whose tab bar owns the tab.
  ///
  /// `tab_index` is the tab's zero-based position in that panel's tab list.
  ///
  /// `now` is the current UI clock timestamp.
  ///
  /// # Returns
  ///
  /// The indicator expiry timestamp when this reveal changed the active tab
  /// target after initialization.
  pub fn reveal_tab(&self, panel_id: PanelId, tab_index: usize, now: Instant) -> Option<Instant> {
    let mut entries = self.entries.borrow_mut();
    let entry_index = entries
      .iter()
      .position(|entry| entry.panel_id == panel_id)
      .unwrap_or_else(|| {
        entries.push(PanelTabScrollEntry::new(panel_id));
        entries.len() - 1
      });
    let entry = &mut entries[entry_index];
    entry.handle.scroll_to_item(tab_index);

    if entry.last_revealed_tab_index.is_none() {
      entry.last_revealed_tab_index = Some(tab_index);
      return None;
    }

    if entry.last_revealed_tab_index == Some(tab_index) {
      return None;
    }

    entry.last_revealed_tab_index = Some(tab_index);
    Some(entry.mark_scroll_activity(now))
  }

  /// Records scroll offset changes and marks the indicator visible when needed.
  ///
  /// # Parameters
  ///
  /// `panel_id` identifies the panel whose tab bar owns the scroll handle.
  ///
  /// `scroll_handle` provides the current GPUI scroll offset.
  ///
  /// `now` is the current UI clock timestamp.
  ///
  /// # Returns
  ///
  /// The indicator expiry timestamp when the offset changed after
  /// initialization.
  pub fn observe_scroll_position(
    &self,
    panel_id: PanelId,
    scroll_handle: &ScrollHandle,
    now: Instant,
  ) -> Option<Instant> {
    if scroll_handle.max_offset().width <= Pixels::ZERO {
      return None;
    }

    let current_offset = scroll_handle.offset();
    let mut entries = self.entries.borrow_mut();
    let entry_index = entries
      .iter()
      .position(|entry| entry.panel_id == panel_id)
      .unwrap_or_else(|| {
        entries.push(PanelTabScrollEntry::new(panel_id));
        entries.len() - 1
      });
    let entry = &mut entries[entry_index];

    if entry.last_offset.is_none() {
      entry.last_offset = Some(current_offset);
      return None;
    }

    if entry.last_offset == Some(current_offset) {
      return None;
    }

    entry.last_offset = Some(current_offset);
    Some(entry.mark_scroll_activity(now))
  }

  /// Returns whether the tab-scroll indicator should currently be visible.
  ///
  /// # Parameters
  ///
  /// `panel_id` identifies the panel whose tab bar owns the scroll handle.
  ///
  /// `now` is the current UI clock timestamp.
  ///
  /// # Returns
  ///
  /// `true` when the indicator is still inside its post-scroll visibility
  /// window.
  pub fn indicator_visible(&self, panel_id: PanelId, now: Instant) -> bool {
    self
      .entries
      .borrow()
      .iter()
      .find(|entry| entry.panel_id == panel_id)
      .and_then(|entry| entry.indicator_visible_until)
      .is_some_and(|visible_until| visible_until > now)
  }

  /// Returns current opacity for the tab-scroll indicator.
  ///
  /// # Parameters
  ///
  /// `panel_id` identifies the panel whose tab bar owns the scroll handle.
  ///
  /// `now` is the current UI clock timestamp.
  ///
  /// `base_opacity` is the fully visible opacity before fading starts.
  ///
  /// # Returns
  ///
  /// The current opacity when the indicator is inside its visibility window.
  pub fn indicator_opacity(
    &self,
    panel_id: PanelId,
    now: Instant,
    base_opacity: f32,
  ) -> Option<f32> {
    let visible_until = self
      .entries
      .borrow()
      .iter()
      .find(|entry| entry.panel_id == panel_id)
      .and_then(|entry| entry.indicator_visible_until)?;

    if visible_until <= now {
      return None;
    }

    let remaining = visible_until.saturating_duration_since(now);
    if remaining >= PANEL_TAB_SCROLL_INDICATOR_FADE_DURATION {
      return Some(base_opacity);
    }

    let fade_progress =
      remaining.as_secs_f32() / PANEL_TAB_SCROLL_INDICATOR_FADE_DURATION.as_secs_f32();
    Some(base_opacity * fade_progress.clamp(0.0, 1.0))
  }

  /// Returns unscheduled indicator redraw timestamps and marks them scheduled.
  ///
  /// # Parameters
  ///
  /// `now` is the current UI clock timestamp.
  ///
  /// # Returns
  ///
  /// Redraw timestamps needed to start, animate, and finish indicator fade-out.
  pub fn indicator_redraws_to_schedule(&self, now: Instant) -> Vec<Instant> {
    self
      .entries
      .borrow_mut()
      .iter_mut()
      .filter_map(|entry| {
        let visible_until = entry.indicator_visible_until?;
        if visible_until <= now {
          return None;
        }

        let fade_starts_at = visible_until
          .checked_sub(PANEL_TAB_SCROLL_INDICATOR_FADE_DURATION)
          .unwrap_or(visible_until);
        let redraw_at = if now < fade_starts_at {
          fade_starts_at
        } else {
          (now + PANEL_TAB_SCROLL_INDICATOR_FADE_FRAME_DURATION).min(visible_until)
        };

        if entry.scheduled_redraw_at == Some(redraw_at) {
          return None;
        }

        entry.scheduled_redraw_at = Some(redraw_at);
        Some(redraw_at)
      })
      .collect()
  }

  #[cfg(test)]
  pub(crate) fn tracked_panel_count(&self) -> usize {
    self.entries.borrow().len()
  }
}

impl fmt::Debug for PanelTabScrollState {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("PanelTabScrollState")
      .field(
        "panel_ids",
        &self
          .entries
          .borrow()
          .iter()
          .map(|entry| entry.panel_id)
          .collect::<Vec<_>>(),
      )
      .finish()
  }
}

impl PartialEq for PanelTabScrollState {
  fn eq(&self, other: &Self) -> bool {
    self
      .entries
      .borrow()
      .iter()
      .map(|entry| entry.panel_id)
      .eq(other.entries.borrow().iter().map(|entry| entry.panel_id))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn scroll_handle_should_reuse_existing_panel_handle() {
    let scroll_state = PanelTabScrollState::new();

    scroll_state.scroll_handle(PanelId::new(1));
    scroll_state.scroll_handle(PanelId::new(1));

    assert_eq!(scroll_state.tracked_panel_count(), 1);
  }

  #[test]
  fn scroll_handle_should_track_distinct_panel_handles() {
    let scroll_state = PanelTabScrollState::new();

    scroll_state.scroll_handle(PanelId::new(1));
    scroll_state.scroll_handle(PanelId::new(2));

    assert_eq!(scroll_state.tracked_panel_count(), 2);
  }

  #[test]
  fn reveal_tab_should_show_indicator_after_active_tab_changes() {
    let scroll_state = PanelTabScrollState::new();
    let now = Instant::now();

    scroll_state.reveal_tab(PanelId::new(1), 0, now);
    scroll_state.reveal_tab(PanelId::new(1), 1, now);

    assert!(scroll_state.indicator_visible(PanelId::new(1), now));
  }

  #[test]
  fn indicator_visible_should_expire_after_timeout() {
    let scroll_state = PanelTabScrollState::new();
    let now = Instant::now();

    scroll_state.reveal_tab(PanelId::new(1), 0, now);
    scroll_state.reveal_tab(PanelId::new(1), 1, now);

    assert!(!scroll_state.indicator_visible(
      PanelId::new(1),
      now + PANEL_TAB_SCROLL_INDICATOR_VISIBLE_DURATION
    ));
  }

  #[test]
  fn indicator_opacity_should_remain_full_before_fade_window() {
    let scroll_state = PanelTabScrollState::new();
    let now = Instant::now();

    scroll_state.reveal_tab(PanelId::new(1), 0, now);
    scroll_state.reveal_tab(PanelId::new(1), 1, now);

    assert_eq!(
      scroll_state.indicator_opacity(PanelId::new(1), now, 0.6),
      Some(0.6)
    );
  }

  #[test]
  fn indicator_opacity_should_fade_linearly_in_last_second() {
    let scroll_state = PanelTabScrollState::new();
    let now = Instant::now();

    scroll_state.reveal_tab(PanelId::new(1), 0, now);
    scroll_state.reveal_tab(PanelId::new(1), 1, now);

    assert_eq!(
      scroll_state.indicator_opacity(PanelId::new(1), now + Duration::from_millis(1500), 0.6),
      Some(0.3)
    );
  }
}
