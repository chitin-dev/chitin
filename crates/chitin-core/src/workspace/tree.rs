use std::path::PathBuf;

/// Represent the type of an entry in the project tree.
///
/// Used to distinguish between files and directories during tree traversal and
/// file system operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectTreeEntryKind {
  /// A directory node containing child entries
  Directory,
  /// A file node with no children
  File,
}

/// A node in the project tree representing a file or directory.
///
/// # Recursive Structure
/// This struct forms a recursive tree:
/// - [`Directory`][ProjectTreeEntryKind::Directory] nodes contain [`children`][Self::children].
/// - [`File`][ProjectTreeEntryKind::File] nodes typically have an empty `children` vector.
///
/// # Example
/// ```
/// use std::path::PathBuf;
/// use chitin_core::workspace::{ProjectTreeEntry, ProjectTreeEntryKind};
///
/// let entry = ProjectTreeEntry {
///   path: PathBuf::from("/project/src"),
///   name: "src".to_string(),
///   kind: ProjectTreeEntryKind::Directory,
///   children: vec![
///     ProjectTreeEntry {
///       path: PathBuf::from("/project/src/main.rs"),
///       name: "main.rs".to_string(),
///       kind: ProjectTreeEntryKind::File,
///       children: vec![],
///     }
///   ],
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectTreeEntry {
  /// Full filesystem path of this entry.
  pub path: PathBuf,
  /// Base name of the file or directory (e.g., `"src"`, `"main.rs"`).
  pub name: String,
  /// Whether this entry is a file or a directory.
  pub kind: ProjectTreeEntryKind,
  /// Child entries if this is a directory; empty for files.
  pub children: Vec<ProjectTreeEntry>,
}

/// The root container of a project file tree.
///
/// Wraps a single root [`ProjectTreeEntry`], representing the top-level
/// directory of a project. Provides the entry point for tree traversal,
/// file system operations, and project-level analyses.
///
/// # Example
/// ```
/// use std::path::PathBuf;
/// use chitin_core::workspace::{ProjectTree, ProjectTreeEntry, ProjectTreeEntryKind};
///
/// let root = ProjectTreeEntry {
///   path: PathBuf::from("/my_project"),
///   name: "my_project".to_string(),
///   kind: ProjectTreeEntryKind::Directory,
///   children: vec![],
/// };
///
/// let tree = ProjectTree { root };
/// assert_eq!(tree.root.name, "my_project");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectTree {
  /// The root entry of the project tree. Typically a directory node representing
  /// the project's root folder.
  pub root: ProjectTreeEntry,
}

/// A complete project workspace containing both the root path and its file tree.
///
/// Combines the filesystem location (`root`) with the in-memory tree structure (`tree`).
/// This is the primary container for project-level operations, providing access to
/// the project's files, directories, and their hierarchical relationships.
///
/// # Relationship to Other Types
/// - [`ProjectTree`] stores the recursive directory structure.
/// - [`ProjectTreeEntry`] represents individual file/directory nodes.
/// - [`ProjectTreeEntryKind`] discriminates between file and directory nodes.
///
/// # Example
/// ```
/// use std::path::PathBuf;
/// use chitin_core::workspace::{ProjectWorkspace, ProjectTree, ProjectTreeEntry, ProjectTreeEntryKind};
///
/// let root = PathBuf::from("/my_project");
/// let tree = ProjectTree {
///   root: ProjectTreeEntry {
///     path: root.clone(),
///     name: "my_project".to_string(),
///     kind: ProjectTreeEntryKind::Directory,
///     children: vec![],
///   },
/// };
///
/// let workspace = ProjectWorkspace { root, tree };
/// assert_eq!(workspace.root, PathBuf::from("/my_project"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectWorkspace {
  /// The root directory path of the workspace on disk.
  pub root: PathBuf,
  /// The in-memory recursive tree representation of the workspace.
  pub tree: ProjectTree,
}
