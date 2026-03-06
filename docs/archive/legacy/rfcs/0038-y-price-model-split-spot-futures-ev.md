# RFC 0038: 가격모델 `Y` 기반 Spot/Futures 분리 EV 계산

- Status: Draft
- Author: sandbox-quant
- Date: 2026-02-25
- Related:
  - `docs/rfcs/0037-probabilistic-ev-random-model-and-hypotheses.md`
  - `docs/rfcs/0035-ev-time-variance-vs-entry-lock.md`

## 1. Problem Statement

현재 EV는 단순 확률(`p_win`) 중심이라, 시장/상품 구조(Spot vs Futures) 차이를 충분히 반영하지 못한다.  
EV를 직접적으로 계산하려면 가격/수익률 확률변수 `Y`의 분포를 먼저 모델링해야 한다.

## 2. Goal

- 공통 가격모델 `Y`를 기반으로:
  - `Spot PnL(Y)` 모델
  - `Futures PnL(Y)` 모델
  를 분리 설계한다.
- 두 모델에서 각각 `EV`를 계산하고, 포지션/게이트 정책에 사용한다.

## 3. Core Idea

1. 먼저 미래 가격변화 확률변수 `Y`를 예측한다.  
2. 상품별 손익함수 `g_m(Y)` (`m in {spot, fut}`)에 대입해 손익분포를 얻는다.  
3. `EV_m = E[g_m(Y)|x]` 계산.  
   (비용/패널티는 `g_m` 내부에 포함해 이중 차감 방지)

즉, `p_win`은 1차 목표가 아니라 파생치:
- `p_win_m = P(g_m(Y) > 0 | x)`

## 4. Variable Definition

- `P0`: 현재 기준가격
- `Y`: 예측 horizon의 로그수익률 (`log(P_T / P0)`)
- `M`: 선물 계약승수(기본 `M=1`, 선형 USDT-M 기준)
- `Q`: 수량
- `s`: 포지션 방향 (`+1` long, `-1` short)
- `c_fee`: 거래 수수료 비용(슬리피지 제외)
- `c_slippage`: 체결 슬리피지 비용
- `c_funding`: 선물 funding 비용(해당 horizon 누적)
- `c_borrow`: 현물 차입/금리 비용(필요시)

가격:
- `P_T = P0 * exp(Y)`

## 5. Profit Models

### 5.1 Spot Profit Model

- 기본 손익:
  - `PnL_spot(Y) = s * Q * (P_T - P0) - c_fee - c_borrow - c_slippage`
- 롱 전용이면 `s=+1`만 허용.

EV:
- `EV_spot = E[PnL_spot(Y) | x]`

### 5.2 Futures Profit Model

- 기본 손익:
  - `PnL_fut(Y) = s * M * Q * (P_T - P0) - c_fee - c_funding - c_slippage`
- 필요시 마크/청산 위험 패널티 추가:
  - `PnL_fut_adj(Y) = PnL_fut(Y) - c_liq_risk`

EV:
- `EV_fut = E[PnL_fut_adj(Y) | x]`

## 6. Distribution Model for `Y`

1차 권장:
- `Y | x ~ Normal(mu(x), sigma(x)^2)` (baseline, log-return 기준)

확장:
- t-distribution (fat-tail)
- mixture model (regime mixture)
- quantile model (direct VaR/CVaR 추정)

## 7. Feature Set (`x`)

- 시장: short/long return, realized vol, ATR, spread proxy
- 전략: source_tag, signal score, gate mode
- 실행: slippage, fill latency, rejection ratio
- 리스크: stop distance, leverage, exposure ratio

## 8. EntryEV / LiveEV Rule

- `EntryEV_m`: 진입 시점 `x_entry`로 계산 후 고정 저장
- `LiveEV_m`: 현재 시점 `x_t`로 재계산
- 표시/운영은 Dual EV 원칙 적용 (`RFC 0035`)
- 파생치:
  - `p_win_m = P(g_m(Y) > 0 | x)` (분포 기반 계산)

## 9. Decision Policy

진입:
- 시장 타입에 맞는 `EV_m` 사용
- `EV_m > gate_min` AND `confidence >= c_min` 일 때 통과

보유/청산:
- `LiveEV_m <= 0` 조건 + 히스테리시스(`N회` 또는 `T초`) 적용

Confidence 정의(초안):
- `confidence = 1 / (1 + EV_std / scale_usdt)`
- `EV_std = std(g_m(Y) | x)`  
- `scale_usdt`: 무차원화를 위한 스케일(예: 주문 notional 또는 고정 10 USDT)
- (`EV_std`가 작을수록 confidence 높음)

## 10. Validation Plan

오프라인:
- `Y` 예측 calibration (coverage, PIT, quantile hit-rate)
- 상품별 EV decile monotonicity
- Spot/Futures 분리 후 tail-loss 개선 확인

온라인(Shadow):
- 기존 EV와 병렬 출력
- 의사결정은 기존 유지, 성과 비교 로그 축적

## 11. Data Contract

추가 저장:
- `market_type` (`spot|fut`)
- `y_mu`, `y_sigma`, `y_model_version`
- `entry_ev_spot` or `entry_ev_fut` (해당 market)
- `live_ev`
- `p_win_derived`

## 12. Rollout

1. Phase A: `Y` 모델 인터페이스 + Spot/Futures PnL 함수 구현
2. Phase B: Shadow 계산 및 로그 적재
3. Phase C: 게이트에 soft 연동
4. Phase D: 하드 게이트/청산 정책 전환

## 13. Open Questions

1. `Y` horizon을 전략별로 다르게 둘지(예: 1m/5m/15m)
2. 선물 `c_funding`을 실시간 추정할지, 보수 고정치로 둘지
3. 청산 위험(`c_liq_risk`) 모델을 1차에서 포함할지 2차로 미룰지
