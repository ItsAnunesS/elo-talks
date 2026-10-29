import json
import os
import pathlib
import subprocess
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
FAKE = (ROOT / "tests/fixtures/github_cli.py").read_text()


class PublicationTests(unittest.TestCase):
    def run_release(self, releases, number):
        with tempfile.TemporaryDirectory() as d:
            p = pathlib.Path(d)
            (p / "dist").mkdir()
            (p / "dist/test.AppImage").write_bytes(b"fixture")
            (p / "gh").write_text(FAKE)
            (p / "gh").chmod(0o755)
            state = {
                "releases": releases,
                "latest": releases[0]["tag_name"] if releases else None,
                "calls": [],
            }
            (p / "state.json").write_text(json.dumps(state))
            env = {
                **os.environ,
                "PATH": d + os.pathsep + os.environ["PATH"],
                "FAKE_STATE": str(p / "state.json"),
                "GITHUB_REPOSITORY": "owner/repo",
                "GITHUB_SHA": "abc123",
                "GITHUB_RUN_NUMBER": str(number),
            }
            result = subprocess.run(
                ["python3", str(ROOT / "scripts/release.py")],
                cwd=p,
                env=env,
                capture_output=True,
                text=True,
            )
            return result, json.loads((p / "state.json").read_text())

    def test_first_release_is_published(self):
        r, s = self.run_release([], 1)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(s["latest"], "v0.1.1")
        self.assertFalse(s["releases"][0]["draft"])

    def test_old_build_does_not_replace_newer_latest(self):
        release = {
            "tag_name": "v0.1.43",
            "target_commitish": "newer",
            "draft": False,
            "prerelease": False,
        }
        r, s = self.run_release([release], 42)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(s["latest"], "v0.1.43")

    def test_published_release_is_not_overwritten(self):
        release = {
            "tag_name": "v0.1.42",
            "target_commitish": "abc123",
            "draft": False,
            "prerelease": False,
        }
        r, s = self.run_release([release], 42)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertFalse(any(a[:2] == ["release", "upload"] for a in s["calls"]))
