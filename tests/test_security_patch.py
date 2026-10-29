import hashlib
import pathlib
import unittest


class PatchTests(unittest.TestCase):
    def test_glib_backport_matches_reviewed_source(self):
        root = pathlib.Path(__file__).resolve().parents[1]
        self.assertEqual(
            hashlib.sha256(
                (root / "vendor/glib/src/variant_iter.rs").read_bytes()
            ).hexdigest(),
            "a0f5ee8acb8faa089bcdfbc9a57372609fce7654026ccef7d9a224d05a654ccc",
        )
        self.assertIn(
            'glib = { path = "vendor/glib" }', (root / "Cargo.toml").read_text()
        )
