//! Desktop execution adapters for typed commands.

pub(crate) mod history;
pub mod portable;
pub mod render;
pub(crate) mod selection;
pub mod shell_host;
mod shell_panels;
pub(crate) use shell_panels::terminal_label;
