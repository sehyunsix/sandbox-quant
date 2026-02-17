# sandbox-quant Docs

브라우저에서 프로젝트 문서를 읽기 위한 문서 포털입니다.

- Release 계획
- 아키텍처 RFC
- UI 전환 RFC

## 로컬 실행

`mdbook` 설치:

```bash
cargo install mdbook
```

문서 서버 실행:

```bash
mdbook serve docs-site --open
```

## API 문서(Rustdoc)

코드 레벨 문서는 아래로 브라우저 오픈:

```bash
cargo doc --no-deps --open
```
