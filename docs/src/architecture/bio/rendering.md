# Structure rendering boundary

Rendering consumes a validated structure or a renderer-neutral scene. It does
not modify source records or decide whether a parsed value was scientifically
valid.

```text
structure snapshot
       ↓
selected model and derived connectivity
       ↓
renderer-neutral scene
       ↓
representation and visual style
       ↓
GPU presentation
```

## Representations

The molecule view supports three complementary representations:

- **stick**: bonds emphasize connectivity;
- **ball-and-stick**: atom centers and bonds are shown together;
- **sphere**: atom volumes are emphasized and bonds are omitted.

The scene keeps atom and bond identities so selection and diagnostics can map
visual objects back to the source structure.

## Visual depth and continuity

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
(z) is the linear view-space depth, a smooth cue can be expressed as:

$$
  c(z)=s\,\operatorname{smoothstep}(z_0,z_1,z),
$$

where (s) is the cue strength and (z_0,z_1) delimit the transition range.

## Style versus data

Element colors, radii, lighting, background, and representation are visual
style. Coordinates, topology, metadata, and bond provenance are scientific data.
Changing style must not alter the structure snapshot; changing the coordinate
state should invalidate only the derived visual data that depends on positions.
