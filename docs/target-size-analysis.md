# Target Size Analysis

Date: 2026-02-13

## Summary

`target/` size was measured at about `2.8G`. The largest contributors were:

- `target/debug/deps`: about `2.0G`
- `target/debug/incremental`: about `441M`
- `target/doc`: about `276M`

This pattern indicates repeated local debug/test builds plus generated Rust docs.

## Root Cause

- Rust dependency artifacts include large debug symbols by default.
- Incremental compilation caches accumulate across build graph changes.
- `cargo doc` output for the full dependency graph is large.

## Mitigation Added In This PR

- `Cargo.toml` now keeps debug info for this crate but disables heavy debug symbols for dependencies in `dev` and `test` profiles.
- This reduces long-term growth of `target/debug` while preserving useful local debugging for project code.

## Operational Commands

Check current size:

```bash
du -sh target
du -h -d 2 target | sort -hr | head -n 20
```

One-time cleanup:

```bash
cargo clean
```

Doc-only cleanup:

```bash
rm -rf target/doc
```
