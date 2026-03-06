# RFC 0014: UI 문서 자동화 (Rust bin + Action 시나리오 + GitHub Actions)

- Status: Draft
- Author: sandbox-quant
- Date: 2026-02-18
- Related:
  - `docs/rfcs/0008-bottom-keybind-bar-simplification.md`
  - `docs/rfcs/0010-portfolio-grid-chart-layering-and-shortcut-separation.md`
  - `docs/rfcs/0011-portfolio-grid-three-tab-separation.md`
  - `docs/rfcs/0013-grid-network-tab-and-latency-observability.md`

## 1. 문제 정의

UI 변경마다 아래 작업을 수동으로 반복하고 있다.

1. `cargo run`으로 앱 실행
2. 키 입력으로 목표 화면 이동
3. 터미널 스크린샷 촬영
4. README 이미지/설명 갱신

이 과정은 시간이 오래 걸리고, 누락/불일치가 자주 발생한다.

## 2. 목표

- 수동 스크린샷/README 갱신 작업을 기본적으로 제거
- UI 상태 이동(키 입력)을 재현 가능한 시나리오로 표준화
- PR 단계에서는 빠르게(smoke), main/release에서는 완전하게(full) 문서 갱신
- 결과물을 `docs/ui` 아래에 일관되게 관리

## 3. 비목표

- 실제 거래소 실시간 데이터 기반 캡처 재현은 범위 밖
- 픽셀 단위 시각 회귀 테스트(완전 visual diff)는 1차 범위 밖
- README 전체 재작성은 범위 밖(마커 구간 치환만 수행)

## 4. 제안 요약

`ui_docs` Rust bin을 도입해, Action 시나리오를 실행하고 스냅샷/문서를 자동 생성한다.

1. `cargo run --bin ui_docs -- smoke|full|scenario <id>`
2. `docs/ui/scenarios/*.toml`에 화면 이동 Action과 스냅샷 시점을 선언
3. `UI_TEST_MODE=1` 고정 데이터로 렌더 재현성 확보
4. PNG 생성 + `docs/ui/INDEX.md` 생성 + README 마커 구간 자동 갱신
5. GitHub Actions에서 변경 범위/라벨에 따라 실행 모드를 분기

## 5. 상세 설계

### 5.1 Rust bin 구성

- 파일: `src/bin/ui_docs.rs`
- 주요 서브커맨드:
  - `smoke`: 핵심 화면만 빠르게 생성
  - `full`: 전체 화면 생성
  - `scenario <id>`: 단일 시나리오만 실행
  - `readme-only`: README 마커 구간만 재생성

입력:
- 시나리오 정의: `docs/ui/scenarios/*.toml`
- 필요 시 fixture: `tests/fixtures/ui/*.json`

출력:
- 이미지: `docs/ui/screenshots/*.png`
- 인덱스: `docs/ui/INDEX.md`
- 캐시: `docs/ui/.ui-docs-cache.json`

### 5.2 Action 시나리오 DSL

예시:

```toml
id = "grid-network"
title = "Portfolio Grid - Network"
size = { w = 220, h = 62 }
mode = "test"

[[step]]
type = "key"
value = "g"

[[step]]
type = "key"
value = "4"

[[step]]
type = "wait"
ms = 120

[[step]]
type = "assert_text"
value = "Network Metrics"

[[step]]
type = "snapshot"
path = "docs/ui/screenshots/grid-network.png"
```

지원 Action(초기):
1. `key`: 단일 키 입력 (`g`, `1`, `2`, `3`, `4`, `tab`, `enter`, `esc` 등)
2. `wait`: 렌더 안정화 대기
3. `assert_text`: 버퍼 내 텍스트 존재 확인
4. `snapshot`: 현재 프레임을 PNG 저장

### 5.3 화면 수 추정 및 운영 모드

예상 대상 화면(초기):
1. 기본 대시보드
2. Symbol Selector
3. Strategy Selector
4. Account Popup
5. History Popup
6. Grid Assets
7. Grid Strategies (ON 선택)
8. Grid Strategies (OFF 선택)
9. Grid Risk
10. Grid Network
11. Strategy Config Popup
12. Disconnected 상태
13. Reconnecting/WARN 상태
14. 체결 마커가 있는 차트
15. 멀티 전략 ON 요약

총 약 12~20개 범위로 운영 가능하며, 아래처럼 분리한다.

- `smoke`: 8개 내외(핵심 화면)
- `full`: 15개 이상(운영 상태 포함)

### 5.4 README 갱신 전략

README 전체를 건드리지 않고 마커 구간만 치환한다.

- `<!-- UI_DOCS:START -->`
- `<!-- UI_DOCS:END -->`

마커 내부에는 대표 이미지(3~4장) + `docs/ui/INDEX.md` 링크를 자동 삽입한다.

### 5.5 GitHub Actions 연동

워크플로우: `.github/workflows/ui-docs.yml`

트리거:
1. `pull_request` (UI 관련 파일 변경 시)
2. `push` to `main`
3. `workflow_dispatch`

실행 정책:
1. PR 기본: `smoke`
2. PR에 `ui-docs-full` 라벨: `full`
3. main push: `full`

산출물 정책:
1. PR: 이미지 artifact 업로드
2. main: `docs/ui/INDEX.md`/README 갱신 검증
3. 선택: bot commit 또는 CI fail-on-diff

## 6. 구현 단계

Phase 1. 기반 도입
1. `ui_docs` bin 골격 + 시나리오 파서
2. `key/wait/assert_text/snapshot` 실행기
3. smoke 시나리오 5~8개 구축

Phase 2. 문서 자동화
1. `docs/ui/INDEX.md` 생성기
2. README 마커 치환기
3. `.ui-docs-cache.json` 기반 변경분만 갱신

Phase 3. CI 통합
1. `ui-docs.yml` 추가
2. PR smoke + main full 분기
3. 실패 메시지 표준화(누락 시나리오/마커 미존재/assert 실패)

## 7. 수용 기준

1. 로컬에서 `cargo run --bin ui_docs -- smoke` 한 번으로 대표 UI 문서가 생성된다.
2. `scenario <id>`로 특정 화면만 재생성 가능하다.
3. README 마커 구간이 자동 갱신되며 다른 섹션은 변경하지 않는다.
4. PR에서는 기본 smoke가 동작하고, full은 라벨/메인 브랜치에서만 실행된다.
5. UI가 바뀌었는데 문서가 갱신되지 않으면 CI가 탐지한다.

## 8. 리스크 및 완화

- 리스크: 시나리오가 UI 키맵 변경에 취약
  - 완화: 공통 키 상수화 + `assert_text`로 조기 실패
- 리스크: 캡처 해상도/폰트 차이로 결과 흔들림
  - 완화: 고정 터미널 크기/폰트/환경변수 사용
- 리스크: full 실행 시간 증가
  - 완화: PR 기본 smoke, full은 제한 실행

## 9. 오픈 이슈

1. PNG 렌더러 구현 방식 선택:
   - ANSI buffer -> 이미지 변환
   - 테스트 백엔드 직접 매핑
2. main에서 자동 커밋 허용 여부
3. 대표 이미지 개수(README 노출 밀도) 최종 확정
