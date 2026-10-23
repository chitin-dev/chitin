"""Generate external coordinates only; the Rust benchmark owns rendering and QA."""

from __future__ import annotations

import hashlib
import json
import platform
import sys
import time
from pathlib import Path
from typing import Any


def main() -> None:
    """Read one bounded-job request and emit coordinates or a classified failure.

    Source-order atoms and bonds are captured before sanitization. Explicit H
    vertices are retained. Sanitization is not allowed to silently alter the
    graph transferred to the shared renderer; Rust checks the original facts.
    """
    stage = "external_import"
    timings: dict[str, float] = {}
    try:
        started = time.perf_counter()
        from rdkit import Chem, rdBase
        from rdkit.Chem import rdCoordGen

        timings[stage] = (time.perf_counter() - started) * 1000
        params = rdCoordGen.CoordGenParams()
        precisions = {
            name: getattr(params, f"sketcher{name.title()}Precision")
            for name in ("coarse", "quick", "standard", "best")
        }
        module_path = Path(rdCoordGen.__file__)
        libraries = module_path.parents[2] / "rdkit.libs"
        library_hashes = {
            path.name: hashlib.sha256(path.read_bytes()).hexdigest()
            for path in sorted(libraries.glob("*"))
            if "coordgen" in path.name.lower()
        }
        metadata = {
            "kind": "rdkit_coordgen",
            "rdkit_version": rdBase.rdkitVersion,
            "rdkit_build": rdBase.rdkitBuild,
            "python_version": platform.python_version(),
            "coordgen_module_sha256": hashlib.sha256(module_path.read_bytes()).hexdigest(),
            "coordgen_version": None,
            "coordinate_seed": None,
            "coordgen_library_sha256": library_hashes,
            "coordgen_scaling": params.coordgenScaling,
            "precision_values": precisions,
        }
        if sys.argv[1:] == ["--probe"]:
            print(json.dumps(metadata, allow_nan=False))
            return
        request: dict[str, Any] = json.load(sys.stdin)
        stage = "external_parse"
        started = time.perf_counter()
        options = Chem.SmilesParserParams()
        options.removeHs = False
        options.sanitize = False
        options.parseName = False
        molecule = Chem.MolFromSmiles(request["case"]["smiles"], options)
        if molecule is None or molecule.GetNumAtoms() == 0:
            raise ValueError("source SMILES could not be parsed")
        if molecule.GetNumAtoms() > request["max_atoms"]:
            raise ValueError("source exceeds atom budget")
        atoms = []
        fixed_hydrogens = [atom.GetNoImplicit() for atom in molecule.GetAtoms()]
        for atom in molecule.GetAtoms():
            atom.SetIntProp("_source_ordinal", atom.GetIdx())
            atoms.append(
                {
                    "element": atom.GetAtomicNum(),
                    "isotope": atom.GetIsotope(),
                    "charge": atom.GetFormalCharge(),
                    "aromatic": atom.GetIsAromatic(),
                    "hydrogens": atom.GetNumExplicitHs(),
                }
            )
        bonds = [
            {
                "ends": [bond.GetBeginAtomIdx(), bond.GetEndAtomIdx()],
                "order": "aromatic" if bond.GetIsAromatic() else str(int(bond.GetBondTypeAsDouble())),
            }
            for bond in molecule.GetBonds()
        ]
        Chem.SanitizeMol(molecule)
        for atom, original, fixed_h in zip(molecule.GetAtoms(), atoms, fixed_hydrogens, strict=True):
            if (
                atom.GetAtomicNum() != original["element"]
                or atom.GetIsotope() != original["isotope"]
                or atom.GetFormalCharge() != original["charge"]
                or (fixed_h and atom.GetNumExplicitHs() != original["hydrogens"])
            ):
                raise ValueError("sanitization changed source atomic facts")
        for bond, original in zip(molecule.GetBonds(), bonds, strict=True):
            if [bond.GetBeginAtomIdx(), bond.GetEndAtomIdx()] != original["ends"]:
                raise ValueError("sanitization changed source connectivity")
            order = "aromatic" if bond.GetIsAromatic() else str(int(bond.GetBondTypeAsDouble()))
            if order != original["order"] and not (
                bond.GetIsAromatic() and original["order"] in {"1", "2", "aromatic"}
            ):
                raise ValueError("sanitization changed a nonaromatic source bond")
        Chem.AssignStereochemistry(molecule, cleanIt=True, force=True)
        timings[stage] = (time.perf_counter() - started) * 1000
        stage = "external_layout"
        started = time.perf_counter()
        precision = request["precision"]
        params.minimizerPrecision = precisions[precision]
        rdCoordGen.AddCoords(molecule, params)
        timings[stage] = (time.perf_counter() - started) * 1000
        conformer = molecule.GetConformer()
        positions: list[list[float] | None] = [None] * len(atoms)
        for atom in molecule.GetAtoms():
            point = conformer.GetAtomPosition(atom.GetIdx())
            positions[atom.GetIntProp("_source_ordinal")] = [point.x, point.y]
        print(
            json.dumps(
                {
                    "stage": stage,
                    "error": None,
                    "timings_ms": timings,
                    "layout": {
                        "atoms": atoms,
                        "bonds": bonds,
                        "positions": positions,
                        "metadata": metadata,
                        "precision": precision,
                    },
                },
                allow_nan=False,
            )
        )
    except Exception as error:
        if stage not in timings:
            timings[stage] = (time.perf_counter() - started) * 1000
        print(json.dumps({"stage": stage, "error": f"{type(error).__name__}: {error}", "timings_ms": timings}))


if __name__ == "__main__":
    main()
