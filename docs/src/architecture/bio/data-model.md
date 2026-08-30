# Structure data model

The structure model is a validated snapshot of molecular identity, topology,
coordinates, and crystallographic metadata. It is independent of GPUI and wgpu.

## Main concepts

```text
Structure
├── models and coordinate states
├── chains
├── residues
├── atoms
├── bonds
└── metadata
    ├── unit cell
    ├── symmetry
    └── biological assembly descriptions
```

Atoms belong to residues, residues belong to chains, and chains are observed in
one or more models. Bonds connect atoms and carry both a bond order (when known)
and a provenance such as source connectivity or geometric inference.

## Identity versus state

Topology is shared across models whenever the source permits it. For atom $a$
and coordinate state $k$, the position is:

$$
  \mathbf{x}_{k,a} = (x_{k,a}, y_{k,a}, z_{k,a}) \in \mathbb{R}^{3}
$$

Changing $k$ changes $\mathbf{x}_{k,a}$, not the atom's identifier, element,
residue membership, chain membership, or source bonds.

## References and invariants

References between tables use stable typed identities. A successful parse must
ensure that every reference points to an existing record:

$$
  \begin{aligned}
    \mathrm{residue}(a) &\in [0, N_{\mathrm{residue}})\\
    \mathrm{chain}(r) &\in [0, N_{\mathrm{chain}})\\
    \mathrm{endpoint}(b) &\in [0, N_{\mathrm{atom}})
  \end{aligned}
$$

Each active coordinate state has one position for every atom it addresses.
Invalid references are parse failures, not deferred renderer errors.

## Crystallographic metadata

Unit-cell edge lengths $(a,b,c)$ are measured in ångströms. Angles
$(\alpha,\beta,\gamma)$ are measured in degrees and satisfy:

$$
  a,b,c > 0, \space 0^\circ < \alpha,\beta,\gamma < 180^\circ
$$

These values describe the source crystal. Reading them does not automatically
expand symmetry mates or duplicate atoms.

## Derived data and provenance

Source bonds and inferred bonds remain distinguishable. A renderer may draw
both, while analysis tools can restrict themselves to source connectivity.
Derived geometry retains the identities of the atoms it came from so that a
selection, diagnostic, or pick result can be mapped back to the domain model.
