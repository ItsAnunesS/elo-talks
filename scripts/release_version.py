"""One version policy shared by packaging and publication."""

import json
import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parents[1]
STABLE = re.compile(r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)")


def build_version(number):
    if not re.fullmatch(r"[1-9][0-9]*", str(number)):
        raise ValueError("Expected a positive GitHub run number")
    config = json.loads((ROOT / "release.json").read_text())
    version = config["version"]
    if not STABLE.fullmatch(version):
        raise ValueError("Invalid release version")
    if config["channel"] == "candidate":
        return f"{version}-rc.{number}"
    if config["channel"] == "stable":
        return version
    raise ValueError("Invalid release channel")


def stable_key(tag):
    value = tag.removeprefix("v")
    return tuple(map(int, value.split("."))) if STABLE.fullmatch(value) else None
