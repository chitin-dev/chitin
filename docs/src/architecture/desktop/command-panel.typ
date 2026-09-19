#import "/book.typ": book-page
#show: book-page

#let source-root = "https://github.com/chitin-dev/chitin/blob/main/crates/chitin-desktop/src"

= 1 Command panel
<command-panel>

The command panel is a searchable, keyboard-friendly launcher inside the
desktop workspace. It provides one place to discover workspace, database, and
application actions without exposing provider-specific behavior to the search
list.

The desktop implementation is assembled in
#link(source-root + "/components/command_panel.rs")[`components/command_panel.rs`].
The panel owns interaction state and command presentation; command providers
own domain operations and background work.

== 1.1 User-facing modes
<user-facing-modes>

- #strong[Search] filters available commands and keeps one result selected.
- #strong[Form] collects the input required by a command such as an RCSB
  download.
- #strong[Closed] returns focus and pointer interaction to the workspace.

The panel is modal while open: Escape closes it, Enter activates the selected
command, and the previous focus target is restored on close. These interaction
rules belong to the component rather than to individual command providers.

== 1.2 Command model
<command-model>

Each command has a stable identifier, a title, optional keywords and shortcut
metadata, and an invocation kind:

- an immediate command executes at once;
- a form command changes the panel to an input state before execution.

Search ranking is presentation policy. It must not alter the typed command or
the command's semantic payload. A command can therefore be renamed or
reordered without changing the operation it invokes.

== 1.3 Separation of concerns
<separation-of-concerns>

The panel coordinates three steps: command discovery, user input, and
application action. Reusable input, selection, and progress controls own their
interaction semantics. The desktop layer owns command registration, focus
transitions, validation, and background-task presentation. Provider and domain
crates own network and structure operations.

This separation keeps the search UI independent of any particular database or
file format. It also makes the same domain operation available from a command
palette, a menu, or a future browser interface.

== 1.4 Download forms
<download-forms>

The RCSB form validates the structure identifier and selected format before
starting a download. While the transfer is active, the form reports either a
determinate percentage when a total size is known or an indeterminate progress
state otherwise.

A successful download reports its final artifact path. A failure leaves the form
open so the user can correct the input or retry. The form should not parse the
downloaded structure itself; parsing remains the responsibility of
`chitin-bio` after the file has been written.
