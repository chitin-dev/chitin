#![forbid(unsafe_code)]
//! Stand-alone SES pipeline visualizer.
//!
//! All diagnostic computation and rendering remains under `examples/`; the
//! production bio and desktop modules expose only reusable scientific and UI
//! APIs.

#[path = "ses-debug/mod.rs"]
mod ses_debug;

fn main() {
  ses_debug::run();
}
