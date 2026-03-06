# RFC 0042: Online Predictor Evaluation과 Live Quality Metrics

- Status: Draft
- Author: sandbox-quant
- Date: 2026-02-27
- Related:
  - `docs/rfcs/0040-predictor-abstraction-and-per-predictor-horizon.md`
  - `docs/rfcs/0041-bet-sizing-and-symbol-specific-predictor-routing.md`
  - `docs/rfcs/0038-y-price-model-split-spot-futures-ev.md`

## 1. 문제 정의

과거 백테스트 점수만으로 predictor 품질을 판단하면 현재 레짐 변화(regime shift)를 반영하기 어렵다.
운영에서는 predictor 정확도를 실시간으로 추적하고, 악화 시 즉시 size/gate/predictor routing에 반영해야 한다.

## 2. 목표

1. 백테스트 대신 온라인(실시간) 품질평가 체계를 정의한다.
2. predictor별/심볼별/호라이즌별 정확도를 지속적으로 수치화한다.
3. 품질 저하를 자동으로 감지하고 대응(축소/차단/전환) 가능하게 한다.

## 3. 평가 단위

평가 키:
- `(predictor_id, symbol, horizon, market, strategy_tag)`

기본 집계는 아래 3단계로 제공:
1. Global predictor level
2. Symbol level
3. Strategy+symbol level

## 4. Online Label 정의

예측 시점 `t0`에서 horizon `H` 예측을 만들면, 레이블은 `t1=t0+H`에서 확정한다.

- 예측값: `Y_hat` (예: `mu`, `sigma`)
- 실제값: `Y_real = log(P(t1)/P(t0))`

부분 체결/미체결과 무관하게, 예측 품질 평가는 시장값 기준으로 수행한다.
(실행 품질은 별도 execution metric으로 분리)

## 5. 핵심 실시간 지표 (권장)

### 5.1 Point Forecast Error

1. `MAE_y = E[|Y_real - mu|]`
2. `RMSE_y = sqrt(E[(Y_real - mu)^2])`
3. `R² = 1 - SSE/SST`
 - `SSE = Σ(Y_real - mu)^2`
 - `SST = Σ(Y_real - mean(Y_real))^2`

용도:
- 평균 오차 크기 추적
- drift 탐지의 기본 신호
- 기준선(평균 예측) 대비 설명력 확인

주의:
- `SST`가 매우 작을 때(횡보/저변동 구간) `R²` 해석이 불안정할 수 있어
  `SST < eps` 구간은 `R²=N/A` 처리 권장

### 5.2 Direction Accuracy

1. `HitRate = P(sign(mu) == sign(Y_real))`
2. `BalancedHitRate` (상승/하락 클래스 불균형 보정)

용도:
- Buy/Sell 방향성 품질 확인

### 5.3 Probabilistic Calibration

분포 예측 품질 핵심:

1. `PIT`(Probability Integral Transform) 균일성 점검
2. `Coverage@kσ`:
- 예: `|Y_real-mu| <= 1*sigma` 비율이 이론치(약 68%)와 가까운지
3. `NLL`(Negative Log-Likelihood)
4. `CRPS`(Continuous Ranked Probability Score)

용도:
- sigma 과소/과대추정 식별
- 확률 예측 신뢰도 평가

### 5.4 EV Quality Metrics

1. `EV_sign_accuracy = P(sign(EV_pred) == sign(PnL_realized_proxy))`
2. `EV_calibration_curve`:
- EV decile별 실제 평균 PnL
3. `EV_bias = E[PnL_realized_proxy - EV_pred]`

용도:
- EV가 실제 기대손익을 과대/과소평가하는지 추적

## 6. 실시간 집계 방식

고정 윈도우 + EWMA 병행을 권장:

1. Rolling window:
- 최근 `N=200` 예측

2. EWMA score:
- 최근 데이터 가중(레짐 변화 민감)

둘을 함께 저장:
- `metric_window`
- `metric_ewma`

## 7. Online Quality Score (단일 스코어)

운영 단순화를 위해 composite score를 둔다.

`Q = w1*calib + w2*direction + w3*ev_quality - w4*uncertainty_penalty`

예:
- `calib`: coverage 오차, NLL 기반 정규화 점수
- `direction`: balanced hit rate
- `ev_quality`: EV bias/decile monotonicity
- `uncertainty_penalty`: sigma 불안정성, sample 부족 패널티

`Q`는 0~100 스케일로 정규화하여 UI에 표시한다.

## 8. 알람/자동 대응 정책

### 8.1 상태 등급

- `HEALTHY`: `Q >= 70`
- `DEGRADED`: `50 <= Q < 70`
- `UNHEALTHY`: `Q < 50`

### 8.2 자동 액션

1. `DEGRADED`:
- bet size 계수 자동 감쇠 (`f_conf` 하향)

2. `UNHEALTHY`:
- 해당 predictor+symbol 조합 신규 진입 차단(Shadow only 옵션 지원)
- fallback predictor로 라우팅 전환 가능

3. 복구:
- 연속 `M`개 윈도우에서 임계치 회복 시 자동 해제

## 9. UI 표시 제안

Positions/Strategies 표에 간단 지표 추가:

- `PredQ` (0~100)
- `Hit%`
- `R²`
- `Calib` (`OK/WARN/BAD`)
- `NLLΔ` (기준 대비 증감)

공간 제약 시 축약:
- 기본 grid: `PredQ`, `Hit%`, `R²`
- 상세 패널: coverage/NLL/CRPS/EV bias

## 10. 로그/저장 계약

이벤트 추가(예시):
- `predict.eval.registered` (예측 등록)
- `predict.eval.resolved` (label 확정)
- `predict.eval.metrics` (집계값 업데이트)
- `predict.eval.alert` (등급 전환)

저장 필드:
- `pred_id`, `symbol`, `horizon`, `t0`, `t1`
- `mu`, `sigma`, `y_real`
- `mae`, `rmse`, `r2`, `nll`, `crps`, `hit`
- `ev_pred`, `pnl_proxy`, `ev_bias`
- `quality_score`, `health_state`

## 11. 구현 단계

Phase 1:
- 예측/레이블 매칭 파이프라인 구축
- 핵심 metric(MAE, HitRate, Coverage, NLL) 실시간 계산

Phase 2:
- composite `Q` 산출 + UI 표시
- 경고 알람만 활성화

Phase 3:
- 자동 대응(size 감쇠, fallback routing) 활성화

## 12. 수용 기준

1. predictor별 품질 점수가 실시간 갱신된다.
2. symbol/horizon 단위로 품질 열람이 가능하다.
3. 품질 악화 시 경고/자동대응이 정책대로 동작한다.
4. 백테스트 없이도 운영 중 모델 열화 감지가 가능하다.
5. `R²`가 rolling window 기준으로 실시간 표시된다(`SST<eps`는 `N/A`).

## 13. Open Questions

1. `Q` 가중치(`w1..w4`)를 고정할지, 전략군별로 다르게 둘지?
2. `pnl_proxy`를 mark-to-market으로 통일할지, 체결 기반 realized로 보정할지?
3. 자동 fallback 전환을 완전자동으로 할지, 승인형(semi-auto)으로 할지?
