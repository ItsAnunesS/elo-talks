# GLib 0.18.5 security backport

This is the crates.io `glib` 0.18.5 source (MIT license retained) with the exact two-line fix from https://github.com/gtk-rs/gtk-rs-core/pull/1343 applied to `src/variant_iter.rs`.

Reason: Tauri 2 on GTK3 depends on glib 0.18; the upstream fixed release (0.20) is not ABI/API-compatible with that dependency graph. RUSTSEC-2024-0429 is an immutable out-pointer incorrectly passed to a variadic C function. The backport changes `p` to mutable and passes `&mut p`.

`tests/test_security_patch.py` verifies the pinned patched source hash and exercises the iterators through the application Rust tests. No other advisory is suppressed. Remove this patch when the Tauri dependency graph supports an upstream fixed GLib release.
