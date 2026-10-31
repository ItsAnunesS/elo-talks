#!/usr/bin/env python3
"""Fail publication if update metadata or the delta companion is missing/stale."""
import fnmatch
import hashlib
import re
import pathlib
import subprocess
import sys
import tempfile

EXPECTED = "gh-releases-zsync|ItsAnunesS|elo-talks|latest|*amd64.AppImage.zsync"


def verify(appimage):
    with tempfile.TemporaryDirectory() as directory:
        section = pathlib.Path(directory) / "update-info"
        subprocess.run(
            [
                "objcopy",
                "--dump-section",
                f".upd_info={section}",
                str(appimage),
                str(pathlib.Path(directory) / "copy"),
            ],
            check=True,
            capture_output=True,
        )
        candidate = re.search(
            r"([0-9]+\.[0-9]+\.[0-9]+-rc\.[1-9][0-9]*)", appimage.name
        )
        expected = (
            EXPECTED.replace("|latest|", f"|v{candidate[1]}|")
            if candidate
            else EXPECTED
        )
        if section.read_bytes().rstrip(b"\0").decode() != expected:
            raise ValueError(
                "AppImage update source does not match the release repository"
            )
    companion = appimage.with_name(appimage.name + ".zsync")
    if not fnmatch.fnmatch(companion.name, EXPECTED.rsplit("|", 1)[1]):
        raise ValueError("Asset name does not match embedded update pattern")
    header = companion.read_bytes().split(b"\n\n", 1)[0].decode()
    fields = dict(line.split(": ", 1) for line in header.splitlines() if ": " in line)
    if fields.get("Filename") != appimage.name:
        raise ValueError("zsync filename mismatch")
    if fields.get("URL") != appimage.name:
        raise ValueError("zsync URL must reference the adjacent release asset")
    if int(fields["Length"]) != appimage.stat().st_size:
        raise ValueError("zsync length mismatch")
    # SHA-1 is part of zsync's format; SHA256SUMS + Sigstore remain separate.
    with appimage.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha1").hexdigest()
    if fields["SHA-1"] != digest:
        raise ValueError("zsync digest does not match the finished AppImage")
    print(f"PASS: embedded GitHub update source and zsync match {appimage.name}")


if __name__ == "__main__":
    images = list(pathlib.Path(sys.argv[1]).glob("*.AppImage"))
    if len(images) != 1:
        raise SystemExit("Expected exactly one AppImage in output directory")
    verify(images[0])
