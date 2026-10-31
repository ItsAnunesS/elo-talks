import pathlib
import unittest
import tempfile
import shutil
import subprocess
import json

ROOT = pathlib.Path(__file__).resolve().parents[1]


class VersionTests(unittest.TestCase):
    def test_updates_only_app_version(self):
        with tempfile.TemporaryDirectory() as d:
            root = pathlib.Path(d)
            for path in [
                "scripts/set-version.py",
                "scripts/release_version.py",
                "release.json",
                "Cargo.lock",
                "src-tauri/Cargo.toml",
                "src-tauri/tauri.conf.json",
            ]:
                target = root / path
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy(ROOT / path, target)
            (root / "release.json").write_text(
                json.dumps({"version": "0.2.0", "first_run_number": 32})
            )
            subprocess.run(
                ["python3", str(root / "scripts/set-version.py"), "42"],
                check=True,
                capture_output=True,
            )
            self.assertEqual(
                json.loads((root / "src-tauri/tauri.conf.json").read_text())["version"],
                "0.2.10",
            )
            self.assertIn(
                'name = "elo-talks"\nversion = "0.2.10"',
                (root / "Cargo.lock").read_text(),
            )
            for run_number, expected in [(32, "0.2.0"), (33, "0.2.1"), (33, "0.2.1")]:
                subprocess.run(
                    ["python3", str(root / "scripts/set-version.py"), str(run_number)],
                    check=True,
                    capture_output=True,
                )
                self.assertEqual(
                    json.loads((root / "src-tauri/tauri.conf.json").read_text())[
                        "version"
                    ],
                    expected,
                )
            before = (root / "Cargo.lock").read_text()
            result = subprocess.run(
                ["python3", str(root / "scripts/set-version.py"), "31"],
                capture_output=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(before, (root / "Cargo.lock").read_text())
            result = subprocess.run(
                ["python3", str(root / "scripts/set-version.py"), "../malicious"],
                capture_output=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(before, (root / "Cargo.lock").read_text())

    def test_remote_views_have_no_capabilities(self):
        cap = json.loads((ROOT / "src-tauri/capabilities/local-ui.json").read_text())
        self.assertEqual(cap["webviews"], ["main"])
        self.assertNotIn("remote", cap)
        self.assertNotIn("windows", cap)


if __name__ == "__main__":
    unittest.main()
