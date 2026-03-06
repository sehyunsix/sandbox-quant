# RFC 0048: Alpha Predictor Regime-Specific Validation and Gating

- Status: Proposed
- Author: Codex + Project maintainer
- Date: 2026-03-04
- Related: 0046, 0047

## 1) Problem Statement

현재 실운영 신호 경로는 `alpha signal -> portfolio decision`으로 단순 연결되어 있고,
최근 백테스트에서 모델별/구간별 성능이 과도하게 불안정해 보인다.

실험 결과 요약:

- BTCUSDT(100k bars, 20 folds)
  - Realized PnL: `-1.563951`
  - Fees: `1.499753`
  - Trades: `52`
  - 모든 수익 거래는 `HOLD` 전환(축소) 단계였고, 실제 진입/청산 판단은 `sell` 중심 손실이 주도
- ETHUSDT(10k bars, 18 folds)
  - Realized PnL: `-3.984014`
  - Fees: `5.420938`
  - Trades: `181`

모델 기여도(누적 realized PnL)로 보면:

- BTC: `holt-fast-v1`이 거래 수와 손실의 대부분을 차지
  - SELL: `13`, PnL `-1.045909`
- ETH: `holt-fast-v1` SELL 53건 `-2.116833`, `ar1-fast-v1` SELL 24건 `-1.765212`
- 소수 모델(예: `holt-v1`, `ewma-fast-v1`, `feat-rls-fast-v1`)은 표본이 적거나 손익 기여도가 낮음

현재 상태에서는 모델 우열 판단과 레짐별 강약 조정이 어렵고, 동일 alpha 정책을 모든 시장 국면에 그대로 적용해 비용 소모가 큼.

## 2) Goals

1. 동일 전략에서 레짐별 alpha 성과를 분리 측정
2. 모델/레짐별로 다른 규칙으로 진입/축소 결정
3. 거래비용이 기대이익을 넘어서는 국면에서는 즉시 진입 억제
4. 과도한 요청 빈도나 동기화로 인한 실시간 운영 리스크를 낮추는 사전 가드 추가

## 3) Proposed Changes

### 3.1 Regime 분해 분석

- `trend_up`, `range`, `trend_down`으로 시장 국면을 분기.
- 각 국면별로 모델 집계 지표를 별도 저장:
  - 거래수, 승률, realized_pnl, fee, avg alpha, hold duration
- Backtest 결과 레포트에서 국면별 샘플 수 대비 기대값 확인

### 3.2 Portfolio Decision Gate 강화

- 포지션 변경 전 `expected_return_usdt`와 예상 슬리피지/수수료를 비교
  - `abs(exp_ret) < fee_buffer + spread_buffer` 이면 `Hold`
- `range`에서는 기본 멀티플 적용(예: `0.2~0.4`)
- `regime confidence`가 낮으면 `hold_multiplier` 축소
- 모델 top1 선택 기준을 절대값 alpha 외에
  - 최근 구간의 hit-rate/MAE/샤프 추이 기반 보정 점수로 변경

### 3.3 Anti-churn/Turnover Controls

- `min_hold_bars` 또는 `min_reentry_gap_ms` 추가
- 매도/축소 직전의 동일 방향 연속 시그널은 감쇠
- 1회성 과대 변동 구간에서 주문량 캡(예: `order_amount_usdt * 0.5`)

### 3.4 실행환경 가드

- 주문/이력 동기화 스케줄러를 상한 제어:
  - 백그라운드 심볼 동기화 주기 최소화(요청 병렬 수 + 리퀘스트 간 최소 간격)
  - 최근 갱신 시점 기반 skip 로직으로 중복 Sync 방지
- API 에러 시 즉시 지수 백오프를 적용하고 동일 시점 재시도 억제

## 4) Implementation Steps

1. Backtest에 레짐 라벨/모델 라벨을 `order_ledger` 컬럼 및 질의 뷰로 노출
2. `decision` 로그를 alpha-only 기준 + regime/context로 정규화
3. 포트폴리오 게이트 파라미터(기대값-비용, hold cooldown, min signal)를 실시간 설정 가능하도록 추가
4. 순차 동기화 주기 검증: `order_history_sync`가 심볼 당 최소 간격을 침범하지 않도록 고정
5. 실험: 
   - baseline / regime gate on / min-hold / turnover cap 조합
   - 동일 run set에서 3-way 비교 후 gate 파라미터 결정

## 5) Acceptance Criteria

- BTC/ETH 1m 샘플에서 동일 조건으로 연속 실행 시
  - 총 realized 손익이 기준선 대비 단기 개선되거나, 최소한 비용 대비 손실 하락
  - 모델별 SELL 기여도가 과도하게 한 모델에 몰리지 않고 분산
- 백테스트/운영 로그에서 `portfolio decision`은 `alpha, regime, cost gate`가 함께 추적됨
- API 동기화 요청이 규칙 기반 상한을 넘지 않음(분당 요청 수 급증 없음)
