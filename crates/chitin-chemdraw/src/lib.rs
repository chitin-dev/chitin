//! Frontend-independent chemical depiction, not an editor or UI toolkit.
//!
//! [`layout`] creates normalized molecular coordinates. [`depict`] converts
//! supplied chemical facts and coordinates into a [`Drawing`] of generic
//! primitives. [`svg`] exports that scene without reinterpreting chemistry.
//! Existing coordinates may replace automatic layout. No stage owns selection,
//! windows, command history, or mutable chemical property caches.
//!
//! # Examples
//! ```
//! use chitin_chem::io::smiles::read_smiles;
//! use chitin_chemdraw::{layout, depict, svg};
//! let graph = read_smiles("CC(O)C")?.into_graph();
//! let coordinates = layout::generate(&graph, &layout::LayoutOptions::default())?;
//! let drawing = depict::depict(&graph, &coordinates, &depict::DepictOptions::default())?;
//! let text = svg::export(&drawing, &svg::SvgOptions::default())?;
//! assert!(text.contains("<svg"));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

#![deny(missing_docs)]

mod chemical;
mod stereo_geometry;

pub mod depict;
pub mod font;
pub mod geometry;
pub mod layout;
pub mod scene;
pub mod svg;

pub use scene::{Circle, Color, Drawing, Line, Primitive, Source, Text};
