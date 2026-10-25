"""Optional real-provider protocol tests, run inside the locked environment."""

from __future__ import annotations

import json
import subprocess
import sys
import unittest
from pathlib import Path
from typing import Any


class CoordinateExchangeTests(unittest.TestCase):
    """Validate the installed provider without replacing the shared Rust QA."""

    def run_worker(self, smiles: str) -> dict[str, Any]:
        """Run one source string using the same installed interpreter."""
        output = subprocess.run(
            [sys.executable, "-I", str(Path(__file__).with_name("worker.py"))],
            input=json.dumps({"case": {"smiles": smiles}, "max_atoms": 128, "precision": "standard"}),
            capture_output=True,
            text=True,
            timeout=15,
            check=True,
        )
        return json.loads(output.stdout)

    def test_explicit_hydrogen_and_isotopes_retain_source_vertices(self) -> None:
        """Independent H vertices cannot disappear through parser defaults."""
        result = self.run_worker("[2H]O[13CH3]")
        self.assertIsNone(result["error"])
        self.assertEqual([atom["isotope"] for atom in result["layout"]["atoms"]], [2, 0, 13])
        self.assertEqual(len(result["layout"]["positions"]), 3)
        self.assertEqual(result["layout"]["precision"], "standard")

    def test_aromatic_perception_can_represent_an_inferred_hydrogen(self) -> None:
        """Perceived aromatic N-H must not be mistaken for changing a source H declaration."""
        result = self.run_worker("C1=CC=CN1")
        self.assertIsNone(result["error"])
        self.assertTrue(all(not atom["aromatic"] for atom in result["layout"]["atoms"]))
        self.assertEqual(result["layout"]["atoms"][-1]["hydrogens"], 0)

    def test_charge_changing_cleanup_is_not_silently_accepted(self) -> None:
        """Chemical normalization is outside the coordinate-only comparison."""
        result = self.run_worker("CN(=O)=O")
        self.assertEqual(result["stage"], "external_parse")
        self.assertIn("changed source atomic facts", result["error"])
        self.assertNotIn("layout", result)

    def test_invalid_source_is_a_failure_without_fallback_coordinates(self) -> None:
        """A rejected input remains a failed benchmark case."""
        result = self.run_worker("C(")
        self.assertEqual(result["stage"], "external_parse")
        self.assertIsNotNone(result["error"])
        self.assertNotIn("layout", result)


if __name__ == "__main__":
    unittest.main()
