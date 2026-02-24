# RFC 0026: Position Lifecycle - Exit Expectancy, Session Fail-safe, and MFE Tracking

- Status: Draft
- Author: sandbox-quant
- Date: 2026-02-24
- Related:
  - `src/main.rs`
  - `src/model/signal.rs`
  - `src/model/position.rs`
  - `src/order_manager.rs`
  - `src/risk_module.rs`
  - `src/order_store.rs`

## 1. Problem Statement

현재 운영 리스크는 개별 이슈가 아니라, **포지션 라이프사이클 모델 부재**에서 동시에 발생한다.

1. 시그널로 포지션을 잡은 뒤 **언제/왜 정리할지** 기대값(EV) 계산이 없다.
2. 세션이 비정상 종료되면 **열린 포지션이 잔존**할 수 있다.
3. 포지션 보유 중 **최대 유리 구간(MFE: Maximum Favorable Excursion)** 추적이 없어, "얼마나 이익이 날 수 있었는지"를 정량화하지 못한다.

## 2. Root Cause Analysis

### 2.1 Signal Contract가 진입/청산 의도를 충분히 표현하지 못함

`Signal`은 현재 `Buy | Sell | Hold` 3값만 표현한다 (`src/model/signal.rs`).
이 구조에서는 아래 정보가 빠진다.

- 청산 근거(타임아웃/목표수익/손절/무효화)
- 보유 예상 시간
- 기대 손익 분포(평균, 분산, tail risk)

결과적으로 "진입은 가능하지만, 청산 설계는 전략 내부 암묵 상태에 의존"하게 된다.

### 2.2 Position State가 거래 수명주기 메트릭을 보존하지 않음

`Position`은 `entry_price`, `realized/unrealized_pnl`, `trade_count` 중심이다 (`src/model/position.rs`).
다음이 없다.

- 포지션별 최고/최저 유리도(MFE/MAE)
- 진입 후 경과 시간 기반 상태
- 청산 사유 및 정책 버전

이 때문에 EV 계산 및 사후 분석(왜 이 포지션이 그렇게 종료됐는가)이 불가능하다.

### 2.3 종료(Shutdown) 경로가 세션 저장 중심이고 포지션 안전 종료 보장이 없음

메인 루프 종료 시 수행되는 핵심은 전략 상태 저장(`strategy_session`)이다 (`src/main.rs` 종료 블록).
하지만 "열린 포지션을 강제 정리하거나 보호 주문을 확정"하는 별도 단계가 없다.
즉, 프로세스 장애/세션 종료 시점에 포지션 리스크가 런타임 외부로 누수될 수 있다.

### 2.4 리스크 모듈 범위가 주문 승인/노출 제한 중심으로 좁음

현재 리스크는 주로 rate budget, 쿨다운, 노출 상한, 수량 검증을 다룬다 (`src/risk_module.rs`, `config/default.toml`).
**출구 리스크(언제 정리해야 유리한가, 종료 실패 시 어떻게 방어할 것인가)**를 다루는 정책 계층이 분리되어 있지 않다.

## 3. Goals

1. 포지션 진입 시점에 청산 정책과 EV 기준을 명시적으로 부여한다.
2. 세션 정상/비정상 종료에서도 포지션 방치 리스크를 줄이는 fail-safe를 도입한다.
3. MFE/MAE를 표준 메트릭으로 수집해 전략 품질을 계량화한다.

## 4. Non-Goals

- 거래소 확장(멀티 브로커) 자체를 본 RFC 범위에 포함하지 않는다.
- 전략 알파 모델 자체(신규 인디케이터 설계)를 본 RFC에서 다루지 않는다.

## 5. Proposal

### 5.1 Position Lifecycle Policy 도입

`entry -> manage -> exit -> postmortem`를 명시적 상태로 정의한다.

- Entry: 진입 승인 + 청산 정책 스냅샷 부착
- Manage: 시간 경과/가격 경로 기반 관리
- Exit: 정책 사유 코드 기반 종료
- Postmortem: MFE/MAE/실현손익/슬리피지 기록

청산 사유 코드는 최소 아래 taxonomy를 권장한다.

- `exit.tp_hit`
- `exit.sl_hit`
- `exit.time_stop`
- `exit.signal_reversal`
- `exit.session_fail_safe`

### 5.2 Exit Expectancy Engine (EV Layer)

진입 직후 각 포지션에 대해 최소 EV 지표를 계산/저장한다.

- `expected_return_usdt`
- `expected_holding_ms`
- `worst_case_loss_usdt` (보수적 손실 추정)
- `policy_confidence` (low/medium/high)

초기 버전은 단순 모델(최근 N개 유사 트레이드 기반)로 시작하고, 이후 고도화한다.

### 5.3 Session Fail-safe (2-Layer)

1. **Broker-side protection first**: 진입 직후 보호성 `stop-loss`(가능하면 reduce-only/bracket) 주문을 우선 배치한다.
2. **Shutdown validation + emergency fallback**: 종료 신호 수신 시 stop-loss 유효성/잔존 수량을 점검하고, 보호 주문이 없거나 비정상일 때만 긴급 시장가 청산을 실행한다.

Fail-safe는 기존 전략 시그널 경로와 분리된 시스템 정책으로 동작해야 한다.

### 5.4 MFE/MAE Tracking

포지션 유지 중 틱 업데이트마다 excursion을 갱신한다.

- `mfe_usdt` = 진입 이후 최대 미실현 이익
- `mae_usdt` = 진입 이후 최대 미실현 손실

포지션 종료 시 order history/store에 함께 영속화하여 전략별 통계를 제공한다.

## 6. Data Contract Changes

최소 추가 필드(개념):

- Position runtime:
  - `opened_at_ms`
  - `mfe_usdt`
  - `mae_usdt`
  - `exit_policy_id`
- Trade/history persistence:
  - `exit_reason_code`
  - `holding_ms`
  - `mfe_usdt`
  - `mae_usdt`
  - `expected_return_usdt_at_entry`

## 7. Execution Plan

### Phase 1: Observability-first

- MFE/MAE 계산 추가 (체결/틱 경로)
- 종료 사유 코드 스키마 추가
- 히스토리 저장/조회 경로 확장

### Phase 2: Exit policy explicitization

- 전략 진입 시 `ExitPolicy` 객체 생성/주입
- 리스크/주문 레이어에 `exit.time_stop`, `exit.session_fail_safe` 지원

### Phase 3: Session fail-safe hardening

- 진입 직후 stop-loss 주문 강제 검증(미배치/거부 시 진입 차단 또는 즉시 보정)
- 종료 훅에서 stop-loss 유효성 점검 후 예외 상황에만 긴급 청산
- 실패 시 재시도/경고 및 긴급 모드(reason code) 기록

### Phase 4: EV loop

- 과거 트레이드 기반 EV 추정치 계산
- 진입 시점 EV와 실제 결과 오차 추적(캘리브레이션)

## 8. Test Strategy

테스트는 모두 `tests/`에만 추가한다.

필수 테스트:

1. 세션 종료 시 오픈 포지션 fail-safe 정리 동작 검증
2. 진입 직후 stop-loss 주문 생성/유효성 검증 동작 검증
3. 포지션 유지 중 MFE/MAE 누적 정확성 검증
4. 청산 사유 코드 저장/조회 round-trip 검증
5. EV 계산 결과가 포지션 생성 시 누락 없이 기록되는지 검증

## 9. Risks and Mitigations

- Risk: 보호 주문(stop-loss)과 전략 청산 시그널 간 충돌 가능
- Mitigation: 정책 우선순위를 명시(시스템 보호 주문 > 전략 신호), reduce-only 제약 적용

- Risk: MFE/MAE 계산으로 상태 업데이트 부하 증가
- Mitigation: tick 처리 경량화 및 샘플링/배치 옵션

- Risk: EV가 초기엔 부정확할 수 있음
- Mitigation: 신뢰도 레벨 공개 + 오차 모니터링

## 10. Acceptance Criteria

1. 포지션 단위로 `exit_reason_code`, `holding_ms`, `mfe_usdt`, `mae_usdt` 조회 가능
2. 진입 포지션은 stop-loss 보호 주문이 존재/유효해야 하며, 누락 시 경고와 보정이 동작함
3. 정상 종료 및 Ctrl+C 종료에서 stop-loss 점검 -> 예외 시 긴급 청산 시퀀스가 실행되고 결과가 로그/이력에 남음
4. 진입 시점 EV와 실제 결과 비교 리포트 생성 가능
5. 관련 회귀 테스트가 `tests/`에서 통과

## 11. Open Questions

1. stop-loss 기본 폭(고정 %, ATR 기반, 변동성 기반) 중 기본값을 무엇으로 둘지?
2. Futures/Spot에서 보호 주문 정책을 동일 taxonomy로 통합할지?
3. EV 계산 윈도우(최근 N건/N일) 기본값을 얼마로 둘지?
