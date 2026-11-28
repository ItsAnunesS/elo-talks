import importlib.util
import pathlib
import tempfile
import unittest
from unittest import mock

ROOT = pathlib.Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "nss_modules", ROOT / "scripts/nss_modules.py"
)
NSS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(NSS)


class NssModulesTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = pathlib.Path(self.temporary.name)
        self.system = self.root / "usr/lib/x86_64-linux-gnu"
        self.firefox = self.root / "usr/lib/firefox"
        self.system.mkdir(parents=True)
        self.firefox.mkdir(parents=True)
        for name in NSS.MODULES:
            (self.system / name).write_bytes(b"system fixture")
            (self.firefox / name).write_bytes(b"private firefox fixture")
        self.paths = [str(self.system / name) for name in NSS.MODULES]

    def test_uses_package_owned_modules_despite_private_firefox_copies(self):
        files = "\n".join([str(self.system), *reversed(self.paths)])
        self.assertEqual(NSS.resolve(files), self.paths)

    def test_missing_package_module_does_not_fall_back_to_firefox(self):
        (self.system / NSS.MODULES[0]).unlink()
        with self.assertRaisesRegex(RuntimeError, "libsoftokn3.so"):
            NSS.resolve("\n".join(self.paths))

    def test_rejects_missing_ambiguous_and_relative_package_entries(self):
        for files in [
            "\n".join(self.paths[1:]),
            "\n".join([*self.paths, self.paths[0]]),
            "\n".join([NSS.MODULES[0], *self.paths[1:]]),
        ]:
            with self.subTest(files=files), self.assertRaises(RuntimeError):
                NSS.resolve(files)

    def test_cli_queries_libnss3_and_outputs_all_modules(self):
        with mock.patch.object(
            NSS.subprocess, "check_output", return_value="\n".join(self.paths)
        ) as query, mock.patch("builtins.print") as output:
            NSS.main()
        query.assert_called_once_with(
            ["dpkg-query", "--listfiles", "libnss3"], text=True
        )
        output.assert_called_once_with("\n".join(self.paths))

    def test_cli_does_not_emit_partial_results_on_failure(self):
        with mock.patch.object(
            NSS.subprocess, "check_output", return_value="\n".join(self.paths[1:])
        ), mock.patch("builtins.print") as output:
            with self.assertRaisesRegex(SystemExit, "Cannot package system NSS"):
                NSS.main()
            output.assert_not_called()
