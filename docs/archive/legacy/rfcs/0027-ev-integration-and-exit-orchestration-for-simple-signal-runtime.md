# RFC 0027: EV Integration and Exit Orchestration for `Buy|Sell|Hold` Runtime

- Status: Draft
- Author: sandbox-quant
- Date: 2026-02-24
- Related:
  - `docs/rfcs/0026-position-lifecycle-exit-expectancy-and-session-fail-safe.md`
  - `src/model/signal.rs`
  - `src/main.rs`
  - `src/order_manager.rs`
  - `src/model/position.rs`
  - `src/order_store.rs`

## 1. Problem

현재 전략 런타임은 `Signal::Buy | Signal::Sell | Signal::Hold`를 생성하고, `main.rs`의 `risk_eval_rx` 경로에서 `OrderManager::submit_order`를 호출한다.

이 구조는 단순하고 안정적이지만, 다음 공백이 있다.

1. 진입 순간 EV(기댓값) 계산/저장이 없다.
2. 청산이 전략 반전 시그널에 과도하게 의존한다.
3. stop-loss 기반 보호 청산과 전략 청산의 우선순위가 명시되지 않았다.

## 2. Goal

`Signal` enum을 즉시 대규모 변경하지 않고도 아래를 달성한다.

1. 진입 직전 EV 계산 및 포지션 메타데이터 저장
2. 명시적 청산 오케스트레이션(전략 시그널 + 시스템 정책)
3. stop-loss 우선 보호 정책과 예외 시 긴급 청산

## 3. Non-Goals

- 전략별 시그널 인터페이스를 당장 `Buy/Sell/Hold` 밖으로 확장하지 않는다.
- 백테스트 엔진 전면 재작성은 포함하지 않는다.

## 4. Design Summary

핵심 원칙: **Signal은 유지하고, 실행 계층에 EV/Exit 어댑터를 추가**한다.

### 4.1 New Runtime Components

1. `EvEstimator`
- 입력: `source_tag`, `instrument`, 최근 체결/성과 통계
- 출력: `EntryExpectancySnapshot`

2. `PositionLifecycleEngine`
- 역할:
  - 진입 이벤트 시 lifecycle state 생성
  - 틱마다 MFE/MAE 갱신
  - 시간/리스크 기반 exit condition 평가

3. `ExitOrchestrator`
- 역할:
  - 청산 이벤트 우선순위 결정
  - stop-loss 보호 주문 유효성 점검
  - 필요 시 `emergency_close` 실행

### 4.2 Minimal Data Model Additions

`Signal` 변경 없이 아래 런타임 상태를 추가한다.

```rust
struct EntryExpectancySnapshot {
    expected_return_usdt: f64,
    expected_holding_ms: u64,
    worst_case_loss_usdt: f64,
    confidence: EvConfidence,
    model_version: String,
    computed_at_ms: u64,
}

struct PositionLifecycleState {
    position_id: String,
    source_tag: String,
    instrument: String,
    opened_at_ms: u64,
    entry_price: f64,
    current_qty: f64,
    mfe_usdt: f64,
    mae_usdt: f64,
    stop_loss_order_id: Option<String>,
    expectancy: EntryExpectancySnapshot,
}
```

## 5. Integration Into Current Flow

현재 흐름:
- `strategy.on_tick` -> `risk_eval_tx.send((signal, source_tag, instrument))`
- `risk_eval_rx.recv()` -> `mgr.submit_order(signal, source_tag)`

변경 흐름(핵심):

1. `signal != Hold` 수신 시, 주문 전 `ExecutionIntentContext` 구성
2. `Buy`이고 flat -> `EvEstimator`로 EV 계산
3. `submit_order` 성공 후 fill 확인 시 lifecycle state 생성
4. lifecycle state 생성 직후 stop-loss 보호 주문 생성/검증
5. tick 루프에서 `PositionLifecycleEngine::on_tick` 호출하여 MFE/MAE 및 exit condition 갱신
6. exit condition이 true면 `ExitOrchestrator`가 청산 신호 생성(내부 시스템 이벤트)

## 6. Exit Orchestration Policy

청산 우선순위(높음 -> 낮음):

1. `exit.stop_loss_protection` (보호 주문 체결/유효성 실패 대응)
2. `exit.max_holding_time` (time stop)
3. `exit.risk_degrade` (리스크 상태 악화)
4. `exit.signal_reversal` (기존 Sell 반전 시그널)

정책 규칙:

1. stop-loss 보호 주문은 진입 직후 필수 검증
2. 보호 주문이 없거나 취소/거부 상태면 즉시 보정 시도
3. 보정 실패 시 `exit.emergency_close`로 시장가 청산
4. 모든 청산은 `exit_reason_code`를 강제 기록

## 7. EV Calculation (v1)

### 7.1 Input Features (v1, 단순형)

1. 최근 N건 동일 `source_tag + instrument` 트레이드
2. win-rate, avg win/loss, median holding time
3. 최근 변동성(간단히 tick/close 분산 기반)

### 7.2 Output

`EV = p(win) * avg_win - p(loss) * avg_loss - fee_slippage_penalty`

산출물:
- `expected_return_usdt`
- `expected_holding_ms`
- `worst_case_loss_usdt` (과거 손실 분포 하위 quantile 기반)

### 7.3 Guardrail

아래 조건이면 진입 자체를 제한 가능(설정 기반):
- `expected_return_usdt <= 0`
- `worst_case_loss_usdt > risk_cap`
- 샘플 수 부족(신뢰도 `low`)

## 8. Storage Contract

기존 `order_store` 확장 필드(개념):

1. `position_id`
2. `exit_reason_code`
3. `holding_ms`
4. `mfe_usdt`, `mae_usdt`
5. `expected_return_usdt_at_entry`
6. `ev_model_version`

효과:
- 전략별 EV 예측 대비 실제 성과 오차 추적
- 청산 사유별 성과 분해 분석 가능

## 9. API and Module Boundaries

### 9.1 `OrderManager` changes (최소)

1. 기존 `submit_order(signal, source_tag)`는 유지
2. 신규 오버로드/보조 메서드:
- `submit_order_with_context(signal, source_tag, ctx)`
- `place_protective_stop(position_id, ...)`
- `emergency_close(position_id, reason_code)`

### 9.2 `main.rs` changes (최소)

1. `risk_eval_rx` 분기에서 EV 계산/컨텍스트 구성
2. `tick_rx` 분기에서 lifecycle engine tick 갱신
3. `ExitOrchestrator`가 생성한 내부 청산 이벤트 채널 추가

## 10. Migration Plan

### Phase A: Shadow Mode

- EV 계산은 하되 주문 차단은 하지 않음
- `expected_return_usdt_at_entry`와 실제 결과 오차만 기록

### Phase B: Soft Gate

- EV가 임계치 이하일 때 warning + 로그만 발생
- 설정으로 전략별 gate on/off

### Phase C: Hard Gate + Exit Policy Enforcement

- EV gate 활성화 전략은 진입 차단 가능
- stop-loss 누락/비정상 시 즉시 보정 또는 긴급 청산

## 11. Testing Plan

테스트는 모두 `tests/`에 추가한다.

1. `Buy` 진입 시 EV snapshot 생성/기록 검증
2. `Sell` 반전 외에 time-stop 조건으로 청산되는지 검증
3. stop-loss 누락 상태에서 보정 -> 실패 -> emergency_close 폴백 검증
4. MFE/MAE 누적 및 종료 시 저장값 일치 검증
5. EV shadow mode에서 거래 결과가 기존 대비 변하지 않는지 검증

## 12. Acceptance Criteria

1. `Signal` 인터페이스 유지 상태에서 EV 기록이 동작
2. 포지션별 `exit_reason_code`, `holding_ms`, `mfe/mae` 조회 가능
3. stop-loss 보호 주문 점검 및 예외 폴백이 동작
4. shadow mode에서 기존 주문 체결 경로 회귀 없음
5. `tests/` 회귀 테스트 통과

## 13. Risks and Mitigations

- Risk: runtime 복잡도 증가
- Mitigation: `EvEstimator`, `LifecycleEngine`, `ExitOrchestrator`를 모듈 분리

- Risk: EV 품질 부족으로 잘못된 차단
- Mitigation: shadow -> soft -> hard 단계 적용

- Risk: 전략 Sell 신호와 시스템 청산 충돌
- Mitigation: 우선순위 정책 고정 + 단일 `exit_reason_code` 결정기

## 14. Open Questions

1. EV gate 기본값을 전역 OFF로 둘지, 일부 전략 ON으로 시작할지?
2. `expected_holding_ms` 초과 시 부분청산 vs 전량청산 중 기본 정책은 무엇인지?
3. stop-loss 기본 산식(고정 %, ATR, 변동성 적응형)의 초기 선택은 무엇인지?
