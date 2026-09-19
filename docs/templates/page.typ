// This is important for shiroa to produce a responsive layout
// and multiple targets.
#import "@preview/shiroa:0.4.0": (
  get-page-width, is-html-target, is-pdf-target, is-web-target, plain-text,
  shiroa-sys-target, templates,
)
#import templates: *
#import "/config.typ": web-theme

// Metadata
#let page-width = get-page-width()
#let is-html-target = is-html-target()
#let is-pdf-target = is-pdf-target()
#let is-web-target = is-web-target()
#let sys-is-html-target = ("html" in dictionary(std))

// Theme (Colors)
#let themes = theme-box-styles-from(toml("theme-style.toml"), read: it => read(
  it,
))
#let (
  default-theme: (
    style: theme-style,
    is-dark: is-dark-theme,
    is-light: is-light-theme,
    main-color: main-color,
    dash-color: dash-color,
    code-extra-colors: code-extra-colors,
  ),
) = themes; #let (
  default-theme: default-theme,
) = themes;
#let theme-box = theme-box.with(themes: themes)

// Fonts
#let main-font = (
  // Prefer the fonts bundled by Shiroa so CI does not depend on host fonts.
  "Libertinus Serif",
)
#let code-font = (
  // Prefer the fonts bundled by Shiroa so code blocks render consistently.
  "Cascadia Mono",
)

// Sizes
#let main-size = if is-web-target {
  16pt
} else {
  10.5pt
}
#let heading-sizes = if is-web-target {
  (2, 1.5, 1.17, 1, 0.83).map(it => it * main-size)
} else {
  (26pt, 22pt, 14pt, 12pt, main-size)
}
#let list-indent = 0.5em

// Keep the theme-specific CSS separate because Starlight and mdBook expose
// different HTML class names for their navigation and content containers.
#let starlight-extra-css = ```css
  .site-title {
    font-size: 1.2rem;
    font-weight: 600;
    font-style: bold;
  }

  .toc .outline-item > a {
    /* Override Starlight's `padding-inline` shorthand as a complete value. */
    padding-inline: calc(0.2rem * var(--depth) + var(--pad-inline));
  }

  .sl-markdown-content > .code-image {
    margin-block: 0.5rem;
    pre {
      border-radius: 0.5rem;
    }
  }
```

#let mdbook-extra-css = ```css
  .content > main > .code-image {
    margin-block: 0.5rem;
  }

  .content > main > .code-image pre {
    border-radius: 0.5rem;
  }
```

// Select only the stylesheet belonging to the active web theme.
#let extra-css = if web-theme == "starlight" {
  starlight-extra-css
} else if web-theme == "mdbook" {
  mdbook-extra-css
} else {
  panic("Unknown web theme: " + web-theme)
}

/// The project show rule that is used by all pages.
///
/// Example:
/// ```typ
/// #show: project
/// ```
///
/// - title (str): The title of the page.
/// - description (auto): The description of the page.
///   - If description is `auto`, it will be generated from the plain body.
///   - If description is `none`, an error is raised to force migration. In future, `none` will mean the description is not generated.
///   - Hint: use `""` to generate an empty description.
/// - authors (array | str): The author(s) of the page.
/// - kind (str): The kind of the page.
/// - plain-body (content): The plain body of the page.
#let project(
  title: "Typst Book",
  description: auto,
  authors: (),
  kind: "page",
  plain-body,
) = {
  // set basic document metadata
  set document(
    author: authors,
    title: title,
  ) if not is-pdf-target

  // set web/pdf page properties
  set page(
    numbering: none,
    number-align: center,
    width: page-width,
  ) if not (sys-is-html-target or is-html-target)

  // remove margins for web target
  set page(
    margin: (
      // reserved beautiful top margin
      top: 20pt,
      // reserved for our heading style.
      // If you apply a different heading style, you may remove it.
      left: 20pt,
      // Typst is setting the page's bottom to the baseline of the last line of text. So bad :(.
      bottom: 0.5em,
      // remove rest margins.
      rest: 0pt,
    ),
    height: auto,
  ) if is-web-target and not is-html-target

  let common = (
    web-theme: web-theme,
  )

  let template-args = arguments(
    include "/book.typ",
    title: title,
    description: description,
    plain-body: plain-body,
    extra-assets: (extra-css,),
  )

  // Apply the theme selected in `docs/config.typ`.
  show: if web-theme == "starlight" {
    import "@preview/shiroa-starlight:0.4.0": starlight
    starlight.with(..template-args)
  } else if web-theme == "mdbook" {
    import "@preview/shiroa-mdbook:0.4.0": mdbook
    mdbook.with(..template-args)
  } else {
    panic("Unknown web theme: " + web-theme)
  }

  // Set main text
  set text(
    font: main-font,
    size: main-size,
    fill: main-color,
    lang: "en",
  )

  // markup setting
  show: markup-rules.with(
    ..common,
    themes: themes,
    heading-sizes: heading-sizes,
    list-indent: list-indent,
    main-size: main-size,
  )
  // math setting
  show: equation-rules.with(..common, theme-box: theme-box)
  // code block setting
  show: code-block-rules.with(..common, themes: themes, code-font: code-font)

  // Apply project-specific Typst styles after Shiroa's generic markup rules.
  // Those rules install their own heading and raw show rules, so placing these
  // overrides earlier in the template would allow them to be replaced.
  show heading: set text(weight: "extrabold")
  show raw.where(block: false): set text(fill: orange)

  // Main body.
  set par(justify: true)

  plain-body
}

#let part-style = heading
