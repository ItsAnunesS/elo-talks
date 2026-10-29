#!/usr/bin/env python3
"""Idempotent publication; a delayed older build never replaces latest."""
import json, os, pathlib, subprocess

def gh(*args):
    return subprocess.check_output(["gh", *args], text=True)
repo, sha, number = os.environ["GITHUB_REPOSITORY"], os.environ["GITHUB_SHA"], int(os.environ["GITHUB_RUN_NUMBER"])
tag = f"v0.1.{number}"
existing = subprocess.run(["gh", "api", f"repos/{repo}/releases/tags/{tag}"], capture_output=True, text=True)
if existing.returncode == 0:
    release = json.loads(existing.stdout)
    if release["target_commitish"] != sha: raise SystemExit("Existing tag has a different source commit")
    if not release["draft"]:
        print("Already published; immutable assets retained.")
        raise SystemExit(0)
else:
    notes = pathlib.Path("release-notes.md")
    notes.write_text(f"Elo Talks {tag}\n\nCommit: `{sha}`\n\nAppImage Linux x86_64. Implementação inicial; consulte docs/VALIDATION.md para os limites de validação.\n\nVerifique SHA256SUMS e sua assinatura Sigstore antes de executar.\n")
    gh("release", "create", tag, "--repo", repo, "--target", sha, "--title", f"Elo Talks {tag}", "--notes-file", str(notes), "--draft")
for path in sorted(pathlib.Path("dist").iterdir()):
    if path.is_file(): gh("release", "upload", tag, str(path), "--repo", repo, "--clobber")
# Publish without touching latest, then reconcile against all published semantic versions.
gh("release", "edit", tag, "--repo", repo, "--draft=false", "--latest=false")
def highest():
    pages = json.loads(gh("api", "--paginate", "--slurp", f"repos/{repo}/releases?per_page=100"))
    versions = [int(r["tag_name"][5:]) for page in pages for r in page if r["tag_name"].startswith("v0.1.") and r["tag_name"][5:].isdigit() and not r["draft"] and not r["prerelease"]]
    return f"v0.1.{max(versions)}"
for _ in range(20):
    candidate = highest()
    gh("release", "edit", candidate, "--repo", repo, "--latest")
    actual = json.loads(gh("api", f"repos/{repo}/releases/latest"))["tag_name"]
    if actual == highest(): break
else:
    raise SystemExit("Could not reconcile latest; existing releases and assets were preserved")
