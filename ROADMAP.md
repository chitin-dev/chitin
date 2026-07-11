# Chitin Roadmap

*Chitin* is a modern, agent-native computational chemistry and bioinformatics
integrated development suite. The roadmap is organized around building a useful
local desktop first, then expanding into reproducible scientific workflows and
agent-assisted automation.

## Product Principles

- Local-first project workspaces with reproducible inputs, outputs, and
  provenance.
- Agent-native workflows where agents can inspect context, propose actions, run
  tools, and explain results.
- Strong separation between domain logic, desktop UI, execution backends, and
  integrations.
- Scientific engines should be designed as independent crates before they are
  wired into the desktop, agents, workflows, or external-tool adapters.
- Scientific correctness over interface novelty.
- Extensible plugin-style architecture for chemistry, bioinformatics,
  visualization, and model providers.

## Phase 0: Foundation

Goal: Establish the engineering shape of the project.

Deliverables:

- [x] Cargo workspace with `chitin-core` and `chitin-desktop`.
- [x] GPUI desktop shell with a real application frame.
- [x] Formatting, linting, and CI-ready check commands.
- [x] Project identity: logo, README, roadmap, license, contribution guide.
- [ ] Initial architecture notes for workspace, task, file, and agent
      boundaries.

Acceptance checks:

- `cargo fmt --all --check`
- `cargo check --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`

## Phase 1: Desktop Workspace MVP

Goal: Make Chitin useful as a project-oriented scientific desktop.

Deliverables:

- [ ] Project workspace open/create flow.
- [ ] File explorer for common chemistry and bioinformatics files.
- [ ] Central editor/viewer area with tabs.
- [ ] Command palette for actions.
- [ ] Task panel for local jobs and agent actions.
- [ ] Settings model for paths, executables, and model providers.
- [ ] Persistent workspace metadata under a hidden project directory.

Initial file targets:

- [ ] Chemistry: `.sdf`, `.mol`, `.mol2`, `.pdb`, `.cif`, `.xyz`, `.smi`.
- [ ] Bioinformatics: `.fasta`, `.fastq`, `.gb`, `.gff`, `.vcf`, `.bam` metadata
      view.
- [ ] Tables and logs: `.csv`, `.tsv`, `.json`, `.toml`, `.log`.

Acceptance checks:

- A user can create a Chitin workspace and reopen it.
- A user can browse files and open at least text, table, molecule, and sequence
  views.
- Workspace state survives restart.

## Phase 2: Scientific Data Model

Goal: Define the core domain model independent of GPUI.

Deliverables:

- [ ] Workspace, document, dataset, molecule, sequence, job, and result types in
      `chitin-core`.
- [ ] Import pipeline with typed parse results and recoverable diagnostics.
- [ ] Provenance model for generated files and job outputs.
- [ ] Stable serialization for workspace metadata.
- [ ] Unit tests for parsers and domain transformations.

Important design points:

- [ ] Keep UI-specific state out of `chitin-core`.
- [ ] Use typed errors for library boundaries.
- [ ] Treat every generated artifact as traceable to inputs, parameters, tool
      version, and timestamp.

Acceptance checks:

- Core crate can be tested without launching GPUI.
- Invalid scientific files produce actionable diagnostics instead of panics.
- Repeated imports of the same file are deterministic.

## Phase 2.5: Independent Scientific Engines

Goal: Plan the native Rust engine crates that will eventually re-implement core
logic from docking, chemistry, bioinformatics, and molecule/protein viewing
software.

Planned crates:

- [ ] `chitin-molecule`: atoms, bonds, molecular graphs, small-molecule
      properties, conformers, format-neutral molecule models, and deterministic
      molecule transformations.
- [ ] `chitin-structure`: proteins, nucleic acids, residues, chains, ligands,
      structural annotations, selections, measurements, and structure analysis.
- [ ] `chitin-docking`: docking problem definitions, search spaces, pose models,
      scoring abstractions, grid maps, ligand/receptor preparation contracts,
      and eventually native docking algorithms.
- [ ] `chitin-viewer-core`: renderer-independent scene state, camera state,
      molecular selections, labels, measurements, color modes, and visual
      styles.
- [ ] `chitin-sequence`: biological sequence models, sequence annotations,
      alignments, feature intervals, and search result types.

Design constraints:

- These crates must not depend on GPUI.
- The desktop should consume these crates, not own their logic.
- External tools such as AutoDock Vina, Open Babel, RDKit, BLAST, minimap2, or
  visualization backends should be adapter layers, not the core model.
- Start with data models, parsers, diagnostics, and deterministic
  transformations before optimized algorithms.
- Use compatibility fixtures to compare Chitin behavior against established
  software while native implementations mature.

Acceptance checks:

- Each planned engine crate has a written responsibility boundary before it is
  created.
- Core types can be exercised from tests or CLI tools without launching the
  desktop.
- Tool adapters can be replaced without changing stable Chitin domain types.

## Phase 3: Molecule And Structure Viewer

Goal: Provide native visual inspection for molecular structures.

Deliverables:

- [ ] 2D molecule summary view for small molecules.
- [ ] 3D structure viewer for protein and ligand files.
- [ ] Selection model for atoms, residues, chains, ligands, and annotations.
- [ ] Basic measurement tools: distance, angle, residue contact list.
- [ ] Color modes: element, chain, residue type, hydrophobicity,
      confidence/metadata.
- [ ] Exportable snapshots.

Candidate dependencies:

- [ ] Rust-native parsers where practical.
- [ ] External backends only behind explicit adapter boundaries.

Acceptance checks:

- A user can open a PDB file and inspect chains, residues, and ligands.
- A user can select two atoms or residues and get a measurement.
- Large structures remain responsive enough for routine inspection.

## Phase 4: Local Tool Execution

Goal: Run scientific tools from Chitin with structured inputs and outputs.

Deliverables:

- [ ] Job runner abstraction in core.
- [ ] Desktop job queue with logs, status, cancellation, and output capture.
- [ ] Tool adapters for common local executables.
- [ ] Environment detection and setup diagnostics.
- [ ] Result viewers that link outputs back to source inputs.

Initial adapters:

- Open Babel or RDKit-based conversion workflow.
- AutoDock Vina docking workflow.
- BLAST or minimap2 sequence search workflow.
- Multiple sequence alignment workflow.

Acceptance checks:

- A user can configure a local executable path.
- A user can run a job, watch logs, cancel it, and inspect outputs.
- Failed jobs preserve enough context to debug parameters and environment
  issues.

## Phase 5: Agent Runtime

Goal: Add agent-assisted scientific work without hiding execution details.

Deliverables:

- [ ] Agent session model with messages, tool calls, files, jobs, and citations.
- [ ] Tool permission layer for read, write, execute, network, and external
      model calls.
- [ ] Context builder for selected files, active workspace, job history, and
      visible UI state.
- [ ] Agent task panel integrated with the desktop workspace.
- [ ] Human approval flow for risky actions.

Agent capabilities:

- [ ] Explain a molecule, sequence, or result file.
- [ ] Suggest next analysis steps.
- [ ] Prepare docking or sequence-search jobs.
- [ ] Summarize job outputs and flag quality concerns.
- [ ] Generate reproducible workflow notes.

Acceptance checks:

- Agent actions are auditable.
- The user can see exactly what files and commands an agent used.
- The agent cannot mutate workspace files without an explicit approved action
  path.

## Phase 6: Workflow Builder

Goal: Turn repeated scientific tasks into reproducible pipelines.

Deliverables:

- [ ] Visual or structured workflow editor.
- [ ] Nodes for import, conversion, docking, alignment, filtering, scoring, and
      reporting.
- [ ] Parameter presets and validation.
- [ ] Workflow run history.
- [ ] Exportable workflow definitions.

Acceptance checks:

- A user can build and rerun a small docking or sequence analysis workflow.
- Workflow outputs are linked to exact inputs and parameters.
- Workflows can be shared as project files.

## Phase 7: Reporting And Collaboration

Goal: Make results easy to review, export, and share.

Deliverables:

- Report composer for methods, inputs, figures, tables, and agent summaries.
- Export to Markdown, HTML, PDF, and project archive.
- Result comparison views.
- Annotation and note system.
- Optional Git-backed workspace history.

Acceptance checks:

- A user can produce a report from a completed workflow.
- Reports include provenance and tool versions.
- Project archives can be reopened on another machine.

## Phase 8: Extension System

Goal: Let Chitin grow beyond built-in workflows.

Deliverables:

- [ ] Stable adapter traits for file formats, tools, viewers, and agents.
- [ ] Plugin manifest format.
- [ ] Sandboxed plugin execution strategy.
- [ ] Plugin registry support for local development.
- [ ] Example plugins for one chemistry and one bioinformatics workflow.

Acceptance checks:

- A plugin can add a file parser and viewer.
- A plugin can add a job adapter.
- Plugin failures do not crash the desktop shell.

## Near-Term Engineering Tasks

- Add CI for format, check, clippy, and tests.
- Add `CONTRIBUTING.md`.
- Add `ARCHITECTURE.md` with crate boundaries.
- Add a `chitin-workspace` crate if workspace persistence becomes large enough
  to split from core.
- Decide on error crates for core and binaries.
- Decide on serialization format for workspace metadata.
- Add a small fixture set for molecule and sequence parser tests.

## Open Decisions

- Whether 3D rendering should be implemented directly in GPUI, embedded through
  a dedicated renderer, or delegated to an external visualization engine.
- Whether Python-backed chemistry integrations should be supported through
  subprocess adapters, a service boundary, or optional bindings.
- Which model providers should be supported first for agent workflows.
- How strict project reproducibility should be before cloud or remote execution
  is introduced.
- Whether plugin APIs should stabilize before or after the first full workflow
  builder.
