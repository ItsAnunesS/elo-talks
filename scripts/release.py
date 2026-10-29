#!/usr/bin/env python3
"""Publish immutable releases and keep the newest published version as latest."""
import json
import os
import pathlib
import subprocess

MAX_LATEST_RECONCILIATION_ATTEMPTS = 20
TAG_PREFIX = "v0.1."


def github_cli(*arguments):
    return subprocess.check_output(["gh", *arguments], text=True)


def highest_published_version(repository):
    pages = json.loads(
        github_cli(
            "api", "--paginate", "--slurp", f"repos/{repository}/releases?per_page=100"
        )
    )
    versions = [
        int(release["tag_name"][len(TAG_PREFIX) :])
        for page in pages
        for release in page
        if release["tag_name"].startswith(TAG_PREFIX)
        and release["tag_name"][len(TAG_PREFIX) :].isdigit()
        and not release["draft"]
        and not release["prerelease"]
    ]
    return f"{TAG_PREFIX}{max(versions)}"


def reconcile_latest(repository):
    for _ in range(MAX_LATEST_RECONCILIATION_ATTEMPTS):
        candidate = highest_published_version(repository)
        github_cli("release", "edit", candidate, "--repo", repository, "--latest")
        actual = json.loads(github_cli("api", f"repos/{repository}/releases/latest"))[
            "tag_name"
        ]
        if actual == highest_published_version(repository):
            return
    raise SystemExit(
        "Could not reconcile latest; existing releases and assets were preserved"
    )


def prepare_draft(repository, revision, tag):
    existing = subprocess.run(
        ["gh", "api", f"repos/{repository}/releases/tags/{tag}"],
        capture_output=True,
        text=True,
    )
    if existing.returncode == 0:
        release = json.loads(existing.stdout)
        if release["target_commitish"] != revision:
            raise SystemExit("Existing tag has a different source commit")
        if not release["draft"]:
            print("Already published; immutable assets retained.")
            return False
        return True

    notes = pathlib.Path("release-notes.md")
    notes.write_text(
        f"Elo Talks {tag}\n\nCommit: `{revision}`\n\n"
        "Linux x86_64 AppImage. Initial implementation; see docs/VALIDATION.md for validation limits.\n\n"
        "Verify SHA256SUMS and its Sigstore signature before running the application.\n"
    )
    github_cli(
        "release",
        "create",
        tag,
        "--repo",
        repository,
        "--target",
        revision,
        "--title",
        f"Elo Talks {tag}",
        "--notes-file",
        str(notes),
        "--draft",
    )
    return True


def main():
    repository = os.environ["GITHUB_REPOSITORY"]
    revision = os.environ["GITHUB_SHA"]
    tag = f"{TAG_PREFIX}{int(os.environ['GITHUB_RUN_NUMBER'])}"
    if not prepare_draft(repository, revision, tag):
        return
    for artifact in sorted(pathlib.Path("dist").iterdir()):
        if artifact.is_file():
            github_cli(
                "release",
                "upload",
                tag,
                str(artifact),
                "--repo",
                repository,
                "--clobber",
            )
    # An older parallel build must not become latest merely because it finished last.
    github_cli(
        "release", "edit", tag, "--repo", repository, "--draft=false", "--latest=false"
    )
    reconcile_latest(repository)


if __name__ == "__main__":
    main()
