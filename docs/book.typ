#import "@preview/shiroa:0.4.0": *

#show: book

#book-meta(
  title: "Chitin developer documentation",
  authors: ("ashgrey",),
  language: "en",
  summary: [
    = CLI
    #prefix-chapter("src/cli/completions.typ")[Shell completion]
    #prefix-chapter("src/cli/database.typ")[Database commands]
    #prefix-chapter("src/cli/bio.typ")[Bio commands]

    = `chitin-bio` architecture
    #prefix-chapter("src/architecture/bio/overview.typ")[Overview]
    #prefix-chapter("src/architecture/bio/data-model.typ")[Structure data model]
    #prefix-chapter("src/architecture/bio/parsing.typ")[PDB and PDBx/mmCIF parsing]
    #prefix-chapter("src/architecture/bio/bond-inference.typ")[Geometric bond inference]
    #prefix-chapter("src/architecture/bio/rendering.typ")[Structure rendering boundary]
    #prefix-chapter("src/architecture/bio/mmcif-schema.typ")[PDBx/mmCIF dictionary model]
    #prefix-chapter("src/architecture/bio/structure.typ")[Structure model contract]

    = `chitin-desktop` architecture
    #prefix-chapter("src/architecture/desktop/command-panel.typ")[Command panel]
  ],
)

// Re-export the shared page template for individual chapters.
#import "/templates/page.typ": project
#let book-page = project
