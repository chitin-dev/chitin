# Geometric bond inference

Structure files often omit ordinary covalent bonds. Explicit connectivity is
therefore combined with a conservative geometric inference step when a scene
needs complete visual connectivity.

Inferred bonds are derived from one selected coordinate state. They retain
their provenance and never overwrite source topology.

## Distance criterion

For atoms (i) and (j), with positions (\mathbf{x}_i) and
\(\mathbf{x}_j\), their separation is:

$$
d_{ij}=\lVert\mathbf{x}_i-\mathbf{x}_j\rVert_2.
$$

A candidate is accepted only when:

$$
d_{\min} < d_{ij} \leq
\min\bigl(t(e_i,e_j), d_{\max}\bigr),
$$

where (e_i,e_j) are the normalized elements, (d_{\min}) excludes
coincident coordinates, (d_{\max}) limits the search neighborhood, and
(t) is an element-pair distance threshold.

The threshold combines experimentally common element pairs with a fallback
based on their covalent radii:

$$
t(e_i,e_j) = \frac{r_i+r_j}{1.95}.
$$

Distance alone cannot reliably determine whether a connection is single,
double, triple, or aromatic. Inferred bond order therefore remains unknown
unless stronger source evidence is available.

## Compatibility rules

Two atoms in different explicit alternate conformations must not be connected:

$$
\operatorname{compatible}(i,j) =
(\operatorname{alt}_i=\varnothing)\lor
(\operatorname{alt}_j=\varnothing)\lor
(\operatorname{alt}_i=\operatorname{alt}_j).
$$

Hydrogen–hydrogen pairs are excluded. A pair already present in source
connectivity is not inferred again. These rules keep the result useful for
visualization without pretending to solve complete chemical perception.

## Locality

Naively testing every atom pair costs:

$$
\frac{N(N-1)}{2}=O(N^2).
$$

The search is localized by partitioning space into cells whose width is the
maximum search distance. An atom only needs its own cell and neighboring cells,
so practical work is proportional to the number of local candidate pairs rather
than all global pairs.

## Scope

Inference is a fallback for missing topology. It does not replace component
dictionaries, valence models, aromaticity perception, metal coordination rules,
or periodic crystal-image bonds. Those sources should take precedence whenever
they are available.
