#import "/book.typ": book-page
#show: book-page

= 1 Shell completion
<shell-completion>

Chitin's shell completion is generated from the CLI definition at runtime by
the #link("https://crates.io/crates/clap_complete")[`clap_complete`] crate. The
`chitin` command is defined with #link("https://crates.io/crates/clap")[`clap`],
and the completion generator receives that same command schema, so subcommands,
options, and value choices stay aligned with the CLI instead of being maintained
as separate shell-specific files.

Generate a completion script for the shell you use:

```bash
chitin completions [bash|zsh|fish|powershell]
```

== 1.1 Zsh
<zsh>

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

== 1.2 Bash
<bash>
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

== 1.3 Fish
<fish>
Fish loads completion files from `~/.config/fish/completions`:

```fish
mkdir -p ~/.config/fish/completions
chitin completions fish > ~/.config/fish/completions/chitin.fish
```

Start a new Fish shell, or reload the current one with:

```fish
source ~/.config/fish/completions/chitin.fish
```

== 1.4 PowerShell
<powershell>
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
