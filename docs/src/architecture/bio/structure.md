# Structure model contract

This page defines the scientific contract shared by parsers, analysis, and
renderers. It describes what the model means, not how a particular crate stores
or uploads it.

## Topology and coordinate states

The logical hierarchy is:

```text
structure → model → chain → residue → atom
```

Topology contains atom identity, membership, and bonds. Coordinate state (k)
contains positions and other values that may vary between models:

$$
  \mathrm{state}_k =
  \{\mathbf{x}_{k,a}\mid a\text{ is active in state }k\}.
$$

Changing coordinate state must not change atom identifiers or silently rewrite
source topology.

## Units and coordinate systems

Cartesian coordinates use ångströms and a right-handed frame. Fractional
coordinates use the unit-cell basis:

$$
  \mathbf{x}=A\mathbf{f}.
$$

For edge lengths (a,b,c) and angles (\alpha,\beta,\gamma), a conventional
triclinic basis is:

$$
A =
\begin{bmatrix}
a & b\cos\gamma & c\cos\beta \\
0 & b\sin\gamma &
c\dfrac{\cos\alpha-\cos\beta\cos\gamma}{\sin\gamma} \\
0 & 0 & c\sqrt{1-u^2-v^2}
\end{bmatrix},
$$

with

$$
u=\cos\beta,
\qquad
v=\frac{\cos\alpha-\cos\beta\cos\gamma}{\sin\gamma}.
$$

Valid cell parameters satisfy:

$$
a,b,c>0,
\qquad
0^\circ<\alpha,\beta,\gamma<180^\circ.
$$

The basis must be non-singular. Its volume is:

$$
V=|\det(A)|>0.
$$

Zero or straight angles are therefore invalid, as are cells whose numerical
volume is too close to zero for reliable conversion.

## Identifiers and provenance

Author and label identifiers are separate namespaces. Alternate locations, model
numbers, source serials, and optional experimental values are preserved when
present.

Every bond records whether it came from source connectivity or geometric
inference. Derived geometry must remain traceable to the atom identities that
produced it.

## Failure boundary

The parser rejects malformed values and invalid references with a source field
or record location. A renderer may reject unsupported visual data, but it must
not be responsible for discovering broken scientific invariants.
