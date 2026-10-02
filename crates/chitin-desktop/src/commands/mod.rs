//! Desktop execution adapters for typed commands.

pub mod portable;
pub mod render;
pub mod shell_host;
mod shell_panels;
pub(crate) use shell_panels::terminal_label;
