# Agent Operating Rules (Gemini CLI)

## 목적
Gemini CLI 에이전트는 이 저장소에서 다음 작업을 주기적으로 수행한다.
- 이슈 생성
- 기능(Feature) 제안
- 에러 상세화(Error triage and concretization)

## 작업 주기
- 기본 주기: 작업 세션마다 최소 1회
- 권장 주기: 코드 변경 단위(기능/버그 단위)마다 1회

## 브랜치 정책 (필수)
- 모든 작업 시작 전에 반드시 새 브랜치를 생성한다.
- 기본 브랜치(main/master)에서 직접 작업하거나 직접 커밋하지 않는다.
- 브랜치 네이밍 예시:
  - `feature/<short-topic>`
  - `fix/<short-topic>`
  - `chore/<short-topic>`

## 표준 실행 순서
1. 기본 브랜치 최신화
2. 작업 브랜치 생성
3. 코드/문서 분석
4. 아래 명령 카테고리 수행
5. 변경 검증 후 커밋/푸시
6. PR 또는 이슈 링크 정리

## 명령 카테고리
- 이슈 생성: 재현 단계, 기대 결과, 실제 결과, 영향 범위 포함
- 기능 제안: 문제 정의, 대안, 예상 효과, 리스크 포함
- 에러 구체화: 로그/스택트레이스/환경/재현성/우선순위 포함

## 권한 정책
- `~/project/sandbox-quant` 하위 파일 읽기/탐색 권한을 사용 가능 상태로 유지한다.
- GitHub 관련 권한(이슈 생성/조회, PR 생성/조회, 원격 푸시)을 사용 가능 상태로 유지한다.

## 권장 커맨드 예시
```bash
git checkout -b feature/<topic>
rg --files
rg "TODO|FIXME|error|panic"
gh issue create --title "..." --body "..."
gh pr create --title "..." --body "..."
```

## 산출물 최소 기준
- 이슈: 제목 + 재현 단계 + 영향도 + 우선순위
- 기능 제안: 배경 + 목표 + 설계 요약 + 수용 기준
- 에러 구체화: 원인 가설 + 검증 계획 + 임시/영구 대응

## 테스트 작성 규칙 (필수)
- 작성하는 테스트 코드는 반드시 `tests/` 디렉토리에 둔다.
- `src/` 내부에 테스트 코드를 새로 추가하지 않는다.
- feature를 추가하면 반드시 해당 feature를 검증하는 테스트 코드를 함께 작성한다.

## 단축 실행 명령
- `c`: commit
- `cp`: commit + push
- `cpp`: commit + push + PR
- `cppm`: commit + push + PR + merge
