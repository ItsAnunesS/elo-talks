#!/usr/bin/env python3
"""Set the build version without making a version commit."""
import json, pathlib, re, sys
number = sys.argv[1]
if not re.fullmatch(r"[1-9][0-9]*", number):
    raise SystemExit("Expected a positive GitHub run number")
version = f"0.1.{number}"
root = pathlib.Path(__file__).resolve().parents[1]
p = root / "src-tauri/Cargo.toml"
p.write_text(re.sub(r'^version = "[^"]+"', f'version = "{version}"', p.read_text(), count=1, flags=re.M))
p = root / "src-tauri/tauri.conf.json"
config = json.loads(p.read_text()); config["version"] = version
p.write_text(json.dumps(config, indent=2) + "\n")
p = root / "Cargo.lock"
s = p.read_text()
s, count = re.subn(r'(\[\[package\]\]\nname = "elo-talks"\nversion = ")[^"]+("\n)', rf'\g<1>{version}\2', s)
if count != 1: raise SystemExit("App package not found in Cargo.lock")
p.write_text(s)
print(version)
