#![forbid(unsafe_code)]
//! Renderer-independent biological structure models and format readers.
//!
//! The first supported format is the fixed-column PDB format. Parsing produces
//! an indexed [`structure::Structure`] snapshot that can be shared by the
//! command-line, desktop, and rendering crates without a GPUI dependency.

pub mod chemistry;
pub mod structure;
