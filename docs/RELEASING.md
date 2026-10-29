# Release authentication

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
