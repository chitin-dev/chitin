//! Platform-specific discovery of shells that can back native PTY sessions.

use std::{
  env,
  ffi::OsString,
  fs,
  path::{Path, PathBuf},
};

/// A discovered shell that can be launched in a native pseudo-terminal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShellDefinition {
  /// Stable identity used by terminal profile selectors.
  pub id: String,
  /// Name shown to the user.
  pub label: String,
  /// Resolved executable, independent of later `PATH` changes.
  pub executable: PathBuf,
  /// Arguments passed when the shell starts.
  pub arguments: Vec<OsString>,
}

impl ShellDefinition {
  fn new(id: &str, label: &str, executable: PathBuf, arguments: &[&str]) -> Self {
    Self {
      id: id.into(),
      label: label.into(),
      executable,
      arguments: arguments.iter().map(OsString::from).collect(),
    }
  }
}

/// Shells available to the current desktop process.
#[derive(Clone, Debug, Default)]
pub struct ShellCatalog {
  definitions: Vec<ShellDefinition>,
}

impl ShellCatalog {
  /// Discovers launchable system shells using the current platform's conventions.
  ///
  /// # Returns
  ///
  /// A snapshot of available shells; applications should rediscover after changes to
  /// installed shells or the process environment. The Chitin built-in shell is not
  /// included because it does not require an executable or PTY.
  pub fn discover() -> Self {
    let mut catalog = Self::default();
    platform::discover(&mut catalog);
    catalog
  }

  /// Returns the discovered launchable definitions in display order.
  pub fn available(&self) -> &[ShellDefinition] {
    &self.definitions
  }

  /// Looks up a discovered shell by its stable selector identity.
  pub fn get(&self, id: &str) -> Option<&ShellDefinition> {
    self.definitions.iter().find(|definition| definition.id == id)
  }

  fn add(&mut self, id: &str, label: &str, executable: PathBuf, arguments: &[&str]) {
    if self.get(id).is_none() && is_executable(&executable) {
      self
        .definitions
        .push(ShellDefinition::new(id, label, executable, arguments));
    }
  }

  #[cfg(windows)]
  fn add_named(&mut self, id: &str, label: &str, name: &str, arguments: &[&str]) {
    if let Some(executable) = find_on_path(name) {
      self.add(id, label, executable, arguments);
    }
  }
}

fn find_on_path(name: &str) -> Option<PathBuf> {
  let paths = env::var_os("PATH")?;
  for directory in env::split_paths(&paths) {
    for candidate in executable_names(name) {
      let path = directory.join(candidate);
      if is_executable(&path) {
        return Some(path);
      }
    }
  }
  None
}

#[cfg(windows)]
fn executable_names(name: &str) -> Vec<OsString> {
  let path = Path::new(name);
  if path.extension().is_some() {
    return vec![OsString::from(name)];
  }
  let extensions = env::var_os("PATHEXT").unwrap_or_else(|| OsString::from(".COM;.EXE;.BAT;.CMD"));
  extensions
    .to_string_lossy()
    .split(';')
    .filter(|extension| !extension.is_empty())
    .map(|extension| OsString::from(format!("{name}{extension}")))
    .collect()
}

#[cfg(not(windows))]
fn executable_names(name: &str) -> Vec<OsString> {
  vec![OsString::from(name)]
}

pub(crate) fn is_executable(path: &Path) -> bool {
  let Ok(metadata) = fs::metadata(path) else {
    return false;
  };
  if !metadata.is_file() {
    return false;
  }
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
  }
  #[cfg(not(unix))]
  {
    true
  }
}

#[cfg(unix)]
mod platform {
  use super::*;

  pub(super) fn discover(catalog: &mut ShellCatalog) {
    if let Some(default) = env::var_os("SHELL").map(PathBuf::from)
      && is_executable(&default)
    {
      catalog.add("system-shell", "Default shell", default, &[]);
    }

    // /etc/shells records login shells that might live outside the GUI process's PATH.
    let registered = fs::read_to_string("/etc/shells").unwrap_or_default();
    for (id, label, name) in [
      ("bash", "Bash", "bash"),
      ("zsh", "Zsh", "zsh"),
      ("fish", "Fish", "fish"),
      ("nushell", "Nushell", "nu"),
      ("pwsh", "PowerShell 7", "pwsh"),
    ] {
      let registered_path = registered
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with('/') && !line.starts_with('#'))
        .map(PathBuf::from)
        .find(|path| path.file_name().is_some_and(|file| file == name) && is_executable(path));
      if let Some(path) = registered_path.or_else(|| find_on_path(name)) {
        catalog.add(id, label, path, &[]);
      }
    }
    if catalog.get("system-shell").is_none()
      && let Some(path) = find_on_path("sh")
    {
      catalog.add("system-shell", "Default shell", path, &[]);
    }
  }
}

#[cfg(windows)]
mod platform {
  use super::*;

  pub(super) fn discover(catalog: &mut ShellCatalog) {
    let system32 = env::var_os("SystemRoot")
      .map(PathBuf::from)
      .map(|root| root.join("System32"));
    if let Some(path) = env::var_os("ComSpec").map(PathBuf::from)
      && is_executable(&path)
    {
      catalog.add("system-shell", "Default shell", path, &[]);
    }
    if catalog.get("system-shell").is_none() {
      catalog.add_named("system-shell", "Default shell", "cmd", &[]);
    }
    if catalog.get("system-shell").is_none()
      && let Some(system32) = &system32
    {
      catalog.add("system-shell", "Default shell", system32.join("cmd.exe"), &[]);
    }
    catalog.add_named("cmd", "Command Prompt", "cmd", &[]);
    if let Some(system32) = &system32 {
      catalog.add("cmd", "Command Prompt", system32.join("cmd.exe"), &[]);
    }
    catalog.add_named("powershell", "Windows PowerShell", "powershell", &[]);
    if let Some(system32) = &system32 {
      catalog.add(
        "powershell",
        "Windows PowerShell",
        system32.join("WindowsPowerShell/v1.0/powershell.exe"),
        &[],
      );
    }
    catalog.add_named("pwsh", "PowerShell 7", "pwsh", &[]);
    for variable in ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"] {
      if let Some(root) = env::var_os(variable).map(PathBuf::from) {
        catalog.add("pwsh", "PowerShell 7", root.join("PowerShell/7/pwsh.exe"), &[]);
      }
    }
    catalog.add_named("nushell", "Nushell", "nu", &[]);
    catalog.add_named("bash", "Bash", "bash", &[]);
    catalog.add_named("fish", "Fish", "fish", &[]);

    // Git for Windows places git.exe on PATH but its interactive bash.exe under bin/.
    if let Some(git) = find_on_path("git")
      && let Some(root) = git.parent().and_then(Path::parent)
    {
      catalog.add("git-bash", "Git Bash", root.join("bin/bash.exe"), &["--login", "-i"]);
    }
    for variable in ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"] {
      if let Some(root) = env::var_os(variable).map(PathBuf::from) {
        catalog.add(
          "git-bash",
          "Git Bash",
          root.join("Git/bin/bash.exe"),
          &["--login", "-i"],
        );
      }
    }
  }
}

#[cfg(not(any(unix, windows)))]
mod platform {
  use super::*;

  pub(super) fn discover(catalog: &mut ShellCatalog) {
    let _ = catalog;
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn catalog_omits_non_executable_shells() {
    let mut catalog = ShellCatalog::default();
    catalog.add("missing", "Missing", PathBuf::from("/not/a/chitin/shell"), &[]);
    assert!(catalog.available().is_empty());
  }

  #[test]
  fn catalog_keeps_first_definition_for_a_stable_id() -> std::io::Result<()> {
    let mut catalog = ShellCatalog::default();
    let executable = env::current_exe()?;
    catalog.add("test", "First", executable.clone(), &[]);
    catalog.add("test", "Second", executable, &[]);
    assert_eq!(catalog.get("test").map(|shell| shell.label.as_str()), Some("First"));
    Ok(())
  }

  #[cfg(unix)]
  #[test]
  fn catalog_discovers_pwsh_when_installed() {
    if find_on_path("pwsh").is_some() {
      assert!(ShellCatalog::discover().get("pwsh").is_some());
    }
  }
}
