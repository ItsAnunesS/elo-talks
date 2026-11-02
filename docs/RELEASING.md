# Release authentication

## Validate before publication

The `Checks` workflow runs on pull requests, pushes to `codex/**`, and manual
dispatch. It is also the reusable implementation called by `AppImage release`.
All Rust, native clipboard/launcher/layout/notification, browser, Python and
security checks run in both paths, including AppImage packaging. The check-only
path has read-only permissions, uses the source manifest version and never
creates a tag, release or version commit.

Push a focused change to a `codex/**` branch, inspect the named failing step and
its diagnostics, and wait for a green Checks run before fast-forwarding the same
source commit onto `release`. Native fixture failures attach their synthetic
logs with stage/scale annotations. Keep source revisions and signed artifacts
unchanged when retrying a failed publication.

PR commit checks inspect `base.sha..head.sha`, not the temporary merge commit
GitHub creates for testing. Invalid contributor commit messages still fail.
The release path retains the full-history Conventional Commit check.

The release workflow's existing run counter remains the sole version input;
running Checks does not consume a release version. Only the release caller asks
the reusable workflow to set that version and export artifacts for publication.
Signature creation/verification and privileged publication remain in
`release.yml`, retaining the updater's expected Sigstore identity.

The build uses the read-only `GITHUB_TOKEN`. Only the publication step receives
`RELEASE_TOKEN`, when configured; otherwise it uses `GITHUB_TOKEN` with
`contents: write`. Sigstore continues to use GitHub OIDC, independently of this token.

GitHub can reject release creation or updates with HTTP 403 when the source commit
has workflow changes relative to the default branch. This can happen when another
push changes the workflow while an older build is running. An ancestry check does
not remove this permission requirement, and `GITHUB_TOKEN` cannot receive the
Workflows write permission.

## Repository setup

1. Create a fine-grained personal access token owned by the repository owner.
2. Select only `ItsAnunesS/elo-talks` under repository access.
3. Grant repository **Contents: Read and write** and **Workflows: Read and write**.
   Metadata read access is implicit. No account-wide permissions are needed.
4. Set an expiration and store the token as the repository Actions secret
   `RELEASE_TOKEN` under Settings → Secrets and variables → Actions.
5. Rotate the secret before expiration. Never commit or paste its value into logs,
   issues, chat, or release notes.

This credential can modify repository content and workflows. It is exposed only
to the publication script after the build succeeds, never to dependency installs,
build scripts, or pull request jobs.

## Failed publication recovery

A workflow run uses the workflow and scripts from its original commit. Rerunning
an older run that predates `RELEASE_TOKEN` support will not pick up this change.
A new push uses the updated workflow and its own version. Do not retarget an old
AppImage to a newer SHA or overwrite a published version to recover a failure.

For runs containing token support, configure or renew the secret, then rerun the
failed publication job while its build artifact is still available. The run number,
source SHA, and version stay unchanged. Existing published releases are immutable;
drafts can resume uploading. Publication failures stop without switching to a
newer commit or promoting an incomplete draft as latest.

References: [GitHub release API permissions](https://docs.github.com/en/rest/releases/releases)
and [workflow scope enforcement](https://github.blog/changelog/2023-11-02-github-actions-enforcing-workflow-scope-when-creating-a-release/).

## Automatic stable releases

Version selection comes from `release.json`, shared by the version setter and
publisher through `scripts/release_version.py`. The patch number is the configured
base patch plus `github.run_number - first_run_number`. In the current 0.5 series,
run 52 produces `0.5.0` and run 62 produces `0.5.10`. Reruns preserve the version. Builds before the configured
first run are rejected. When starting a new series, update both fields together.

Every successful push to `release` publishes directly to the stable channel; no RC step is used.
Tags/assets remain immutable. Latest reconciliation compares all stable
major/minor/patch components so older builds cannot replace newer releases.
See [RELEASE-0.5.md](RELEASE-0.5.md) for the current scope and validation limits.

CI now runs the pinned Playwright browser workflow suite before packaging. On Ubuntu
runners, a temporary profile grants user namespaces only to the test browser binary;
it is removed afterwards and is not packaged. The production WebKit sandbox stays on.
