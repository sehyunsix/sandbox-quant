# RFC 0029: EV + Exit Minimum Implementation Spec

- Status: Draft
- Author: sandbox-quant
- Date: 2026-02-24
- Related:
  - `docs/rfcs/0027-ev-integration-and-exit-orchestration-for-simple-signal-runtime.md`
  - `docs/rfcs/0028-ev-probability-estimation-framework.md`
  - `src/main.rs`
  - `src/order_manager.rs`
  - `src/order_store.rs`

## 1. Scope

본 RFC는 0027/0028을 실제 코드로 옮기기 위한 **최소 구현 스펙(MVP)** 을 정의한다.

목표:

1. `Buy|Sell|Hold` 인터페이스 유지
2. 진입 시 EV/확률 스냅샷 계산 및 저장
3. stop-loss 보호 주문 점검 + 예외 시 긴급청산 경로 도입

## 2. Module Plan

신규 모듈:

1. `src/ev/mod.rs`
2. `src/ev/types.rs`
3. `src/ev/estimator.rs`
4. `src/lifecycle/mod.rs`
5. `src/lifecycle/engine.rs`
6. `src/lifecycle/exit_orchestrator.rs`

`src/lib.rs`에 `pub mod ev; pub mod lifecycle;` 추가.

## 3. Core Types (Draft Signatures)

```rust
// src/ev/types.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfidenceLevel { Low, Medium, High }

#[derive(Debug, Clone)]
pub struct ProbabilitySnapshot {
    pub p_win: f64,
    pub p_tail_loss: f64,
    pub p_timeout_exit: f64,
    pub n_eff: f64,
    pub confidence: ConfidenceLevel,
    pub prob_model_version: String,
}

#[derive(Debug, Clone)]
pub struct EntryExpectancySnapshot {
    pub expected_return_usdt: f64,
    pub expected_holding_ms: u64,
    pub worst_case_loss_usdt: f64,
    pub fee_slippage_penalty_usdt: f64,
    pub probability: ProbabilitySnapshot,
    pub ev_model_version: String,
    pub computed_at_ms: u64,
}
```

```rust
// src/lifecycle/engine.rs
#[derive(Debug, Clone)]
pub struct PositionLifecycleState {
    pub position_id: String,
    pub source_tag: String,
    pub instrument: String,
    pub opened_at_ms: u64,
    pub entry_price: f64,
    pub qty: f64,
    pub mfe_usdt: f64,
    pub mae_usdt: f64,
    pub expected_holding_ms: u64,
    pub stop_loss_order_id: Option<String>,
}

#[derive(Debug, Clone)]
pub enum ExitTrigger {
    StopLossProtection,
    MaxHoldingTime,
    RiskDegrade,
    SignalReversal,
    EmergencyClose,
}
```

## 4. Estimator API (Draft Signatures)

```rust
// src/ev/estimator.rs
pub struct EvEstimatorConfig {
    pub prior_a: f64,
    pub prior_b: f64,
    pub tail_prior_a: f64,
    pub tail_prior_b: f64,
    pub recency_lambda: f64,
    pub shrink_k: f64,
    pub loss_threshold_usdt: f64,
    pub timeout_ms_default: u64,
    pub gamma_tail_penalty: f64,
}

pub trait TradeStatsReader {
    fn load_local_stats(&self, source_tag: &str, instrument: &str, lookback: usize) -> anyhow::Result<TradeStatsWindow>;
    fn load_global_stats(&self, source_tag: &str, lookback: usize) -> anyhow::Result<TradeStatsWindow>;
}

pub struct EvEstimator<R: TradeStatsReader> {
    cfg: EvEstimatorConfig,
    reader: R,
}

impl<R: TradeStatsReader> EvEstimator<R> {
    pub fn estimate_entry_expectancy(
        &self,
        source_tag: &str,
        instrument: &str,
        now_ms: u64,
    ) -> anyhow::Result<EntryExpectancySnapshot>;
}
```

## 5. Lifecycle/Exit API (Draft Signatures)

```rust
// src/lifecycle/engine.rs
pub struct PositionLifecycleEngine {
    states: std::collections::HashMap<String, PositionLifecycleState>, // key: instrument
}

impl PositionLifecycleEngine {
    pub fn on_entry_filled(
        &mut self,
        instrument: &str,
        source_tag: &str,
        entry_price: f64,
        qty: f64,
        expectancy: &EntryExpectancySnapshot,
        now_ms: u64,
    ) -> String; // returns position_id

    pub fn on_tick(&mut self, instrument: &str, mark_price: f64, now_ms: u64) -> Option<ExitTrigger>;

    pub fn set_stop_loss_order_id(&mut self, instrument: &str, order_id: Option<String>);

    pub fn on_position_closed(&mut self, instrument: &str) -> Option<PositionLifecycleState>;
}
```

```rust
// src/lifecycle/exit_orchestrator.rs
pub struct ExitOrchestrator;

impl ExitOrchestrator {
    pub fn decide(trigger: ExitTrigger) -> &'static str; // exit_reason_code
}
```

## 6. `main.rs` Integration Points

### 6.1 Risk Eval Branch

대상: `risk_eval_rx.recv()` 분기

작업:

1. `Buy` + flat 상태일 때 `estimate_entry_expectancy(...)` 호출
2. `OrderManager::submit_order` 성공 후 `OrderUpdate::Filled`이면 lifecycle `on_entry_filled(...)`
3. entry 직후 보호 주문 생성 함수 호출

### 6.2 Tick Branch

대상: `tick_rx.recv()` 분기

작업:

1. `PositionLifecycleEngine::on_tick(...)`
2. `Some(trigger)`이면 내부 청산 큐(`internal_exit_tx`)로 이벤트 발행

### 6.3 Internal Exit Branch

신규 채널:

```rust
let (internal_exit_tx, mut internal_exit_rx) = mpsc::channel::<(String, ExitTrigger)>(64);
```

작업:

1. trigger -> `exit_reason_code` 결정
2. `Sell` 청산 주문 실행
3. 실패 시 `emergency_close` 경로

## 7. `OrderManager` Minimal Extension

신규 메서드(초안):

```rust
impl OrderManager {
    pub async fn place_protective_stop_for_open_position(
        &mut self,
        source_tag: &str,
        stop_price: f64,
    ) -> anyhow::Result<Option<String>>; // order_id

    pub async fn ensure_protective_stop(
        &mut self,
        source_tag: &str,
        fallback_stop_price: f64,
    ) -> anyhow::Result<bool>; // valid or repaired

    pub async fn emergency_close_position(
        &mut self,
        source_tag: &str,
        reason_code: &str,
    ) -> anyhow::Result<()>;
}
```

주의:

1. spot/futures 주문 API 차이를 `OrderManager` 내부에서 캡슐화
2. futures는 가능하면 reduce-only 사용

## 8. DB Migration Draft

대상: `order_history_trades` 중심 확장

```sql
ALTER TABLE order_history_trades ADD COLUMN position_id TEXT;
ALTER TABLE order_history_trades ADD COLUMN exit_reason_code TEXT;
ALTER TABLE order_history_trades ADD COLUMN holding_ms INTEGER NOT NULL DEFAULT 0;
ALTER TABLE order_history_trades ADD COLUMN mfe_usdt REAL NOT NULL DEFAULT 0.0;
ALTER TABLE order_history_trades ADD COLUMN mae_usdt REAL NOT NULL DEFAULT 0.0;
ALTER TABLE order_history_trades ADD COLUMN expected_return_usdt_at_entry REAL;
ALTER TABLE order_history_trades ADD COLUMN p_win_estimate REAL;
ALTER TABLE order_history_trades ADD COLUMN p_tail_loss_estimate REAL;
ALTER TABLE order_history_trades ADD COLUMN p_timeout_exit_estimate REAL;
ALTER TABLE order_history_trades ADD COLUMN prob_model_version TEXT;
ALTER TABLE order_history_trades ADD COLUMN ev_model_version TEXT;
ALTER TABLE order_history_trades ADD COLUMN confidence_level TEXT;
ALTER TABLE order_history_trades ADD COLUMN n_eff REAL;
```

보조 인덱스:

```sql
CREATE INDEX IF NOT EXISTS idx_order_history_position_id ON order_history_trades(position_id);
CREATE INDEX IF NOT EXISTS idx_order_history_exit_reason ON order_history_trades(exit_reason_code);
```

## 9. Config Draft

`config/default.toml`에 추가 제안:

```toml
[ev]
enabled = true
mode = "shadow" # shadow | soft | hard
lookback_trades = 200
prior_a = 6.0
prior_b = 6.0
tail_prior_a = 3.0
tail_prior_b = 7.0
recency_lambda = 0.08
shrink_k = 40.0
loss_threshold_usdt = 15.0
gamma_tail_penalty = 0.8
entry_gate_min_ev_usdt = 0.0

[exit]
max_holding_ms = 1_800_000
stop_loss_pct = 0.015
enforce_protective_stop = true
```

## 10. Step-by-Step Implementation

1. DB 컬럼 추가 + `order_store` read/write 확장
2. `ev` 모듈 도입(추정 + snapshot 반환)
3. `lifecycle` 모듈 도입(MFE/MAE 및 timeout 트리거)
4. `main.rs`에 internal exit channel 및 분기 연결
5. `OrderManager` 보호주문/긴급청산 메서드 추가
6. shadow mode 검증 후 soft/hard 확장

## 11. Test Matrix (`tests/`)

1. `ev_estimator_tests.rs`
- posterior/recency/shrink 계산 검증

2. `position_lifecycle_engine_tests.rs`
- MFE/MAE 및 timeout trigger 검증

3. `exit_orchestrator_tests.rs`
- trigger 우선순위 -> reason_code 매핑 검증

4. `order_store_ev_fields_tests.rs`
- migration 이후 round-trip 검증

5. `strategy_runtime_ev_shadow_tests.rs`
- shadow 모드에서 기존 주문 흐름 불변 검증

## 12. Acceptance Criteria

1. shadow 모드에서 EV/확률/청산 메타데이터가 저장됨
2. stop-loss 누락 감지 시 보정 또는 긴급청산 경로가 동작
3. timeout 기반 청산이 `exit_reason_code`와 함께 기록됨
4. 신규 테스트가 `tests/`에서 통과

## 13. Open Decisions

1. `max_holding_ms` 기본값을 전략별 override 가능하게 할지?
2. `stop_loss_pct` vs ATR 기반 stop 중 기본 구현 우선순위?
3. hard 모드 적용 대상을 전체 전략이 아닌 화이트리스트로 시작할지?
