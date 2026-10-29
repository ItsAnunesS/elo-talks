import json
import os
import pathlib
import subprocess
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
FAKE = (ROOT / "tests/fixtures/github_cli.py").read_text()


class PublicationTests(unittest.TestCase):
    def run_release(
        self,
        releases,
        number,
        comparison="identical",
        lookup_denied=False,
        creation_denied=False,
    ):
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
                "comparison": comparison,
                "lookup_denied": lookup_denied,
                "creation_denied": creation_denied,
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

    def test_removed_commit_is_not_published(self):
        result, state = self.run_release([], 1, comparison="diverged")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("no longer in release history", result.stdout)
        self.assertFalse(any(call[0] == "release" for call in state["calls"]))

    def test_permission_failure_is_not_treated_as_missing_release(self):
        result, state = self.run_release([], 1, lookup_denied=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("refusing to create", result.stderr)
        self.assertFalse(any(call[0] == "release" for call in state["calls"]))

    def test_ancestor_commit_is_still_published(self):
        result, state = self.run_release([], 1, comparison="ahead")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(state["latest"], "v0.1.1")

    def test_creation_permission_failure_preserves_source_and_assets(self):
        result, state = self.run_release(
            [], 3, comparison="ahead", creation_denied=True
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("RELEASE_TOKEN", result.stderr)
        self.assertIn("Workflows: write", result.stderr)
        mutations = [call for call in state["calls"] if call[0] == "release"]
        self.assertEqual(len(mutations), 1)
        self.assertEqual(mutations[0][:3], ["release", "create", "v0.1.3"])
        self.assertEqual(mutations[0][mutations[0].index("--target") + 1], "abc123")
        self.assertEqual(state["releases"], [])
