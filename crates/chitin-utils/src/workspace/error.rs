//! Error types returned by workspace discovery and loading operations.

use std::{
  io,
  path::{Path, PathBuf},
};

/// Errors that can occur when interacting with a project workspace.
///
/// A workspace is a directory that contains a Chitin project configuration file
/// (`chitin.yaml` or `.chitin/config.yaml`) and associated subdirectories
/// for structures, docking results, trajectories, etc.
///
/// This enum captures filesystem-level errors that happen during workspace
/// discovery, validation, and directory traversal.
#[derive(Debug, thiserror::Error)]
pub enum ProjectWorkspaceError {
  /// The requested workspace path does not exist on disk.
  #[error("project path doesn't exist: {0}")]
  NotFound(PathBuf),

  /// The requested path exists but is a file, not a directory. It will be treated
  /// using different logic.
  #[error("project path is not a directory: {0}")]
  NotDirectory(PathBuf),

  /// Failed to resolve the path to an absolute (canonical) form.
  /// This typically occurs due to permissions issues or broken symlinks.
  #[error("failed to canonicalize project path '{path}': {source}")]
  Canonicalize {
    /// The path that could not be canonicalized.
    path: PathBuf,
    /// The underlying I/O error from `std::fs::canonicalize`.
    #[source]
    source: io::Error,
  },

  /// Failed to read the directory contents during workspace scanning.
  #[error("failed to read directory '{path}': {source}")]
  ReadDir {
    /// The directory path that could not be read.
    path: PathBuf,
    /// The underlying I/O error from `std::fs::read_dir`.
    #[source]
    source: io::Error,
  },

  /// Failed to read a specific directory entry (file or subdirectory).
  ///
  /// This is different from `ReadDir` in that it occurs while iterating
  /// entries, rather than when opening the directory itself.
  #[error("failed to read directory entry under '{path}': {source}")]
  ReadEntry {
    /// The path of the specific entry that caused the error.
    path: PathBuf,
    /// The underlying I/O error from the entry's metadata read.
    #[source]
    source: io::Error,
  },

  /// Failed to determine the file type of a directory entry.
  ///
  /// This can happen when the filesystem does not support file type
  /// metadata or when the entry is a broken symlink.
  #[error("failed to read file type for '{path}': {source}")]
  FileType {
    /// The path of the entry whose file type could not be determined.
    path: PathBuf,
    /// The underlying I/O error from `fs::metadata` or `entry.file_type()`.
    #[source]
    source: io::Error,
  },
}

impl ProjectWorkspaceError {
  /// Creates a canonicalization error for a path that could not be resolved.
  ///
  /// Use this helper when `std::fs::canonicalize` fails during workspace
  /// opening or validation.
  ///
  /// # Parameters
  ///
  /// * `path` is the workspace path that could not be canonicalized.
  /// * `source` is the underlying filesystem error.
  ///
  /// # Returns
  ///
  /// [`ProjectWorkspaceError::Canonicalize`] carrying an owned copy of `path`
  /// and the original I/O error.
  pub(crate) fn canonicalize(path: &Path, source: io::Error) -> Self {
    Self::Canonicalize {
      path: path.to_path_buf(),
      source,
    }
  }

  /// Creates a directory-read error for a path that could not be opened.
  ///
  /// Use this helper when `std::fs::read_dir` fails before iteration begins.
  ///
  /// # Parameters
  ///
  /// * `path` is the directory that could not be read.
  /// * `source` is the underlying filesystem error.
  ///
  /// # Returns
  ///
  /// [`ProjectWorkspaceError::ReadDir`] carrying an owned copy of `path` and
  /// the original I/O error.
  pub(crate) fn read_dir(path: &Path, source: io::Error) -> Self {
    Self::ReadDir {
      path: path.to_path_buf(),
      source,
    }
  }

  /// Creates an entry-read error while iterating a directory.
  ///
  /// Use this helper when one entry returned by a directory iterator cannot be
  /// read even though opening the parent directory succeeded.
  ///
  /// # Parameters
  ///
  /// * `path` is the parent directory being iterated when the entry read failed.
  /// * `source` is the underlying filesystem error.
  ///
  /// # Returns
  ///
  /// [`ProjectWorkspaceError::ReadEntry`] carrying an owned copy of `path` and
  /// the original I/O error.
  pub(crate) fn read_entry(path: &Path, source: io::Error) -> Self {
    Self::ReadEntry {
      path: path.to_path_buf(),
      source,
    }
  }

  /// Creates a file-type error for a path whose metadata could not be read.
  ///
  /// Use this helper when classifying a project tree entry fails.
  ///
  /// # Parameters
  ///
  /// * `path` is the entry path whose file type could not be determined.
  /// * `source` is the underlying filesystem error.
  ///
  /// # Returns
  ///
  /// [`ProjectWorkspaceError::FileType`] carrying an owned copy of `path` and
  /// the original I/O error.
  pub(crate) fn file_type(path: &Path, source: io::Error) -> Self {
    Self::FileType {
      path: path.to_path_buf(),
      source,
    }
  }
}
