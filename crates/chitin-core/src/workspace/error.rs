use core::fmt;
use std::{
  error::Error,
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
#[derive(Debug)]
pub enum ProjectWorkspaceError {
  /// The requested workspace path does not exist on disk.
  NotFound(PathBuf),

  /// The requested path exists but is a file, not a directory. It will be treated
  /// using different logic.
  NotDirectory(PathBuf),

  /// Failed to resolve the path to an absolute (canonical) form.
  /// This typically occurs due to permissions issues or broken symlinks.
  Canonicalize {
    /// The path that could not be canonicalized.
    path: PathBuf,
    /// The underlying I/O error from `std::fs::canonicalize`.
    source: io::Error,
  },

  /// Failed to read the directory contents during workspace scanning.
  ReadDir {
    /// The directory path that could not be read.
    path: PathBuf,
    /// The underlying I/O error from `std::fs::read_dir`.
    source: io::Error,
  },

  /// Failed to read a specific directory entry (file or subdirectory).
  ///
  /// This is different from `ReadDir` in that it occurs while iterating
  /// entries, rather than when opening the directory itself.
  ReadEntry {
    /// The path of the specific entry that caused the error.
    path: PathBuf,
    /// The underlying I/O error from the entry's metadata read.
    source: io::Error,
  },

  /// Failed to determine the file type of a directory entry.
  ///
  /// This can happen when the filesystem does not support file type
  /// metadata or when the entry is a broken symlink.
  FileType {
    /// The path of the entry whose file type could not be determined.
    path: PathBuf,
    /// The underlying I/O error from `fs::metadata` or `entry.file_type()`.
    source: io::Error,
  },
}

impl ProjectWorkspaceError {
  /// Creates a `Canonicalize` error for a path that could not be resolved
  /// to its absolute form.
  ///
  /// This is a convenience constructor for `ProjectWorkspaceError::Canonicalize`.
  /// Use this when `std::fs::canonicalize()` fails.
  ///
  /// # Arguments
  /// * `path` - The path that could not be canonicalized.
  /// * `source` - The underlying I/O error from `std::fs::canonicalize`.
  pub(crate) fn canonicalize(path: &Path, source: io::Error) -> Self {
    Self::Canonicalize {
      path: path.to_path_buf(),
      source,
    }
  }

  /// Creates a `ReadDir` error for a directory that could not be read.
  ///
  /// This is a convenience constructor for `ProjectWorkspaceError::ReadDir`.
  /// Use this when `std::fs::read_dir()` fails.
  ///
  /// # Arguments
  /// * `path` - The directory path that could not be read.
  /// * `source` - The underlying I/O error from `std::fs::read_dir`.
  pub(crate) fn read_dir(path: &Path, source: io::Error) -> Self {
    Self::ReadDir {
      path: path.to_path_buf(),
      source,
    }
  }

  /// Creates a `ReadEntry` error for a directory entry that could not be read.
  ///
  /// This is a convenience constructor for `ProjectWorkspaceError::ReadEntry`.
  /// Use this when iterating over directory entries and an individual entry
  /// cannot be read, such as when `DirEntry::metadata()` or `DirEntry::file_type()`
  /// fails.
  ///
  /// # Arguments
  /// * `path` - The path of the directory entry that could not be read.
  /// * `source` - The underlying I/O error.
  pub(crate) fn read_entry(path: &Path, source: io::Error) -> Self {
    Self::ReadEntry {
      path: path.to_path_buf(),
      source,
    }
  }

  /// Creates a `FileType` error for a path whose file type could not be determined.
  ///
  /// This is a convenience constructor for `ProjectWorkspaceError::FileType`.
  /// Use this when `std::fs::metadata()` or `DirEntry::file_type()` fails.
  ///
  /// # Arguments
  /// * `path` - The path whose file type could not be determined.
  /// * `source` - The underlying I/O error.
  pub(crate) fn file_type(path: &Path, source: io::Error) -> Self {
    Self::FileType {
      path: path.to_path_buf(),
      source,
    }
  }
}

/// Formats a `ProjectWorkspaceError` for human-readable display.
///
/// This implementation provides user-friendly error messages for each variant of
/// the `ProjectWorkspaceError` enum. It is intended to be used with `println!`,
/// `format!`, or `to_string()` when reporting errors to users.
///
/// # Examples
///
/// ```
/// use std::path::PathBuf;
/// use chitin_core::workspace::ProjectWorkspaceError;
///
/// let err = ProjectWorkspaceError::NotFound(PathBuf::from("/nonexistent"));
/// assert_eq!(
///   err.to_string(),
///   "project path doesn't exist: /nonexistent"
/// );
/// ```
///
/// # Note
///
/// For error variants that carry a `source` field, the source error is intentionally
/// omitted from the display output to keep messages clean and user-focused.
/// Use the `source()` method to access the underlying cause when needed.
impl fmt::Display for ProjectWorkspaceError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::NotFound(path) => {
        write!(formatter, "project path doesn't exist: {}", path.display())
      }
      Self::NotDirectory(path) => {
        write!(
          formatter,
          "project path is not a directory: {}",
          path.display()
        )
      }
      Self::Canonicalize { path, .. } => {
        write!(
          formatter,
          "failed to canonicalize project path: {}",
          path.display()
        )
      }
      Self::ReadDir { path, .. } => {
        write!(formatter, "failed to read directory: {}", path.display())
      }
      Self::ReadEntry { path, .. } => {
        write!(
          formatter,
          "failed to read directory entry under: {}",
          path.display()
        )
      }
      Self::FileType { path, .. } => {
        write!(
          formatter,
          "failed to read file type for: {}",
          path.display()
        )
      }
    }
  }
}

/// Returns the underlying cause of this error, if any.
///
/// For filesystem-related errors, this method exposes the original `std::io::Error`
/// that caused the failure. This allows callers to inspect the root cause when
/// additional context is needed beyond the user-facing error message.
///
/// # Return Value
///
/// - Returns `Some(source)` for errors that wrap an `std::io::Error`:
///   - `Canonicalize`
///   - `ReadDir`
///   - `ReadEntry`
///   - `FileType`
///
/// - Returns `None` for errors that do not wrap an underlying error:
///   - `NotFound` (path simply doesn't exist)
///   - `NotDirectory` (path is not a directory)
///
/// # Example
///
/// ```
/// use std::path::PathBuf;
/// use std::error::Error;
/// use chitin_core::workspace::ProjectWorkspaceError;
///
/// let err = ProjectWorkspaceError::ReadDir {
///   path: PathBuf::from("/tmp"),
///   source: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "permission denied"),
/// };
///
/// assert!(err.source().is_some());
/// assert_eq!(err.source().unwrap().to_string(), "permission denied");
/// ```
///
/// # Note
///
/// The `source()` method is part of Rust's standard error handling ecosystem,
/// used by libraries like `anyhow` and `eyre` to build error chains.
/// The `Display` implementation intentionally omits the source error to keep
/// user-facing messages clean; callers can use this method to retrieve it
/// for logging or diagnostic purposes.
impl Error for ProjectWorkspaceError {
  fn source(&self) -> Option<&(dyn Error + 'static)> {
    match self {
      Self::NotFound(_) | Self::NotDirectory(_) => None,
      Self::Canonicalize { source, .. }
      | Self::ReadDir { source, .. }
      | Self::ReadEntry { source, .. }
      | Self::FileType { source, .. } => Some(source),
    }
  }
}
