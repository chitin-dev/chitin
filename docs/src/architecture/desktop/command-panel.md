# Command panel

The command panel is a searchable, keyboard-friendly launcher inside the
desktop workspace. It provides one place to discover workspace, database, and
application actions without exposing provider-specific behavior to the search
list.

## User-facing modes

- **Search** filters available commands and keeps one result selected.
- **Form** collects the input required by a command such as an RCSB download.
- **Closed** returns focus and pointer interaction to the workspace.

The panel is modal while open: Escape closes it, Enter activates the selected
command, and the previous focus target is restored on close.

## Command model

Each command has a stable identifier, a title, optional keywords and shortcut
metadata, and an invocation kind:

- an immediate command executes at once;
- a form command changes the panel to an input state before execution.

Search ranking is presentation policy. It must not alter the typed command or
the command's semantic payload.

## Separation of concerns

The panel coordinates three responsibilities:

```text
command discovery  →  user input  →  application action
```

Reusable input, selection, and progress controls own interaction semantics.
The desktop layer owns command registration, focus transitions, validation, and
background task presentation. Provider and domain crates own network and
structure operations. This keeps the search UI independent of any particular
database or file format.

## Download forms

The RCSB form validates the structure identifier and selected format before
starting a download. While the transfer is active, the form reports either a
determinate percentage when a total size is known or an indeterminate progress
state otherwise. A successful download reports its final artifact path; a
failure leaves the form open so the user can correct the input or retry.
