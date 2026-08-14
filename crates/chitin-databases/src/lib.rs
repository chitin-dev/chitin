#![forbid(unsafe_code)]
//! Typed access to external biological and chemical databases.
//!
//! This crate owns provider clients, transport policy, provider-specific
//! response DTOs, downloaded artifact metadata, and provenance information. It
//! does not own canonical molecular models, file-format parsers, rendering
//! resources, or application UI state.

mod client;
mod config;
mod error;
mod provenance;
mod request;
mod response;
mod retry;
mod transport;

/// External database provider clients.
pub mod providers;
/// Test helpers for deterministic provider tests.
#[cfg(any(test, doctest, feature = "test-support"))]
pub mod test_support;

pub use client::Client;
pub use config::ClientConfig;
pub use error::{DataError, DecodeError, RemoteError, TransportError};
pub use provenance::{Provenance, ProviderId};
pub use request::{HttpMethod, HttpRequest};
pub use response::{ArtifactFormat, DownloadedArtifact, HttpResponse};
pub use retry::RetryPolicy;
pub use transport::{DownloadProgressCallback, HttpTransport};
