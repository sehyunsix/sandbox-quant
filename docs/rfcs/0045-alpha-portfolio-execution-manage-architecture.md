# RFC 0045: Alpha-Portfolio-Execution-Manage Layered Architecture

- Status: Proposed
- Author: Codex (GPT-5) + project maintainer
- Date: 2026-03-03
- Related: 0023, 0026, 0032, 0039, 0040, 0041, 0042, 0043

## 1. Background

현재 런타임은 사실상 `Strategy -> Signal -> Order` 중심 구조다.

이 구조는 다음 문제가 있다.

- 포지션 관리 책임이 여러 레이어에 분산되어 디버깅이 어렵다.
- 진입/청산 의사결정이 시그널 이벤트에 묶여 상태 기반 제어가 약하다.
- 포트폴리오 전체 관점의 sizing/exposure 제어가 일관되지 않다.
- Fill/OMS/Reconcile/Risk sync와 전략 로직의 경계가 불명확하다.

## 2. Problem Statement

시스템은 Alpha 예측을 사용하고 있지만, "다음에 어떤 포지션 상태를 가져가야 하는지"를
포트폴리오 상태 관점에서 일관되게 결정하지 못한다.

그 결과:

1. 기존 포지션(특히 과거 Alpha에서 생성된 포지션)의 수명주기 관리가 어렵다.
2. 주문 집행 이후 상태 동기화와 리스크 노출 관리가 분절된다.
3. 신규 모델/알파 추가 시 전략 레이어 복잡도만 증가한다.

## 3. Goals

1. 전략 레이어를 제거하고 Alpha를 예측 전용으로 축소한다.
2. Portfolio 레이어가 계좌/포지션/리스크 상태를 기반으로 목표 포지션을 결정한다.
3. Execution 레이어가 목표-현재 delta를 주문으로 변환한다.
4. Manage System(OMS, Fill Processor, Reconcile, Risk Sync)으로 동기화/무결성을 보장한다.
5. 단계적 전환(Shadow -> Cutover)으로 운영 리스크를 낮춘다.

## 4. Non-goals

1. 본 RFC에서 알파 모델의 성능 자체를 개선하지 않는다.
2. 본 RFC에서 거래소 API 스펙/브로커 어댑터를 재설계하지 않는다.
3. 한 번에 모든 UI/메트릭 화면을 교체하지 않는다.

## 5. Target Architecture

`Alpha -> Portfolio -> Execution -> Manage`

### 5.1 Alpha Layer

- 입력: 시장 데이터, 특징량
- 출력: `AlphaSignal`
- 책임: 예측만 수행 (의사결정/주문 금지)

`AlphaSignal` 최소 필드:

- `symbol`
- `side_bias` (long/short/flat bias)
- `strength`
- `expected_return`
- `risk_estimate`
- `horizon`
- `confidence`
- `timestamp_ms`

### 5.2 Portfolio Layer

- 입력: `AlphaSignal + PortfolioState + RiskBudget`
- 출력: `TargetPosition`
- 책임:
  - position sizing
  - existing position hold/reduce/exit 판단
  - exposure/turnover/risk cap 준수

### 5.3 Execution Layer

- 입력: `TargetPosition`, 현재 포지션/오더 상태
- 출력: 주문 의도/주문 요청
- 책임:
  - delta 계산
  - 주문 슬라이싱/최소수량/예약증거금 고려
  - 주문 정책(IOC/GTC 등) 적용

### 5.4 Manage System

- OMS: 주문 상태 머신(`new/accepted/partial/filled/canceled/rejected`)
- Fill Processor: fill 이벤트를 PortfolioState에 반영
- Reconcile: 거래소 스냅샷과 내부 상태 diff 보정
- Risk Sync: exposure metric 업데이트 및 리스크 게이트 반영

## 6. Portfolio State Model

Portfolio 레이어는 최소 아래 상태를 가진다.

1. Symbol별 포지션
2. 포지션별 realized/unrealized PnL
3. Fees
4. Open orders + reserved cash/margin
5. Risk용 exposure metrics

업데이트 규칙:

1. Fill 이벤트 직후 즉시 반영
2. 거래소 periodic snapshot sync 반영
3. Symbol/포지션 업데이트 후 exposure metric 재계산

## 7. Event and Source-of-Truth Policy

1. 실시간 소스: 주문/체결 이벤트
2. 보정 소스: 주기적 계좌/포지션/오더 스냅샷
3. 충돌 시: 최신 거래소 스냅샷 우선, 내부 상태는 reconcile event로 수정
4. 모든 반영은 idempotency key 기반으로 중복 처리 안전성 보장

## 8. Migration Plan

### Phase 0: Domain Freeze

- `PortfolioState`, `PositionState`, `OrderState`, `ExposureState`, `AccountState` 타입 고정
- 이벤트 스키마(`AlphaSignal`, `TargetPosition`, `OrderEvent`, `ReconcileDiff`) 고정

### Phase 1: Portfolio State Store

- 단일 읽기 지점으로 포트폴리오 상태 저장소 도입
- fill + snapshot sync 이중 업데이트 경로 구현

### Phase 2: Alpha Contract Standardization

- 기존 predictor 출력을 `AlphaSignal` 계약으로 표준화
- 전략 전용 필드/분기 제거

### Phase 3: Portfolio Decision Engine

- `AlphaSignal + state + risk` -> `TargetPosition`
- sizing/exposure/turnover 정책 탑재

### Phase 4: Execution Adapter

- target delta를 주문으로 변환
- OMS와 연결하여 주문 상태 전이 관리

### Phase 5: Manage Integration

- Fill Processor / Reconcile / Risk Sync를 포트폴리오 상태 업데이트의 공식 경로로 통합

### Phase 6: Shadow Run

- 기존 경로는 실제 주문 유지
- 신규 경로는 계산만 수행하고 diff/메트릭 기록

### Phase 7: Hard Cutover

- 신규 경로만 실제 주문 수행
- 기존 strategy order path read-only 후 제거

## 9. Observability and SLO

필수 지표:

1. target-vs-actual position deviation
2. reconcile diff rate
3. fill 처리 지연 (p50/p95/p99)
4. exposure breach count
5. 주문 실패율/거절 사유 분포

SLO 예시:

1. fill->state 반영 지연 p95 < 300ms
2. reconcile critical diff rate < 0.5%
3. exposure gate 누락 0건

## 10. Acceptance Criteria

1. 모든 주문은 `TargetPosition delta` 경로에서만 생성된다.
2. 포지션/PnL/Fee/open order/reserved margin이 단일 `PortfolioState`에서 조회된다.
3. fill 반영 + reconcile 보정이 idempotent하게 재현된다.
4. shadow 기간 동안 기존 대비 reconcile diff rate가 임계치 이하로 유지된다.
5. cutover 이후 strategy order path가 비활성화되고 제거 계획이 확정된다.

## 11. Risks and Mitigations

1. Risk: 전환 중 의사결정 불일치
   - Mitigation: shadow diff 대시보드 + 하드 게이트 전 검증 기간 운영

2. Risk: 상태 동기화 지연으로 잘못된 sizing
   - Mitigation: stale-state TTL + conservative fallback sizing

3. Risk: reconcile 과잉 보정
   - Mitigation: diff severity 레벨과 심각도별 자동/수동 정책 분리

4. Risk: 롤백 어려움
   - Mitigation: feature flag 기반 즉시 rollback path 유지

## 12. Rollback Plan

1. `portfolio_decision_engine_enabled=false` 플래그로 기존 경로 즉시 복귀
2. 신규 경로는 shadow-only 모드로 전환
3. cutover 기간에는 이전 경로 코드 제거를 금지하고 1~2주 보류

## 13. Open Questions

1. TargetPosition granularity를 symbol 단위로만 둘지, strategy attribution 필드를 유지할지?
2. Reserved margin 계산을 거래소 실측 기반으로만 둘지 내부 추정과 혼합할지?
3. Reconcile 자동 보정 임계치를 자산군(spot/futures)별로 다르게 둘지?

## 14. Decision

본 RFC가 승인되면, 구현은 다음 순서로 진행한다.

1. 상태 모델/이벤트 계약 고정
2. Portfolio Store + Decision Engine 도입
3. Execution/OMS/Reconcile 통합
4. Shadow 운영 후 Hard Cutover
5. Strategy 레이어 제거
