# Automated Versioning

`main` 브랜치에 push 되면 GitHub Actions가 자동으로 버전을 올리고 릴리스를 생성한다.

Workflow:
- `.github/workflows/auto-version-release.yml`

Rules:
- 기본: `patch` 증가 (`x.y.z -> x.y.(z+1)`)
- 커밋 메시지에 `#minor` 포함: `minor` 증가 (`x.(y+1).0`)
- 커밋 메시지에 `#major` 또는 `BREAKING CHANGE` 포함: `major` 증가 (`(x+1).0.0`)

Outputs:
- `Cargo.toml` + `Cargo.lock` 버전 업데이트 커밋
- `CHANGELOG.md` 자동 갱신 (최신 릴리스 항목이 상단에 prepend)
- git tag 생성 (`vX.Y.Z`)
- GitHub Release 생성
