# RFC 0030: UI Integration for EV and Exit Price Visibility

- Status: Draft
- Author: sandbox-quant
- Date: 2026-02-24
- Related:
  - `docs/rfcs/0026-position-lifecycle-exit-expectancy-and-session-fail-safe.md`
  - `docs/rfcs/0027-ev-integration-and-exit-orchestration-for-simple-signal-runtime.md`
  - `docs/rfcs/0028-ev-probability-estimation-framework.md`
  - `docs/rfcs/0029-ev-exit-minimum-implementation-spec.md`
  - `src/ui/mod.rs`
  - `src/ui/dashboard.rs`

## 1. Problem

현재 EV/청산 정책이 런타임에 일부 도입되었지만, UI에서 사용자가 즉시 판단할 수 있는 가시성이 부족하다.

부족한 정보:

1. 진입 직전 EV가 얼마였는지
2. 현재 포지션의 예상 청산 가격(Stop/Timeout/Signal 기준)
3. 왜 청산됐는지(`exit_reason_code`)와 EV 대비 실제 결과 오차

결과적으로 "전략이 왜 지금 진입/청산되는지"를 사용자 관점에서 해석하기 어렵다.

## 2. Goal

현 화면 구조(메인 대시보드 + Grid + History Popup)를 유지하면서 EV/청산 가시성을 단계적으로 강화한다.

1. 메인 화면에서 현재 포지션의 EV/청산 가격을 즉시 확인
2. Grid Strategies 탭에서 전략별 EV 상태와 gate 상태 비교
3. History에서 진입 당시 EV와 실제 결과를 사후 분석 가능

## 3. Non-Goals

- UI 전체 레이아웃 재설계는 범위 밖
- 신규 차트 엔진 도입은 범위 밖

## 4. Current UI Surfaces (As-Is)

1. Main right panel:
- Position/Strategy metrics 분리(기존 RFC 0018 방향)
- 하지만 EV/exit price 필드 없음

2. Grid Strategies tab:
- 전략별 pnl/trade 중심
- EV gate 상태/예상 손익 미노출

3. History popup:
- 기간별 수익률 집계 중심
- `exit_reason_code`, EV snapshot 비교 없음

## 5. Proposed UX

### 5.1 Main Dashboard: Position Risk Strip 추가

Position panel 하단에 1줄 요약 스트립 추가:

- `EV@entry`: `+1.24 USDT`
- `p_win`: `0.58`
- `Stop`: `24510.50`
- `Timeout`: `12m left` 또는 `expired`
- `Gate`: `shadow | soft-warn | hard-block`

표시 규칙:

1. Flat 상태면 `--`
2. `soft-warn`은 amber, `hard-block` 이벤트는 red
3. stop 미확보 시 `STOP MISSING` 배지

### 5.2 Chart Overlay: Exit Price Line

메인 차트에 가격선 2종 추가:

1. `stop_price` (red dashed)
2. `projected_exit_price` (yellow dotted, optional)

마우스가 없는 TUI 특성상 범례를 차트 우상단 텍스트로 고정 노출:
- `STOP 24510.50`
- `XPRJ 24780.20`

### 5.3 Grid Strategies: EV Column Set

Strategies 표에 열 추가:

1. `EV`
2. `p_win`
3. `Gate`
4. `Last Exit`
5. `StopCov` (stop coverage ratio)

`StopCov` 정의:
- 최근 N개 진입 중 보호 stop 확보 성공 비율

### 5.4 History Popup: EV vs Realized Tab

기존 History popup에 토글 추가:

1. `Return` (기존)
2. `EV Audit` (신규)

`EV Audit` 행 컬럼:

- timestamp
- strategy/source
- entry price
- `ev_at_entry`
- `expected_holding_ms`
- `exit_reason_code`
- realized pnl
- `ev_error = realized - ev_at_entry`

## 6. Information Architecture

UI 레벨 상태 분리:

1. `PositionLiveView`
- 현재 포지션 중심(실시간)

2. `StrategyDecisionView`
- 전략별 EV/gate/last signal

3. `ExecutionAuditView`
- 청산 사유/예상-실제 오차

이 분리는 기존 `AppState`/`UiProjection` 리팩터 방향(0016/0018)과 정합성을 가진다.

## 7. Data/Event Contract Additions

신규/확장 이벤트 제안:

1. `AppEvent::EvSnapshotUpdate { symbol, source_tag, ev, p_win, gate_mode, gate_blocked }`
2. `AppEvent::ExitPolicyUpdate { symbol, stop_price, expected_holding_ms, protective_stop_ok }`
3. `AppEvent::LifecycleCloseAudit { symbol, source_tag, exit_reason_code, ev_at_entry, realized_pnl }`

`AppState` 저장 필드(개념):

1. `latest_ev_by_symbol_source`
2. `exit_policy_by_symbol`
3. `close_audit_rows`

## 8. Visual Priority Rules

사용자 혼동 방지를 위한 우선순위:

1. 시스템 리스크 경고(`STOP MISSING`, `HARD BLOCK`) > 전략 성과 수치
2. 현재 포지션 정보 > 과거 통계
3. 진입/청산 의사결정 근거 텍스트는 항상 숫자 옆에 표기

예:
- `Gate: hard-block (EV <= 0.0)`
- `Exit: stop_loss_protection`

## 9. Rollout Plan

### Phase A (Low risk)

1. Main panel 텍스트 필드 추가 (`EV@entry`, `Stop`, `Gate`)
2. Grid Strategies에 `EV`, `Gate` 열 추가

### Phase B

1. Chart stop/exit 라인 오버레이
2. History `EV Audit` 토글 추가

### Phase C

1. Stop coverage / EV calibration mini summary 추가
2. 전략별 EV drift 경고 배지

## 10. Acceptance Criteria

1. 포지션 보유 시 메인 화면에서 EV/stop/timeout/gate를 동시에 확인 가능
2. Grid Strategies에서 전략별 EV와 gate 상태 비교 가능
3. History EV Audit에서 `exit_reason_code`와 `ev_error` 조회 가능
4. `STOP MISSING`/`HARD BLOCK`이 시각적으로 즉시 구분됨

## 11. Risks and Mitigations

- Risk: 정보 과밀로 UI 가독성 저하
- Mitigation: 기본은 요약값만, 상세는 popup/tab으로 분리

- Risk: 실시간 이벤트 증가로 렌더 부하
- Mitigation: EV/정책 이벤트는 변경 시점에만 발행(틱마다 전체 갱신 금지)

- Risk: 용어 혼동(EV vs realized)
- Mitigation: 라벨에 시점 명시(`EV@entry`, `Realized`)

## 12. Open Questions

1. Main panel에서 `p_tail_loss`까지 기본 노출할지, tooltip/상세 탭으로 숨길지?
2. `projected_exit_price` 계산식(평균복귀 vs 변동성 기반)을 표준화할지?
3. EV Audit 기본 정렬을 시간순/오차절대값순 중 무엇으로 할지?
