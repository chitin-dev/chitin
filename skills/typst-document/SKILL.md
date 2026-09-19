---
name: typst-document
description: "Edit and organize Chitin's Typst documentation with valid Typst syntax, source-linked explanations, mathematical notation, and package-backed diagrams. Use when changing docs/*.typ or the Shiroa documentation template."
---

# Chitin Typst Documentation

Use this skill for the documentation under `docs/`. The output should be
precise, detailed, and easy to maintain without repeating the same architecture
explanation in several chapters.

## Project conventions

- Write prose in clear technical English unless the surrounding document uses a
  different language.
- Keep one concept in one chapter and link to it from other chapters instead of
  copying its full explanation.
- Link important implementation claims to the corresponding source file or
  public API in `crates/`.
- Preserve the distinction between scientific data, derived data, rendering,
  and UI concerns.
- Use normal Typst markup and symbols. Do not write escaped pseudo-Typst such as
  `upright(...)`, `\alpha`, or `\space` when a native Typst form is available.
- In prose, represent literal string values with quoted strings, for example
  `"MODEL"`, rather than turning them into mathematical expressions.

## Mathematics

Use equations only for compact symbolic relationships, dimensions, invariants,
or physical constraints. Do not use equations to describe relationships between
software objects; use prose, lists, tables, or diagrams for those.

Prefer short symbol-first equations, for example:

```typst
$ bold(x)_(k,a) = (x_(k,a), y_(k,a), z_(k,a)) in bb(R)^3 $
```

Do not put long explanatory text inside math. Explain symbols in the paragraph
following the equation. Avoid `upright(...)` and escaped LaTeX commands.

## Diagrams

Never use an ASCII diagram in a code block. Use Fletcher when the relationship
is a graph or pipeline:

```typst
#import "@preview/fletcher:0.5.8": diagram, edge, node

#diagram(
  node((0, 0), [Input]),
  node((2, 0), [Validated model]),
  edge((0, 0), (2, 0), "-|>"),
)
```

Use a table for exact mappings and a numbered list for a linear process. If
Fletcher cannot express the needed layout cleanly, search Typst Universe for a
maintained diagram package before inventing a custom SVG solution. Remember
that CeTZ/Fletcher layout may be ignored by Typst's experimental static HTML
export; verify the selected Shiroa mode when diagrams matter.

## Shiroa verification

The documentation uses Shiroa 0.4.0 and selects its theme in
`docs/config.typ`. Theme-specific CSS belongs in separate values in
`docs/templates/page.typ`; do not use Starlight selectors for mdBook pages.

Run the repository recipe after edits so local verification uses the same
GitHub Pages path prefix as deployment:

```bash
just docs-build
```

The recipe currently expands to
`shiroa build --mode dyn-paged --path-to-root /chitin/ docs`. Do not omit
`--path-to-root /chitin/` when invoking Shiroa directly, because root-relative
chapter and asset URLs must remain under the repository's Pages path.

For static HTML validation, use:

```bash
shiroa build --mode static-html --path-to-root /chitin/ docs
```

Also run `git diff --check`. Treat Typst syntax errors as failures. Treat the
HTML-export warning as expected, but investigate any warning that says content
or layout was ignored when the affected chapter depends on a diagram.
