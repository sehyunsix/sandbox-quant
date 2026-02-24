# RFC 0028: EV Probability Estimation Framework

- Status: Draft
- Author: sandbox-quant
- Date: 2026-02-24
- Related:
  - `docs/rfcs/0027-ev-integration-and-exit-orchestration-for-simple-signal-runtime.md`
  - `docs/rfcs/0026-position-lifecycle-exit-expectancy-and-session-fail-safe.md`
  - `src/order_store.rs`
  - `src/order_manager.rs`

## 1. Problem

EV 계산식은 이미 정의할 수 있지만, 핵심 입력 확률(`p(win)`, 손실 tail 확률)이 불안정하면 EV 자체가 왜곡된다.

현재 시스템에서 예상되는 실패 패턴:

1. 표본 수가 적은 전략에서 승률 과대추정
2. 최근 시장 레짐 변화 반영 지연
3. 단순 평균 기반 tail risk 과소평가

## 2. Goal

EV 입력 확률을 아래 원칙으로 추정한다.

1. 소표본 안정성: 과신 방지
2. 레짐 적응성: 최근 데이터 가중
3. 설명 가능성: 계산식/근거 추적 가능

## 3. Non-Goals

- 초기 단계에서 복잡한 딥러닝 확률모델은 도입하지 않는다.
- 완전한 오프라인 리서치 파이프라인 구축은 본 RFC 범위 밖이다.

## 4. Probability Targets for EV

EV 입력으로 최소 아래 확률을 산정한다.

1. `p_win`: 진입 후 최종 실현손익이 양수일 확률
2. `p_tail_loss`: 손실이 `loss_threshold` 이하(큰 손실)로 끝날 확률
3. `p_timeout_exit`: `expected_holding_ms` 내 청산되지 않을 확률

보조 통계:

1. `avg_win`, `avg_loss`
2. `q05_loss` (손실 하위 5% quantile)
3. `median_holding_ms`

## 5. Estimation Method (v1)

### 5.1 Base: Bayesian Win Probability

승/패를 Bernoulli로 보고 Beta-Binomial posterior를 사용한다.

- Prior: `Beta(a0, b0)`
- Data: 최근 윈도우에서 `wins`, `losses`
- Posterior mean:

`p_win = (a0 + wins) / (a0 + b0 + wins + losses)`

기본 prior 제안:

- 전역 prior: 전체 전략 풀 기준 `a0=6, b0=6` (중립 0.5, 과신 방지)
- 전략별 데이터가 충분해지면 prior 영향 자동 감소

### 5.2 Recency Weighting

레짐 반영을 위해 최근 거래에 더 큰 가중치를 준다.

- 각 샘플 가중치: `w_i = exp(-lambda * age_days_i)`
- 유효 승/패 수:
  - `wins_eff = sum(w_i for win trades)`
  - `losses_eff = sum(w_i for loss trades)`

posterior 계산 시 `wins/losses` 대신 `wins_eff/losses_eff` 사용.

### 5.3 Hierarchical Shrinkage

표본 부족 시 과적합을 막기 위해 수축 적용:

`p_win_final = alpha * p_win_local + (1 - alpha) * p_win_global`

- `p_win_local`: `source_tag + instrument` posterior
- `p_win_global`: 전체 또는 동일 전략군 posterior
- `alpha = n_eff / (n_eff + k)` (`k`는 수축 강도 하이퍼파라미터)

### 5.4 Tail Loss Probability

`p_tail_loss`는 아래 두 방식 중 v1에서는 경험적 비율 + 베이지안 보정을 사용:

1. 손실 트레이드 중 `pnl <= -loss_threshold` 이벤트를 Bernoulli로 정의
2. 동일하게 Beta posterior 평균으로 추정

`p_tail_loss = (a_tail + tail_events) / (a_tail + b_tail + loss_events)`

## 6. Expected Holding Probability

`p_timeout_exit`는 단순 생존확률 근사로 계산한다.

1. 과거 holding time 분포에서 `T = expected_holding_ms`
2. `P(holding > T)`를 경험적 CDF로 추정
3. 샘플 부족 시 global 분포로 수축

## 7. EV Formula with Probability Inputs

기본 EV:

`EV = p_win * avg_win - (1 - p_win) * avg_loss - fee_slippage_penalty`

보수적 EV(권장 운영값):

`EV_conservative = EV - gamma * p_tail_loss * |q05_loss|`

`gamma`는 tail 패널티 민감도.

## 8. Confidence Scoring

확률값과 함께 신뢰도 등급을 반환한다.

입력 요소:

1. `n_eff` (유효 표본 수)
2. posterior 분산
3. 최근 캘리브레이션 오차(Brier/log-loss)

등급:

- `high`: 표본 충분 + 오차 낮음
- `medium`: 중간
- `low`: 소표본 또는 오차 높음

`low`일 때는 Hard Gate를 비활성화하거나 보수 계수를 강화한다.

## 9. Data Contract

저장 필드(개념):

1. `p_win_estimate`
2. `p_tail_loss_estimate`
3. `p_timeout_exit_estimate`
4. `prob_model_version`
5. `n_eff`
6. `confidence_level`

추가로 사후평가를 위한 필드:

1. `realized_is_win` (0/1)
2. `realized_tail_loss` (0/1)
3. `realized_holding_ms`

## 10. Runtime Algorithm (Pseudo)

```text
on_entry_intent(source_tag, instrument):
  stats_local  = load_recent_stats(source_tag, instrument)
  stats_global = load_global_stats(source_tag or all)

  p_local  = beta_posterior(stats_local.weighted_wins, stats_local.weighted_losses, a0, b0)
  p_global = beta_posterior(stats_global.weighted_wins, stats_global.weighted_losses, a0, b0)

  alpha = n_eff_local / (n_eff_local + k)
  p_win = alpha * p_local + (1 - alpha) * p_global

  p_tail_loss = beta_tail_posterior(...)
  p_timeout_exit = survival_prob(...)

  return ProbabilitySnapshot { p_win, p_tail_loss, p_timeout_exit, n_eff, confidence }
```

## 11. Calibration and Monitoring

운영 중 확률 추정 품질을 지속 모니터링한다.

1. Brier score (`p_win` vs realized win)
2. Calibration bucket (예: 0.1 단위)별 예측-실현 차이
3. 최근 7일/30일 드리프트

품질 저하 시:

1. Hard Gate 자동 완화(soft-only)
2. prior/수축 파라미터 재조정

## 12. Rollout Plan

### Phase 1: Shadow

- 확률 추정/저장만 수행, 진입 차단 없음
- 캘리브레이션 리포트 축적

### Phase 2: Soft Use

- EV 계산에 반영하되 warning-only
- 신뢰도 `low`에서는 참고값으로만 사용

### Phase 3: Hard Gate

- 특정 전략군에 한해 진입 차단 정책 적용
- 성능 모니터링 기준 미달 시 즉시 soft로 롤백

## 13. Testing Plan

테스트는 `tests/`에만 작성한다.

1. 소표본 환경에서 posterior가 과신하지 않는지 검증
2. recency weighting이 최근 데이터 방향으로 확률을 이동시키는지 검증
3. 수축계수(`alpha`)가 `n_eff` 증가와 함께 local 비중을 늘리는지 검증
4. tail 확률 산정값이 극단 손실 이벤트 증가 시 단조 증가하는지 검증
5. 캘리브레이션 지표 계산(Brier 등) 정확성 검증

## 14. Acceptance Criteria

1. EV 계산에 `p_win`, `p_tail_loss`, `p_timeout_exit`가 구조적으로 포함됨
2. 확률 추정마다 `n_eff`와 `confidence`가 함께 기록됨
3. Shadow 기간 캘리브레이션 리포트 생성 가능
4. 확률모델 버전별 성능 비교가 가능함

## 15. Open Questions

1. prior 초기값(`a0,b0`)을 전역 고정으로 둘지, 전략군별로 다르게 둘지?
2. recency decay `lambda`를 시간 기반 고정값으로 둘지, 변동성 기반으로 동적화할지?
3. hard gate 기준을 `EV_conservative <= 0` 하나로 둘지, `p_tail_loss` 임계치와 결합할지?
