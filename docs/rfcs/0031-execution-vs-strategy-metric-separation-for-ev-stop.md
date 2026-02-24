# RFC 0031: EV/Stop 표시의 책임 분리 (Execution 1:1 vs Strategy 1:N)

- Status: Draft
- Author: sandbox-quant
- Date: 2026-02-24
- Related:
  - `docs/rfcs/0018-right-panel-semantics-split-position-vs-strategy-metrics.md`
  - `docs/rfcs/0026-position-lifecycle-exit-expectancy-and-session-fail-safe.md`
  - `docs/rfcs/0027-ev-integration-and-exit-orchestration-for-simple-signal-runtime.md`
  - `docs/rfcs/0030-ui-integration-for-ev-and-exit-price-visibility.md`
  - `src/ui/dashboard.rs`
  - `src/ui/mod.rs`

## 1. Problem

현재 Grid Strategies 테이블에 `EV`, `Score(p_win)`, `Gate`, `Stop`가 노출된다.

하지만 도메인 관점에서:

1. `EV/Stop/Gate`는 "진입 시점의 실행(Execution) 컨텍스트"에 강하게 결합된 값이다.
2. 실행 컨텍스트는 포지션/주문 단위(사실상 1:1)로 해석되어야 한다.
3. Strategies 테이블은 전략 인스턴스 목록(1:N) 비교/관리 UI다.

즉, 실행 단위 값을 전략 목록에 혼합하면 의미가 왜곡되고, 사용자가 "전략 속성"과 "현재 주문 상태"를 혼동하게 된다.

## 2. Decision

`EV/Stop/Gate/Score`의 1차 표시 책임을 Strategy Table에서 제거하고, Order/Position Panel로 이동한다.

- Strategy Table은 전략 자체의 상태/성과/신호 중심으로 유지한다.
- Order/Position Panel은 현재 실행 컨텍스트(EV/Stop/Gate/Expected Hold)를 단일 소스로 표시한다.

## 3. Scope and Non-Goals

### In Scope

1. Grid Strategies 컬럼 재정의
2. Position/Order 패널 정보 강화
3. 용어/라벨 정리(`EV@entry`, `Gate(at entry)` 등)

### Non-Goals

1. EV 모델 수식 변경
2. Exit Orchestrator 정책 변경
3. 전체 레이아웃 전면 개편

## 4. Proposed UI Model

### 4.1 Strategy Table (1:N)

유지/강화 대상:

1. Strategy ID / Symbol
2. Running status / Last signal / Signal age
3. W/L/T / Realized PnL
4. (선택) 전략 수준 집계 지표: 최근 N회 평균 EV 오차, stop coverage ratio

제거 대상(실행-결합값):

1. EV (point-in-time)
2. Score(p_win) (point-in-time)
3. Gate(blocked 여부 포함)
4. Stop(가격)

### 4.2 Order/Position Panel (1:1)

핵심 표시값:

1. `EV@entry`
2. `p_win@entry`
3. `Gate mode` / `blocked 여부`
4. `Stop price`
5. `expected_holding_ms` (또는 남은 시간)
6. `exit_reason_code` (포지션 종료 시 최근값)

표시 규칙:

1. Flat 상태에서는 `--` 표시
2. `blocked`는 red, `soft`는 amber
3. stop 미확보 시 명시적 경고(`STOP MISSING`)

## 5. Data Ownership Rule

표시 책임 규칙을 명시한다:

1. **Strategy-owned**: 전략 정의/설정/집계 성과/신호 추세
2. **Execution-owned**: 단일 포지션/주문 라이프사이클에서 생성되는 스냅샷 값(EV/Stop/Gate)

`Execution-owned` 값은 Strategy Table에서 직접 표시하지 않는다.

## 6. Migration Plan

### Phase A (Immediate)

1. Strategies 표에서 `EV/Score/Gate/Stop` 컬럼 제거
2. Position panel 라벨을 `EV@entry`, `pW@entry`, `Gate`, `Stop`, `Hold`로 정규화

### Phase B

1. History/EV Audit에서 execution 값 조회 강화
2. Strategies 표에는 execution raw 값 대신 집계 지표만 노출

## 7. Acceptance Criteria

1. Strategies 탭만 보고도 "전략 간 비교"가 명확하다.
2. EV/Stop/Gate는 Position/Order 패널에서만 조회된다.
3. 사용자 테스트에서 EV를 전략 속성이 아니라 "진입 시점 실행 컨텍스트"로 올바르게 해석한다.

## 8. Risks and Mitigations

- Risk: Strategy Table 정보가 줄어들어 아쉽게 느껴질 수 있음
- Mitigation: 집계형 대체 지표(예: EV calibration error, stop coverage) 제공

- Risk: 기존 사용자 혼란
- Mitigation: 릴리즈 노트/키 가이드에 "Execution vs Strategy" 구분 명시

## 9. Open Questions

1. Order panel 명칭을 `Position`에서 `Execution`으로 바꿀지?
2. Strategies 탭에 execution 집계 지표를 기본 표시할지 토글로 둘지?
3. EV Audit 기본 진입점은 Grid History 탭으로 둘지 별도 단축키를 둘지?
