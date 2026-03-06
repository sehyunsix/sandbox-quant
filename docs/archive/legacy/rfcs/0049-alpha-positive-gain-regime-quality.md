# RFC 0049: Alpha-to-Positive-PnL Strategy Blueprint

## 1) 문제 요약

- 현재 전략은 `alpha` 신호를 **regime-aware하게 가공만** 하고, predictor 선택에서 비용·품질 필터가 약하다.
- 결과적으로 평균적으로 미세한 신호를 자주 실행하며 수수료와 슬리피지로 누적 손실이 크게 발생한다.
- 초기 백테스트 실행에서 fold 단위로 보면, 신호가 적더라도 실현손실 비중이 높아 장기적으로 `PnL`이 음수로 수렴하는 패턴이 반복된다.

## 2) 기본 가설

1. 시장 레짐별로 predictor 신호의 방향 정합성이 다르다.
2. 레짐별 predictor 품질이 낮은 모델을 동일 비중으로 고르면 샤프가 하락한다.
3. 기대수익이 수수료/슬리피지 버퍼를 못 넘는 신호는 실거행을 하지 않아야 한다.
4. 과도한 리밸런싱은 미달성 alpha만 확대해 평균 비용을 키운다.

## 3) 개선 설계 (이행 우선순위)

### 3.1 Walk-forward 레짐별 모델 품질 학습
- 각 fold의 train 구간에서 predictor별 `regime(trend_up/range/trend_down)`의 정답률/오차를 추정.
- test 구간에서 모델 선택 시 `가중치 = direction_hit_rate(충분 샘플)`을 사용.
- 품질 점수가 낮은 모델은 `selection score`에서 자동으로 축소.
- 신호 선택은 여전히 `PortfolioDecision`로 통과되어 기존 레이어를 깨지 않음.

### 3.2 비용 우선 게이트
- 진입 전 `expected_return >= fee + slippage`인지 확인(현재 backtest는 비용 반영 임계값을 포함).
- 수수료/슬리피지를 상수로 고정하지 말고, 폴드/심볼별로 보수적으로 캘리브레이션.

### 3.3 회전율 제어
- `min_hold_bars` 또는 `min_reentry_gap_ms`를 추가해 연속적 신호 반전/과도 매매를 억제.
- `min_delta`와 `order_amount cap`을 조정해 아주 작은 비중 재조정을 줄임.

### 3.4 검증 계획
1. Baseline (현재 방식)
2. Baseline + quality gating (레짐별 hit-rate 적용)
3. 2 + 비용 게이트 상향
4. 3 + turnover cap

모든 조합을 동일 데이터셋에서 비교하며 아래 지표를 추적:
- 실현 pnl, realized fee, win_rate, maxDD, turnover,
- 모델별/레짐별 실현 pnl 분해,
- `reason` 로그에 `quality` 태그 존재 여부.

## 4) 수용 기준

- 동일 시장/샘플에서 baseline 대비:
  - 실현 pnl이 양호 구간에서 일관되게 개선되거나,
  - 최소한 비용 손실 악화를 방지(총 fee/turnover 하락).
- `quality` 태그가 order ledger에 지속적으로 기록되어 실험 재현 가능.
- 실시간 배포 전, walk-forward에서 샘플 외 구간에서도 성능 역전 위험(과적합)을 낮춰야 함.
