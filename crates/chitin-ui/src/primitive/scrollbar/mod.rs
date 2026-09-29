//! A draggable scrollbar reporting where a viewport should sit.
//!
//! [`ScrollbarState`] owns the drag. Consumers subscribe to [`ScrollbarEvent`]
//! values rather than attaching component-specific pointer handlers.

mod event;
mod metrics;
mod render;
mod state;

pub use event::ScrollbarEvent;
pub use metrics::ScrollbarMetrics;
pub use render::{Scrollbar, ScrollbarSize};
pub use state::ScrollbarState;
