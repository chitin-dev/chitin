use std::{io, path::PathBuf};

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
