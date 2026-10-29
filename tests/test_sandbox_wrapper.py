import json
import os
import pathlib
import subprocess
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]


class SandboxWrapperTests(unittest.TestCase):
    def test_paths_relocated_without_removing_isolation(self):
        with tempfile.TemporaryDirectory(prefix="elo app ") as d:
            app = pathlib.Path(d)
            (app / "usr/bin").mkdir(parents=True)
            real = app / "usr/bin/bwrap-real"
            real.write_text(
                "#!/usr/bin/python3\nimport json,sys\nprint(json.dumps(sys.argv[1:]))\n"
            )
            real.chmod(0o755)
            read, write = os.pipe()
            try:
                os.write(
                    write,
                    b"--unshare-all\0--die-with-parent\0--ro-bind\0././/lib\0././/lib\0--seccomp\0"
                    + b"22\0",
                )
                os.close(write)
                run = subprocess.run(
                    [
                        str(ROOT / "scripts/bwrap.sh"),
                        "--args",
                        str(read),
                        "--",
                        "././/bin/xdg-dbus-proxy",
                        "--args=23",
                    ],
                    env={**os.environ, "APPDIR": d},
                    pass_fds=(read,),
                    capture_output=True,
                    text=True,
                )
                self.assertEqual(run.returncode, 0, run.stderr)
                args = json.loads(run.stdout)
                self.assertEqual(args[:2], ["--unshare-all", "--die-with-parent"])
                self.assertIn("--seccomp", args)
                self.assertIn("22", args)
                self.assertEqual(
                    args[-6:],
                    [
                        "--ro-bind",
                        d,
                        d,
                        "--",
                        d + "/usr/bin/xdg-dbus-proxy",
                        "--args=23",
                    ],
                )
                self.assertIn(d + "/usr/lib", args)
            finally:
                os.close(read)

    def test_missing_bundle_fails_closed(self):
        env = {**os.environ}
        env.pop("APPDIR", None)
        run = subprocess.run(
            [str(ROOT / "scripts/bwrap.sh"), "--", "/bin/true"],
            env=env,
            capture_output=True,
        )
        self.assertNotEqual(run.returncode, 0)
