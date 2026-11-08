#!/usr/bin/env python3
"""Prepare the checksum-pinned Linux x86_64 CEF distribution used by Cargo and CI."""
import hashlib
import json
import os
import pathlib
import shutil
import tarfile
import tempfile
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parents[1]
NAME = "cef_binary_156.0.26+g8ea0c73+chromium-156.0.8078.88_linux64_minimal.tar.bz2"
SHA256 = "600ac0b8f37cb953f884e7c89998a6c200d24959aa74820ad431b61d5ebcf360"
SIZE = 333351579
METADATA = {
    "last_modified": "2026-11-07T04:05:55.774000Z",
    "name": NAME,
    "sha1": "cacfbb7d7261fea682fb67c108de7e4347519ff7",
    "size": SIZE,
    "type": "minimal",
}


def valid_archive(path):
    if not path.is_file() or path.stat().st_size != SIZE:
        return False
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest() == SHA256


def prepare():
    cache = ROOT / "target"
    cache.mkdir(exist_ok=True)
    destination = cache / "cef-runtime"
    marker = destination / "archive.json"
    if marker.is_file() and json.loads(marker.read_text()) == METADATA:
        for required in ["libcef.so", "include/cef_version.h", "resources.pak"]:
            if not (destination / required).is_file():
                raise RuntimeError(
                    "Incomplete CEF cache; remove target/cef-runtime and retry"
                )
        return destination
    if destination.exists():
        raise RuntimeError(
            "Unrecognized CEF cache; remove target/cef-runtime and retry"
        )
    archive = cache / "cef-minimal.tar.bz2"
    if not valid_archive(archive):
        with tempfile.NamedTemporaryFile(dir=cache, delete=False) as download:
            pending = pathlib.Path(download.name)
            try:
                url = "https://cef-builds.spotifycdn.com/" + NAME.replace("+", "%2B")
                with urllib.request.urlopen(url, timeout=120) as response:
                    shutil.copyfileobj(response, download, 1024 * 1024)
                download.flush()
                if not valid_archive(pending):
                    raise RuntimeError(
                        "CEF archive failed its pinned size/SHA-256 check"
                    )
                os.replace(pending, archive)
            finally:
                pending.unlink(missing_ok=True)
    with tempfile.TemporaryDirectory(dir=cache) as temporary:
        stage = pathlib.Path(temporary)
        with tarfile.open(archive) as source:
            source.extractall(stage, filter="data")
        extracted = stage / NAME.removesuffix(".tar.bz2")
        for directory in ["Release", "Resources"]:
            for entry in (extracted / directory).iterdir():
                shutil.move(entry, extracted / entry.name)
            (extracted / directory).rmdir()
        (extracted / "archive.json").write_text(json.dumps(METADATA) + "\n")
        os.replace(extracted, destination)
    return destination


if __name__ == "__main__":
    print(prepare())
