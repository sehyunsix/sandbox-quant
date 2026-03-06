# Automated Versioning

`main` 브랜치에 push 되면 GitHub Actions가 자동으로 버전을 올리고 릴리스를 생성한다.

Workflow:
- `.github/workflows/auto-version-release.yml`

Rules:
- 기본: `patch` 증가 (`x.y.z -> x.y.(z+1)`)
- 커밋 메시지에 `#minor` 포함: `minor` 증가 (`x.(y+1).0`)
- 커밋 메시지에 `#major` 또는 `BREAKING CHANGE` 포함: `major` 증가 (`(x+1).0.0`)

`1.0.0`를 만들 때는 최종 merge commit 또는 merge body에 `#major`를 포함시키는 것을 기본 규칙으로 본다.

Outputs:
- `Cargo.toml` + `Cargo.lock` 버전 업데이트 커밋
- `CHANGELOG.md` 자동 갱신 (최신 릴리스 항목이 상단에 prepend)
- git tag 생성 (`vX.Y.Z`)
- GitHub Release 생성
- crates.io publish

## Current Release Readiness

Current runtime release assumptions:

- default runtime mode: `demo`
- release crate metadata aligned with the exchange-truth runtime
- `cargo publish --dry-run` verified locally
- GitHub Actions `publish-crate` job verified from recent successful runs

## Recommended Final Merge Message for 1.0.0

```text
release: sandbox-quant 1.0.0 #major
```

or include:

```text
BREAKING CHANGE: reset to exchange-truth trading core architecture
```

## Final Pre-Merge Checks

1. Ensure release-targeted changes are merged into `main`.
2. Ensure `CARGO_REGISTRY_TOKEN` is configured in GitHub Actions secrets.
3. Run:

```bash
cargo check
cargo test -q --test bootstrap_tests --test app_runtime_tests --test core_types_tests --test cli_command_tests --test cli_output_tests
```

4. Confirm the final merge contains `#major` or `BREAKING CHANGE`.
