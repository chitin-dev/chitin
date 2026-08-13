# Shell completion

Generate a completion script for the shell you use:

```bash
chitin completions bash
chitin completions zsh
chitin completions fish
chitin completions powershell
```

## Zsh

```bash
mkdir -p ~/.zfunc
chitin completions zsh > ~/.zfunc/_chitin
```

Add the following to `~/.zshrc`:

```zsh
fpath=(~/.zfunc $fpath)
autoload -U compinit
compinit
```

Restart the shell or run `source ~/.zshrc`.

## Bash

Install the generated completion in the user-level Bash completion directory:

```bash
mkdir -p ~/.local/share/bash-completion/completions
chitin completions bash > ~/.local/share/bash-completion/completions/chitin
```

For the current shell session, load it immediately:

```bash
source ~/.local/share/bash-completion/completions/chitin
```

To load it automatically, ensure Bash completion is enabled in `~/.bashrc`:

```bash
if [[ -r /usr/share/bash-completion/bash_completion ]]; then
  source /usr/share/bash-completion/bash_completion
fi
```

On systems where Bash completion is already configured, opening a new shell is
enough.

## Fish

Fish loads completion files from `~/.config/fish/completions`:

```fish
mkdir -p ~/.config/fish/completions
chitin completions fish > ~/.config/fish/completions/chitin.fish
```

Start a new Fish shell, or reload the current one with:

```fish
source ~/.config/fish/completions/chitin.fish
```

## PowerShell

Save the completion script next to the active PowerShell profile. This works
with Windows PowerShell 5.1, PowerShell 7 on Windows, and PowerShell 7 on
Unix-like systems:

```powershell
$profileDirectory = Split-Path -Parent $PROFILE
$completionPath = Join-Path $profileDirectory "chitin.ps1"
New-Item -ItemType Directory -Force $profileDirectory | Out-Null
chitin completions powershell > $completionPath
```

Add this line to your PowerShell profile (`$PROFILE`):

```powershell
. (Join-Path (Split-Path -Parent $PROFILE) "chitin.ps1")
```

Then reload the profile:

```powershell
. $PROFILE
```

If PowerShell blocks local scripts, allow scripts for the current user once:

```powershell
Set-ExecutionPolicy -Scope CurrentUser RemoteSigned
```

Completion covers the command hierarchy, options, and the `pdb`/`mmcif` format
values. It does not query RCSB for dynamic PDB identifier suggestions.
