# Contributing

## Language and naming

Write application text, error messages, comments, documentation, tests, and identifiers in English. Preserve upstream licenses and third-party sources. Use descriptive names that explain the domain; avoid single-letter names outside short iterators. Use Rust and Python snake_case and JavaScript camelCase.

## Commit messages

Use Conventional Commits:

```text
<type>(optional-scope): imperative description
```

Allowed types: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`, and `revert`. Add `!` for a breaking change. Examples:

```text
fix(ui): keep account controls visible while resizing
feat(updates): publish AppImage update metadata
docs(security): explain vault recovery limitations
```

Keep commits focused. Use the repository owner's configured Git identity; do not add assistant coauthors. Rewriting published history requires explicit authorization. Do not rewrite signed release tags without addressing their provenance.

## Code quality

- Keep UI rendering, native IPC, and input behavior separate.
- Keep security decisions in native code; remote content must never gain vault permissions.
- Keep persisted formats, authentication contexts, and keyring namespaces stable unless implementing an explicit migration.
- Use small, named functions and constants instead of compressed statements and magic limits.
- Comments should explain constraints or intent rather than restating the code.
- Do not change vendored code during general formatting. Its security patch checksum is intentional.

## Checks

```sh
npm ci
npm run format:check
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
python3 -m unittest discover -s tests -v
python3 scripts/check-commit-messages.py
```

Prettier is a pinned development-only dependency; no npm packages are shipped in the frontend. Format Python with Black (24.2.0) and Rust with the pinned toolchain. Native rendering tests must use an isolated desktop rather than the user's GNOME session. See `docs/VALIDATION.md` for the difference between automated tests and platform certification.
