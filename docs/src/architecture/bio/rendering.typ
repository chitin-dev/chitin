#import "/book.typ": book-page
#show: book-page

#let source-root = "https://github.com/chitin-dev/chitin/blob/main/crates/chitin-bio/src"

= 1 Structure rendering boundary
<structure-rendering-boundary>

Rendering consumes a validated structure or a renderer-neutral scene. It does
not modify source records or decide whether a parsed value was scientifically
valid.

The rendering path has four conceptual stages:

1. select a model and coordinate state from the structure snapshot;
2. select source and derived connectivity for the requested view;
3. extract renderer-neutral instances, traces, bounds, and surface artifacts;
4. apply representation style and upload presentation data to the GPU.

The renderer-neutral scene types are defined in
#link(source-root + "/structure/scene.rs")[`structure/scene.rs`]. The desktop
and browser renderers can consume the same scene without taking ownership of
parsing or scientific validation.

== 1.1 Representations
<representations>

Molecule views are composed from independent representation layers:

- #strong[atom representation] displays atoms and bonds, including stick and
  ball-and-stick styles;
- #strong[polymer representation] displays backbone traces, helices, sheets,
  and other polymer-level geometry;
- #strong[surface representation] displays a molecular surface derived from
  the selected atoms and coordinates.

These layers are non-exclusive. A surface may coexist with a cartoon and a
side-chain stick layer. Turning off one layer must not mutate the source
structure or force unrelated layers to disappear.

The molecule renderer consumes representation-neutral data from
#link("https://github.com/chitin-dev/chitin/tree/main/crates/chitin-molecule-renderer")[`chitin-molecule-renderer`].
The biological layer remains responsible for identities, coordinates, and
derived scientific artifacts; the renderer is responsible for geometry layout,
materials, lighting, and GPU resources.

== 1.2 Visual depth and continuity
<visual-depth-and-continuity>

Atom and bond surfaces should meet continuously at junctions. Heteronuclear
bonds use one continuous geometric connection whose color transitions between
the two endpoint elements; they should not appear as two separated pieces.

Depth perception comes from several independent cues:

- surface normals and diffuse lighting;
- soft specular highlights;
- depth-buffer occlusion;
- a restrained distance cue for distant geometry;
- distinct atom and bond radii.

The distance cue is a presentation aid, not a replacement for depth testing. If
$z$ is the linear view-space depth, a smooth cue can be written as:

$ c(z) = s f(z; z_0, z_1) $

Here $s$ is the cue strength and $f$ is a smooth interpolation function. The
values $z_0$ and $z_1$ delimit the transition range.
The cue must be applied after geometry has passed the depth test so it does not
hide occlusion errors.

== 1.3 Surface and extent handling
<surface-and-extent>

Surface geometry can extend beyond the center-to-center bounds of atoms,
cartoons, or sticks. Camera fitting and depth-cue ranges must therefore include
the supplied surface extent whenever the surface layer is enabled. A compact
ligand or a single atom is a useful regression case because the surface radius
can be much larger than the stick radius.

Surface construction belongs to `chitin-bio` and produces domain artifacts;
GPU tessellation and shading belong to the molecule renderer. This boundary
allows the surface algorithm to be tested independently from camera fitting and
graphics APIs.

== 1.4 Style versus data
<style-versus-data>

Element colors, radii, lighting, background, and representation are visual
style. Coordinates, topology, metadata, and bond provenance are scientific
data. Changing style must not alter the structure snapshot. Changing the
coordinate state should invalidate only the derived visual data that depends on
positions.
