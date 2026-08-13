# Command Panel Architecture

The command panel is the desktop shell's searchable command launcher. It has two
visible modes:

- **Search** renders registered commands in a reusable quick-pick list.
- **Form** replaces the list with a command-specific form. The only form
  currently implemented is the RCSB structure download form.

The command panel is not a standalone `chitin-ui` primitive. It coordinates
desktop commands, application state, focus, reusable UI components, and
background work, so its orchestration belongs to `chitin-desktop`.

## Source map

| Module | Responsibility |
| --- | --- |
| `components/command_panel.rs` | Renders the overlay, connects GPUI events to application behavior, invokes commands, and runs the RCSB download. |
| `components/command_panel/controller.rs` | Owns visibility, mode, focus restoration, persistent input entities, and command-panel transitions while composing `QuickPickState`. |
| `components/command_panel/form/rcsb.rs` | Owns and renders the RCSB form's primitive states and download-progress presentation. |
| `commands/command_panel.rs` | Defines command metadata, invocation behavior, registration, search, and ranking. |
| `commands/mod.rs` | Defines the typed `ChitinCommand` hierarchy and the application command bus. |
| `commands/{application,database,tab,workspace}.rs` | Own the commands and descriptors for each feature area. |
| `app.rs` | Owns `CommandPanelController`, installs the global action/key routes, and inserts the overlay into the root layout. |
| `chitin-ui::composite::quickpick` | Provides reusable search-list presentation and `QuickPickState` for query, selection, and virtual-list scrolling. It has no knowledge of desktop commands. |
| `chitin-ui::primitive` | Supplies `TextInput`, `Select`, `Button`, and `Progress` behavior used by the panel and its forms. |
| `chitin-databases` | Supplies the RCSB client, request types, identifiers, formats, transport, and download progress callback. |

The dependency direction is:

```text
chitin-desktop command panel
├── desktop command registry and dispatch
├── chitin-ui quick-pick and primitives
└── chitin-databases RCSB provider

chitin-ui and chitin-databases do not depend on chitin-desktop.
```

## Core data model

### `ChitinCommand`

`ChitinCommand` is the typed command boundary used inside the desktop app. Its
variants delegate ownership to feature-specific enums:

```text
ChitinCommand
├── Workspace(WorkspaceCommand)
├── Database(DatabaseCommand)
└── Application(ApplicationCommand)
```

Internal code should pass this enum instead of parsing command IDs. The stable
dotted ID returned by `ChitinCommand::id` is intended for configuration, search,
keybinding, or future plugin boundaries.

### `CommandDescriptor`

A descriptor connects searchable metadata to a typed command:

| Field | Meaning |
| --- | --- |
| `id` | Stable dotted identifier. |
| `title` | Main label shown in the result row. |
| `category` | Workspace, Database, or Application. |
| `keywords` | Additional search terms. |
| `shortcut` | Optional display label for the primary shortcut. |
| `invocation` | Whether selection executes immediately or opens a form. |
| `command` | Typed `ChitinCommand` payload. |

`form_prompt` and `form_placeholder` are still present in the descriptor, but
the current RCSB form owns its labels and placeholder directly. They are not
part of the active rendering path.

### `CommandInvocationKind`

`CommandInvocationKind` separates command discovery from execution behavior:

- `Immediate` closes the panel and sends the typed command to the application
  command bus.
- `Form` keeps the modal open and changes the controller to
  `CommandPanelMode::Form(command)`.

This distinction lets search results remain data-only. The search list does not
need to know how a selected command is implemented.

### `CommandRegistry`

`CommandRegistry::new` collects descriptors from the feature modules:

```text
workspace::command_descriptors()
tab::command_descriptors()
database::command_descriptors()
application::command_descriptors()
                 │
                 ▼
          CommandRegistry
```

`CommandRegistry::search` normalizes the query, scores every matching
descriptor, and sorts by descending score and then title. Exact IDs rank above
ID prefixes, title matches, keywords, and category matches.

### `CommandPanelController`

`ChitinApp` owns one `CommandPanelController`. The controller is the persistent
state machine for the overlay, not its renderer.

| State | Purpose |
| --- | --- |
| `is_open` | Controls whether `ChitinApp::render` inserts the overlay. |
| `mode` | Selects search or command-specific form behavior. |
| `quickpick` | Composes the generic `QuickPickState` for query, selection, and result-list scrolling. |
| `search_input` | Lazily created persistent `Entity<TextInputState>`. |
| `previous_focus` | Restores the focus target when the modal closes. |
| `rcsb_form` | Lazily created singleton `RcsbFormPanel`. |
| subscription flags | Ensure GPUI entity subscriptions are installed once. |
| `rcsb_focus_pending` | Transfers focus to the form after entering Form mode. |

The principal state transitions are:

```text
Closed
  │ toggle/open
  ▼
Search ── select Form command ──► Form(command)
  │                                  │
  │ select Immediate command         │ Escape
  │ or Escape                         │
  └──────────────► Closed ◄───────────┘
```

`open` captures the current focus and focuses the persistent search input.
`close` clears transient search state, returns to Search mode, and restores the
captured focus. `toggle_without_focus` exists for command dispatch paths that do
not receive a GPUI `Window`; interactive UI should prefer the window-aware
`toggle` method.

## Root render integration

`ChitinApp::render` performs three command-panel operations on every render:

1. `command_panel_search_input` obtains the persistent text input and installs
   its semantic event subscription once.
2. `command_panel_rcsb_form` creates and subscribes the singleton RCSB form when
   Form mode is active. It also handles the pending focus request.
3. If `CommandPanelController::is_open` is true, `render_command_panel` inserts
   the modal overlay above the workbench.

The workbench root also captures key-down events and forwards them through
`ChitinApp::handle_command_panel_key`. The controller translates a raw key into
a semantic `CommandPanelEvent`; `ChitinApp` then performs application-level
effects such as closing the modal or invoking a command.

This division is intentional:

- The controller decides **what transition is requested**.
- `ChitinApp` decides **how that transition affects GPUI and application
  state**.
- Rendering functions decide **what the current state looks like**.

### `QuickPickState`

`QuickPickState` is the reusable behavior boundary in
`chitin-ui::composite::quickpick`. It owns only interaction state that applies
to any searchable, selectable list:

- the query string;
- the highlighted row index;
- the virtual-list scroll handle; and
- bounded previous/next navigation plus reveal requests.

The caller supplies the current item count and remains responsible for filtering,
ranking, row presentation, and activation semantics. This keeps quick-pick
independent from `ChitinCommand`, command descriptors, forms, and desktop event
types.

## Search-mode call flow

### Opening

```text
ToggleCommandPanel action
  → CommandPanelController::toggle(window, cx)
  → CommandPanelController::open(window, cx)
  → reset query, mode, and selection
  → remember previous focus
  → focus TextInputState
  → cx.notify()
  → ChitinApp::render
  → render_command_panel
```

### Typing and rendering results

```text
TextInputEvent::Change
  → CommandPanelController::set_query
  → QuickPickState::set_query
  → selection = 0 and reveal selected row
  → cx.notify()
  → CommandRegistry::search(query)
  → Vec<CommandSearchResult>
  → Vec<QuickPickItem>
  → render_quick_pick_overlay
```

The reusable quick-pick receives display data and an `on_select(index, ...)`
callback. It never receives `ChitinCommand` directly. `render_command_panel`
keeps a parallel vector of typed commands and converts the selected row index
back into a command.

### Keyboard navigation

Up and Down are handled by `CommandPanelController::handle_search_key`. They
delegate bounded navigation and reveal requests to `QuickPickState`, using the
appropriate GPUI scroll strategy. Text editing and Enter submission remain owned by the
`TextInputState` primitive and arrive as `TextInputEvent`s.

### Invoking a result

Pointer selection and Enter eventually call
`ChitinApp::invoke_command_from_panel`:

```text
selected ChitinCommand
  → registry.descriptor_for(command)
  → descriptor.invocation
     ├── Immediate
     │    → controller.close
     │    → ChitinApp::dispatch_command
     │    → feature-specific dispatcher
     └── Form
          → controller.open_form(command)
          → next render displays the form
```

The `ToggleCommandPanel` command is a special immediate command: the invocation
path closes the panel and does not dispatch the toggle again.

## RCSB form architecture

### Form composition

`RcsbFormPanel` is a cloneable handle around four persistent primitive entities:

```text
RcsbFormPanel
├── pdb_id: Entity<TextInputState>
├── format: Entity<SelectInputState>
├── submit: Entity<ButtonState>
└── download: Entity<RcsbDownloadState>
```

`RcsbFormPanel::render` composes those states into a `TextInput`, `Select`,
`Progress`, and primary `Button`. The form owns its presentation; the controller
only stores the singleton and tracks when it needs subscriptions or focus.

When Form mode is first rendered, `command_panel_rcsb_form`:

1. Lazily creates the form.
2. Installs subscriptions for text submission, format selection, and button
   clicks once.
3. Resets the singleton form for the new opening.
4. Focuses the PDB ID input.

`render_command_panel` detects Form mode before building search results and
renders the RCSB form directly. There is no generic quick-pick form rendering
path.

### Submission and validation

Both Enter in the PDB ID input and a button click call
`ChitinApp::submit_rcsb_form`:

```text
form primitives
  → submit_rcsb_form
  → require an open workspace
  → PdbId::new(trimmed input)
  → RcsbFormPanel::selected_format
  → RcsbDownloadRequest
  → RcsbDownloadState::start
  → disable submit button
```

Invalid workspace or PDB ID state stops submission before spawning background
work. The form remains open.

### Background download and progress

The actual transfer is performed outside the GPUI foreground update:

```text
GPUI foreground                         background executor
────────────────                        ───────────────────
submit_rcsb_form
  ├── spawn download task ────────────► download_rcsb_structure
  │                                      ├── Tokio runtime
  │                                      ├── Client::rcsb
  │                                      ├── download with callback
  │                                      └── persist artifact
  │
  ├── spawn 50 ms progress poller
  │      ◄──── atomic byte/percent state ── progress callback
  │
  └── update RcsbDownloadState
         ├── set_progress / set_received_bytes
         ├── finish
         └── fail
```

The transport callback may run without GPUI access, so it writes received bytes,
percentage basis points, total-size availability, and active state into atomics.
A GPUI task polls those atomics every 50 ms and updates the `Entity` on the
foreground executor.

If the response has a total length, the form shows determinate percentage
progress. Otherwise it shows an indeterminate animation and the number of
received kilobytes. Success transitions into the finishing animation; failure
keeps the form open and displays the error. The submit button is re-enabled in
both cases.

Downloaded files are persisted under the active workspace:

```text
.chitin/download/pdb/<PDB_ID>.pdb
.chitin/download/mmcif/<PDB_ID>.cif
```

## Focus, modality, and subscriptions

The command panel is a modal overlay inside the existing application window; it
does not create a system window.

- Opening Search captures the previous focus and focuses the search input.
- Entering the RCSB form schedules one focus transfer to its PDB ID input.
- Closing restores the focus captured when Search opened.
- The full-screen overlay occludes underlying pointer interaction.
- Quick-pick and popup components stop background scroll propagation where
  appropriate.

GPUI subscriptions are detached after installation because the application and
primitive entities own the relevant lifetime. Boolean flags in the controller
prevent the same subscription from being installed during every render.

## Adding a command

For an immediate command:

1. Add a variant to the appropriate feature command enum.
2. Add its stable ID mapping.
3. Add a `CommandDescriptor` in that feature's `command_descriptors` function.
4. Use `CommandInvocationKind::Immediate`.
5. Handle the variant in the feature's `ChitinApp` dispatcher.
6. Add a keybinding only if the command needs a direct shortcut.
7. Add registry-search and dispatcher tests where behavior is not already
   covered.

For a form command, the current architecture requires additional explicit
wiring:

1. Define a form component under `components/command_panel/form/`.
2. Store its persistent state in or behind `CommandPanelController`.
3. Add one-time semantic subscriptions and focus transfer.
4. Route Form mode to the correct renderer.
5. Keep validation and workflow orchestration in `chitin-desktop`; reusable
   visual and interaction behavior belongs in `chitin-ui` primitives.
6. Move provider/domain operations into the corresponding non-GPUI crate.

Do not add provider-specific behavior to `QuickPickOverlay`. Quick-pick is only
the reusable Search-mode presentation.

## Current constraints

The following limitations describe the implementation as it exists now:

- Form routing is specialized for RCSB rather than backed by a general form
  registry.
- The controller still contains a generic `SubmitForm` event and legacy
  form-key path, but the RCSB form submits through primitive semantic events and
  does not use that path.
- A download is background work relative to GPUI rendering, but there is not yet
  a global task center or toast notification system.
- Download completion does not yet open a rendered molecular document tab.
- The form is a singleton, while document tabs are expected to be multi-instance.

These constraints are useful boundaries when extending the command panel: new
work should not accidentally treat an RCSB-specific branch as a generic form or
task framework.
