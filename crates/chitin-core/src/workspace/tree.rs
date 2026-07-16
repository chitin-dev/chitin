use std::{
  cmp::Ordering,
  ffi::OsStr,
  fs,
  path::{Path, PathBuf},
};

use crate::workspace::ProjectWorkspaceError;

/// Directories that should be hidden from the project tree UI.
///
/// Unlike `.gitignore`, this list is purely for visual cleanliness in the file
/// tree. It hides directories that are internal implementation details of tools
/// and should not be directly manipulated by users.
///
/// Currently only hides version control metadata:
/// - `.git/` — Git repository database; editing it manually can corrupt the repo
///
/// # Note
///
/// This list is intentionally minimal. Build outputs, package directories, and
/// cache directories remain visible because users may legitimately need to
/// inspect them. Their contents should be loaded lazily by the UI instead of
/// hidden here.
const NOT_DISPLAYED_DIRS: &[&str] = &[".git"];

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

impl ProjectTreeEntry {
  /// Returns `true` if this entry represents a directory.
  pub fn is_dir(&self) -> bool {
    self.kind == ProjectTreeEntryKind::Directory
  }

  /// Returns `true` if this entry represents a file.
  pub fn is_file(&self) -> bool {
    self.kind == ProjectTreeEntryKind::File
  }
}

/// Extracts a user-friendly display name from a filesystem path.
///
/// This function attempts to retrieve the final component of the path (the file
/// or directory name) as a UTF-8 string. If the path has no final component
/// (e.g., the root directory) or the filename contains invalid UTF-8 bytes,
/// it falls back to the full path representation.
///
/// # Fallback Behavior
///
/// - For paths with a valid UTF-8 filename: returns the filename as `String`
/// - For root directories (`/`, `C:\`): returns the full path
/// - For paths with non-UTF-8 filenames: returns the full path
///
/// # Note
///
/// This function never panics and always returns a `String`. It is intended for
/// UI display purposes where showing some string is better than crashing.
///
/// # See Also
///
/// - [`std::path::Path::file_name`] for the underlying method
/// - [`std::ffi::OsStr::to_str`] for UTF-8 conversion behavior
fn display_name(path: &Path) -> String {
  path
    .file_name()
    .and_then(OsStr::to_str)
    .map_or_else(|| path.display().to_string(), ToOwned::to_owned)
}

/// Determines whether a path points to a directory that should be hidden from
/// the project tree UI.
///
/// # Returns
///
/// - `true` if the path's final component is in NOT_DISPLAYED_DIRECTORIES
/// - `false` otherwise (including edge cases like root directories or
///   non-UTF-8 filenames)
///
/// # Note
///
/// This function is for UI filtering only and does not affect filesystem
/// operations. Users can still access hidden directories through other means
/// (e.g., terminal, file explorer "show hidden files").
///
/// The list is intentionally minimal—only directories that are:
/// - Internal implementation details of version control systems
/// - Dangerous to modify manually
/// - Not useful for everyday development tasks
fn is_not_displayed_directory(path: &Path) -> bool {
  path
    .file_name()
    .and_then(OsStr::to_str)
    .is_some_and(|name| NOT_DISPLAYED_DIRS.contains(&name))
}

/// Compares two project tree entries for sorting.
///
/// This function defines the sorting order for entries in a project tree:
/// 1. **Directories** always appear before **files** (directories first).
/// 2. Entries are sorted **case-insensitively** by name (`A` and `a` are treated
///    as equal for ordering purposes).
/// 3. When two names are equal case-insensitively, the **original case** is used
///    as a tie-breaker (e.g., `README.md` comes before `readme.md`).
///
/// # Ordering Rules
///
/// | Left      | Right     | Result                                              |
/// |-----------|-----------|-----------------------------------------------------|
/// | Directory | File      | `Less` (directory first)                            |
/// | File      | Directory | `Greater` (file after directory)                    |
/// | Directory | Directory | Compare by name (case-insensitive → case-sensitive) |
/// | File      | File      | Compare by name (case-insensitive → case-sensitive) |
///
/// # Note
///
/// This function is intended for UI sorting in project tree views where
/// directories should be visually grouped together before files.
fn compare_entries(left: &ProjectTreeEntry, right: &ProjectTreeEntry) -> Ordering {
  match (left.is_dir(), right.is_dir()) {
    (true, false) => Ordering::Less,
    (false, true) => Ordering::Greater,
    _ => {
      let left_lower_name = left.name.to_lowercase();
      let right_lower_name = right.name.to_lowercase();

      left_lower_name
        .cmp(&right_lower_name)
        .then_with(|| left.name.cmp(&right.name))
    }
  }
}

/// Builds a shallow project tree entry for a given directory path.
///
/// This function reads only the direct children of `path`. Child directories are
/// represented as expandable nodes with empty `children`; their contents should
/// be loaded later when the user expands that node.
///
/// # Behavior
///
/// - **Directories**: Added as child nodes without recursively reading them.
/// - **Files**: Added as leaf nodes with no children.
/// - **Filtering**: Directories matching `is_not_displayed_directory()` are
///   skipped entirely.
/// - **Sorting**: Children are sorted using `compare_entries()` (directories
///   first, then case-insensitive alphabetical order).
///
/// # Errors
///
/// Returns `ProjectWorkspaceError` for any filesystem operation failure:
/// - `ReadDir`: Failed to read the directory contents.
/// - `ReadEntry`: Failed to read a specific directory entry's metadata.
/// - `FileType`: Failed to determine a path's file type.
///
/// # Note
///
/// Keeping this function shallow is important for desktop startup performance:
/// large generated directories such as `target` or `node_modules` stay visible
/// but are not traversed until the UI asks for them.
fn build_directory_entry(path: &Path) -> Result<ProjectTreeEntry, ProjectWorkspaceError> {
  let mut children = Vec::new();

  for entry in fs::read_dir(path).map_err(|source| ProjectWorkspaceError::read_dir(path, source))? {
    let entry = entry.map_err(|source| ProjectWorkspaceError::read_entry(path, source))?;
    let child_path = entry.path();
    let file_type = entry
      .file_type()
      .map_err(|source| ProjectWorkspaceError::file_type(&child_path, source))?;

    if file_type.is_dir() && is_not_displayed_directory(&child_path) {
      continue;
    }
    if file_type.is_dir() {
      children.push(ProjectTreeEntry {
        path: child_path.clone(),
        name: display_name(&child_path),
        kind: ProjectTreeEntryKind::Directory,
        children: Vec::new(),
      });
    } else if file_type.is_file() {
      children.push(ProjectTreeEntry {
        path: child_path.clone(),
        name: display_name(&child_path),
        kind: ProjectTreeEntryKind::File,
        children: Vec::new(),
      });
    }
  }

  children.sort_by(compare_entries);

  Ok(ProjectTreeEntry {
    path: path.to_path_buf(),
    name: display_name(path),
    kind: ProjectTreeEntryKind::Directory,
    children,
  })
}

/// Builds a project tree entry for a given filesystem path.
///
/// This function determines the type of the path (file or directory) and
/// constructs the appropriate `ProjectTreeEntry`:
/// - For **directories**: delegates to `build_directory_entry()` to build the
///   immediate child list.
/// - For **files**: creates a leaf entry with no children.
///
/// # Behavior
///
/// - The path is first checked for its metadata to determine if it is a file,
///   directory, or other special file type (symlinks are followed).
/// - For directories, only direct children are read. Nested directories are
///   loaded on demand through [`ProjectWorkspace::load_directory_children`].
/// - For files, a simple leaf node is created without further processing.
///
/// # Errors
///
/// Returns `ProjectWorkspaceError::FileType` if the path's metadata cannot be
/// read (e.g., permission denied, broken symlink, or path does not exist).
///
fn build_entry(path: &Path) -> Result<ProjectTreeEntry, ProjectWorkspaceError> {
  let file_type = path
    .metadata()
    .map_err(|source| ProjectWorkspaceError::file_type(path, source))?
    .file_type();

  if file_type.is_dir() {
    build_directory_entry(path)
  } else {
    Ok(ProjectTreeEntry {
      path: path.to_path_buf(),
      name: display_name(path),
      kind: ProjectTreeEntryKind::File,
      children: Vec::new(),
    })
  }
}

impl ProjectWorkspace {
  /// Opens a workspace from a given filesystem path.
  ///
  /// This function initializes a new `ProjectWorkspace` by validating the provided
  /// path and building a hierarchical tree representation of its contents. It
  /// serves as the primary entry point for loading a project into the application.
  ///
  /// # Behavior
  ///
  /// - **Path resolution**: The provided path is canonicalized to resolve any
  ///   symlinks and obtain an absolute, normalized path.
  /// - **Validation**: The path must exist and point to a directory. Files are
  ///   currently rejected with [`ProjectWorkspaceError::NotDirectory`].
  /// - **Tree construction**: If the path is valid, a shallow `ProjectTree` is
  ///   built for the root. Nested directories are loaded on user expansion.
  ///
  /// # Arguments
  ///
  /// * `path` - A filesystem path to the workspace root directory. Accepts any
  ///   type that implements `AsRef<Path>` (e.g., `&str`, `String`, `PathBuf`).
  ///
  /// # Returns
  ///
  /// * `Ok(Self)` - A new `ProjectWorkspace` instance with the validated root
  ///   path and a fully built project tree.
  /// * `Err(ProjectWorkspaceError)` - If the path does not exist, is not a
  ///   directory, or cannot be canonicalized.
  ///
  /// # Errors
  ///
  /// | Error variant                    | Condition                                                                |
  /// |----------------------------------|--------------------------------------------------------------------------|
  /// | `NotFound`                       | The path does not exist on the filesystem.                               |
  /// | `Canonicalize`                   | Failed to resolve symlinks or obtain an absolute path.                   |
  /// | `NotDirectory`                   | The path exists but is not a directory.                                  |
  /// | `ReadDir`/`ReadEntry`/`FileType` | Filesystem errors while reading direct children.                         |
  ///
  /// # Note
  ///
  /// This function performs synchronous filesystem I/O for the root directory
  /// only. Directory expansion should still move to a background task once the
  /// UI supports async loading indicators.
  pub fn open(path: impl AsRef<Path>) -> Result<Self, ProjectWorkspaceError> {
    let path_reference = path.as_ref();
    if !path_reference.exists() {
      return Err(ProjectWorkspaceError::NotFound(
        path_reference.to_path_buf(),
      ));
    }

    let root = path_reference
      .canonicalize()
      .map_err(|source| ProjectWorkspaceError::canonicalize(path_reference, source))?;

    if !root.is_dir() {
      return Err(ProjectWorkspaceError::NotDirectory(root));
    }

    let tree = ProjectTree {
      root: build_entry(&root)?,
    };

    Ok(Self { root, tree })
  }

  /// Loads the direct children of `path` without recursively reading
  /// descendants.
  ///
  /// This is the core operation the UI should call when a collapsed directory
  /// row is expanded. Directories such as `target` and `node_modules` remain
  /// visible, but their contents are only read when the user explicitly asks.
  pub fn load_directory_children(
    path: impl AsRef<Path>,
  ) -> Result<Vec<ProjectTreeEntry>, ProjectWorkspaceError> {
    Ok(build_directory_entry(path.as_ref())?.children)
  }
}

#[cfg(test)]
mod tests {
  use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
  };

  use super::*;

  struct TestProject {
    root: PathBuf,
  }

  impl TestProject {
    fn new(name: &str) -> Result<Self, Box<dyn Error>> {
      let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
      let root =
        std::env::temp_dir().join(format!("chitin-{name}-{}-{timestamp}", std::process::id()));

      fs::create_dir(&root)?;

      Ok(Self { root })
    }

    fn path(&self) -> &Path {
      &self.root
    }

    fn mkdir(&self, path: impl AsRef<Path>) -> Result<(), Box<dyn Error>> {
      fs::create_dir_all(self.root.join(path))?;
      Ok(())
    }

    fn touch(&self, path: impl AsRef<Path>) -> Result<(), Box<dyn Error>> {
      let path = self.root.join(path);

      if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
      }

      fs::write(path, "")?;
      Ok(())
    }
  }

  impl Drop for TestProject {
    fn drop(&mut self) {
      let _ = fs::remove_dir_all(&self.root);
    }
  }

  fn entry(name: &str, kind: ProjectTreeEntryKind) -> ProjectTreeEntry {
    ProjectTreeEntry {
      path: PathBuf::from(name),
      name: name.to_string(),
      kind,
      children: Vec::new(),
    }
  }

  fn child_names(entry: &ProjectTreeEntry) -> Vec<&str> {
    entry
      .children
      .iter()
      .map(|child| child.name.as_str())
      .collect()
  }

  #[test]
  fn display_name_should_return_final_path_component() {
    let path = Path::new("/home/user/project/Cargo.toml");
    assert_eq!(display_name(path), "Cargo.toml");
  }

  #[test]
  fn display_name_should_fallback_to_path_display_for_root_path() {
    let path = Path::new("/");
    assert_eq!(display_name(path), "/");
  }

  #[test]
  fn is_not_displayed_directory_should_hide_git_directory() {
    assert!(is_not_displayed_directory(Path::new("/project/.git")));
  }

  #[test]
  fn is_not_displayed_directory_should_not_hide_regular_or_generated_directory() {
    assert!(!is_not_displayed_directory(Path::new("/project/src")));
    assert!(!is_not_displayed_directory(Path::new("/project/target")));
    assert!(!is_not_displayed_directory(Path::new(
      "/project/node_modules"
    )));
  }

  #[test]
  fn compare_entries_should_sort_directories_before_files() {
    let directory = entry("src", ProjectTreeEntryKind::Directory);
    let file = entry("Cargo.toml", ProjectTreeEntryKind::File);
    assert_eq!(compare_entries(&directory, &file), Ordering::Less);
    assert_eq!(compare_entries(&file, &directory), Ordering::Greater);
  }

  #[test]
  fn compare_entries_should_sort_names_case_insensitively() {
    let left = entry("alpha.rs", ProjectTreeEntryKind::File);
    let right = entry("Beta.rs", ProjectTreeEntryKind::File);
    assert_eq!(compare_entries(&left, &right), Ordering::Less);
  }

  #[test]
  fn project_workspace_open_should_reject_missing_path() -> Result<(), Box<dyn Error>> {
    let project = TestProject::new("missing-path")?;
    let missing_path = project.path().join("missing");
    // This path doesn't exist

    let Err(error) = ProjectWorkspace::open(&missing_path) else {
      return Err("missing path should return ProjectWorkspaceError::NotFound".into());
    };

    assert!(matches!(error, ProjectWorkspaceError::NotFound(path) if path == missing_path));
    Ok(())
  }

  #[test]
  fn project_workspace_open_should_reject_file_path() -> Result<(), Box<dyn Error>> {
    let project = TestProject::new("file-path")?;
    project.touch("Cargo.toml")?;
    let file_path = project.path().join("Cargo.toml").canonicalize()?;

    let Err(error) = ProjectWorkspace::open(&file_path) else {
      return Err("file path should return ProjectWorkspaceError::NotDirectory".into());
    };

    assert!(matches!(error, ProjectWorkspaceError::NotDirectory(path) if path == file_path));
    Ok(())
  }

  #[test]
  fn project_workspace_open_should_build_sorted_tree() -> Result<(), Box<dyn Error>> {
    let project = TestProject::new("sorted-tree")?;
    project.mkdir("src")?;
    project.mkdir("Assets")?;
    project.touch("zeta.rs")?;
    project.touch("Alpha.rs")?;

    let workspace = ProjectWorkspace::open(project.path())?;
    let names = child_names(&workspace.tree.root);

    assert_eq!(names, vec!["Assets", "src", "Alpha.rs", "zeta.rs"]);
    Ok(())
  }

  #[test]
  fn project_workspace_open_should_not_recursively_load_nested_directories()
  -> Result<(), Box<dyn Error>> {
    let project = TestProject::new("shallow-tree")?;
    project.touch("src/main.rs")?;

    let workspace = ProjectWorkspace::open(project.path())?;
    let src = workspace
      .tree
      .root
      .children
      .iter()
      .find(|child| child.name == "src")
      .ok_or("src directory should exist")?;

    assert!(src.children.is_empty());
    Ok(())
  }

  #[test]
  fn project_workspace_load_directory_children_should_load_on_demand() -> Result<(), Box<dyn Error>>
  {
    let project = TestProject::new("lazy-children")?;
    project.touch("target/debug/chitin-desktop")?;

    let target_path = project.path().join("target");
    let children = ProjectWorkspace::load_directory_children(&target_path)?;
    let names: Vec<_> = children.iter().map(|child| child.name.as_str()).collect();

    assert_eq!(names, vec!["debug"]);
    Ok(())
  }

  #[test]
  fn project_workspace_open_should_skip_not_displayed_directories() -> Result<(), Box<dyn Error>> {
    let project = TestProject::new("hidden-directories")?;
    project.mkdir(".git")?;
    project.touch(".git/config")?;
    project.mkdir("target")?;
    project.touch("target/debug/chitin-desktop")?;
    project.mkdir("node_modules")?;
    project.touch("node_modules/package/index.js")?;
    project.mkdir("src")?;

    let workspace = ProjectWorkspace::open(project.path())?;
    let names = child_names(&workspace.tree.root);

    assert_eq!(names, vec!["node_modules", "src", "target"]);
    Ok(())
  }
}
