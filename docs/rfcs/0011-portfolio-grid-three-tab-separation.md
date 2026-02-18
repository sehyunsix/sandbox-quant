# RFC 0011: Portfolio Grid 3-Tab 분리 (Assets / Strategies / Risk)

- Status: Draft
- Author: sandbox-quant
- Date: 2026-02-18
- Related:
  - `docs/rfcs/0003-ui-transition-to-multi-asset-multi-strategy.md`
  - `docs/rfcs/0008-bottom-keybind-bar-simplification.md`
  - `docs/rfcs/0009-portfolio-grid-risk-visibility.md`
  - `docs/rfcs/0010-portfolio-grid-chart-layering-and-shortcut-separation.md`

## 1. 문제 정의

현재 Portfolio Grid에 자산/전략/리스크 정보가 한 화면에 동시에 배치되어 다음 문제가 발생한다.

- 정보량 과다로 시선 분산
- 각 테이블 영역이 좁아져 행/컬럼 절단 빈번
- 상태 파악(예: Risk 병목)보다 레이아웃 해석에 시간이 더 걸림

## 2. 목표

- 정보 영역을 목적별로 분리해 가독성 개선
- 각 화면에서 필요한 밀도/컬럼 폭 확보
- 운영자가 “지금 보고 싶은 관점”을 빠르게 전환

## 3. 비목표

- 전략/리스크 계산 로직 자체 변경은 범위 밖
- 키맵 전체 재설계는 범위 밖 (최소 추가만)

## 4. 제안: 3-Tab 구조

Portfolio Grid를 다음 탭으로 분리:

1. `Assets` 탭
- 자산 테이블 중심
- 컬럼: `Symbol | Qty | Price | RlzPnL | UnrPnL`
- 합계: `Total Assets`, `Total Rlz/Unr PnL`

2. `Strategies` 탭
- ON/OFF 전략 테이블 중심
- 컬럼: `Symbol | Strategy | State | RunTime | W/L/T | PnL`
- 기능: `Tab panel`, `J/K`, `O`, `N`, `C`, `X`, `Enter`

3. `Risk` 탭
- Risk summary + 상세 지표 중심
- 그룹별 `used/limit`, 압력 비율, reset 시간
- 상태 배지: `OK / WARN / CRIT`

## 5. 탭 전환 UX

- `[` / `]` 또는 `1/2/3`으로 탭 전환
- 상단 탭 헤더 예시:
  - `[1 Assets] [2 Strategies] [3 Risk]`
- 현재 탭 강조(색상 + bold)

권고 키:
- `1`: Assets
- `2`: Strategies
- `3`: Risk

## 6. 레이아웃 원칙

- 한 탭에는 한 목적의 데이터만 크게 보여준다.
- 하단 안내 바도 탭 컨텍스트에 맞춰 변경한다.
  - Assets 탭: 조회 중심 키
  - Strategies 탭: 조작 중심 키
  - Risk 탭: 필터/상세 토글 중심 키

## 7. 구현 단계

1. 탭 상태 추가
- `v2_grid_tab: Assets | Strategies | Risk`

2. 탭 헤더 렌더 추가
- Grid 상단에 고정 헤더

3. 탭별 본문 렌더 분기
- `render_grid_assets_tab`
- `render_grid_strategies_tab`
- `render_grid_risk_tab`

4. 탭별 키바인드 바 분리
- 기존 단일 문구를 탭별 문구로 분기

5. 회귀 테스트
- 탭 전환 렌더 테스트
- 각 탭 핵심 문자열 존재 테스트
- 전략 조작 키가 Strategies 탭에서만 동작하는지 테스트

## 8. 수용 기준

- 120 cols 기준에서 각 탭 핵심 데이터가 잘리지 않고 읽힌다.
- Strategies 탭에서 현재 기능(토글/선택/삭제/실행)이 그대로 동작한다.
- Risk 탭에서 병목 그룹과 상태(`OK/WARN/CRIT`)를 즉시 식별 가능하다.
- 탭 전환 시 렌더 겹침/잔상 없이 부드럽게 변경된다.

## 9. 리스크 및 완화

- 리스크: 탭 전환이 추가되어 조작 단계가 늘어남
  - 완화: 숫자 단축키(1/2/3) 즉시 전환
- 리스크: 기존 사용자 혼란
  - 완화: 초기 릴리스에서 상단 도움말/로그로 전환 키 안내
