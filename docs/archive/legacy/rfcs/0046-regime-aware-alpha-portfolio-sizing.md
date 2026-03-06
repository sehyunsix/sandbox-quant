# RFC 0046: Regime-Aware Alpha and Portfolio Sizing

- Status: Proposed
- Author: Codex (GPT-5) + project maintainer
- Date: 2026-03-04
- Related: 0040, 0041, 0045

## 1. Background

현재은 레짐(상승/하락/횡보)을 구분하지 않고 동일한 alpha 출력을 포트폴리오에 전달하고 있다.
그 결과 상승 추세에서 쓰던 시그널이 횡보 구간에서도 같은 무게로 동작하면서
거래 횟수 증가, 수수료 비용 상승, 손익 변동성 악화가 반복적으로 발생한다.

또한 현재 운영은 `Alpha -> Portfolio`로 단순 연결되어 있어,
레짐별 신뢰도/신뢰구간을 반영한 사전 게이팅이 부족하다.

## 2. Problem Statement

- 레짐 전환 국면(추세 시작/종료, 박스권 전환)에서 예측 오류가 증가한다.
- 횡보 구간에서도 추세 전제형 판단이 살아 있어 과매매가 발생한다.
- 횡보에서의 소음이 진입 판단에 그대로 반영돼 실현 손익이 침식된다.
- 레짐별 성능이 섞여서 집계되므로 문제 구간 탐지가 어렵다.

## 3. Goals

1. 시장 레짐 분류를 실시간 상태로 추가한다.
2. Portfolio 레이어에서 레짐/신뢰도 기반으로 포지션 크기와 진입 여부를 제어한다.
3. 횡보 구간에서 `size`와 빈도를 동적으로 축소한다.
4. 레짐별 성능 지표를 분리 수집하여 판단 근거를 명확히 만든다.

## 4. Non-goals

1. 이 RFC는 새로운 예측 모델 자체를 재학습하는 것을 목표로 하지 않는다.
2. 거래소/브로커 API 적응 또는 주문 타입 확장을 다루지 않는다.
3. 전략/시그널 계산 파이프라인 전면 교체를 요구하지 않는다.

## 5. Regime Model

### 5.1 입력 지표

- `ema_fast`, `ema_slow`
- `ema_slope` (최근 N개 값의 기울기)
- `adx`
- `bb_width` (Bollinger 폭)
- `atr_pct`

### 5.2 레짐 클래스

`trend_up`, `trend_down`, `range` 3클래스 분류를 사용한다.

예시 규칙:

- `trend_up`
  - `ema_fast > ema_slow`
  - `ema_slope > 0`
  - `adx >= 20`
- `trend_down`
  - `ema_fast < ema_slow`
  - `ema_slope < 0`
  - `adx >= 20`
- `range`
  - `adx < 20` 또는 `bb_width`가 중간 이하 구간

### 5.3 안정성 조치

- 레짐 전환은 1회 시점 즉시 반영하지 않고, 직전 레짐 유지 시간을 반영한 `hysteresis`를 둔다.
- 레짐 신뢰도(`regime_confidence`)를 산출해 0~1로 관리한다.

## 6. Integration with Portfolio Layer

### 6.1 의사결정 입력 변경

Portfolio 의사결정 입력에 다음 값을 추가한다.

- `alpha_signal` (현재 스코어, 방향, confidence)
- `symbol_regime`
- `regime_confidence`
- `regime_metrics`(adx, atr_pct, bb_width 등 최소 스냅샷)

### 6.2 사이징 룰

레짐별 기본 가중치:

- `trend_up`: `1.0 ~ 1.2`
- `trend_down`: `1.0 ~ 1.2` (전략 성격에 맞춰 조정)
- `range`: `0.2 ~ 0.5`

공통 가드:

- `alpha_confidence < threshold` 이면 `target_delta = 0` 또는 최소화
- `regime_confidence < threshold` 이면 축소 모드로 처리
- 거래비용/예상 슬리피지 반영 후 기대이익 < 비용이면 진입 보류
- 최근 N회 연속 손실 구간에서는 자동 conservative 모드 적용

## 7. Observability and UI

1. 포트폴리오 패널에 레짐 라벨(현재/신뢰도) 표시.
2. `레짐별 realized pnl`, `win rate`, `trade count`, `turnover` 집계 표시.
3. 체결 지연/동기화 지연과 함께 레짐 전환 빈도 추적.

## 8. Migration Plan

### Phase 1: Regime Detector

- 런타임 지표에서 레짐 상태 계산 모듈 추가
- App/portfolio 상태에 `RegimeState` 보관
- 이벤트(`RegimeUpdate`) 발행

### Phase 2: Portfolio Gate

- 기존 alpha 기반 sizing 경로 앞단에 레짐 게이팅 적용
- 레짐별 multiplier와 최소 진입 임계치 적용

### Phase 3: KPI 분리 및 대시보드

- 레짐별 집계 스토리지 추가
- UI에 레짐별 성능 지표 탭/라인 추가

### Phase 4: 점진 적용

- 기존 경로를 보조 지표로 병행 운영
- 2주 이상 A/B 비교 후 임계치 조정
- 성공 조건 충족 시 기본 값으로 전환

## 9. Acceptance Criteria

1. 2주 이상 동시 운영에서 총 손익 역행 없이 레짐별 변동성 개선이 확인되어야 한다.
2. 횡보 구간에서 거래 빈도/턴오버가 감소해야 한다.
3. 레짐 전환 구간에서 잘못된 추세 진입 비율이 감소해야 한다.
4. 모든 의사결정은 `alpha + regime + risk` 값을 사용해 추적 가능해야 한다.
5. 레짐 신뢰도 낮은 구간에서 보수 모드가 적용되어야 한다.

## 10. Rollback

`regime_gate_enabled=false` 설정 시 기존 동작으로 즉시 복귀한다.
또한 레짐 multiplier를 `1.0`, 보수 모드 임계치 `0`으로 설정해 영향도를 최소화한다.

## 11. Open Questions

1. 레짐 분류 임계치(특히 ADX, bb_width)는 자산별 고정값을 유지할지 적응형을 적용할지.
2. 레짐 신뢰도를 단일 지표로 만들지, ADX/ATR/BB의 가중합으로 만들지.
3. 레짐별 로그/대시보드에 전략 attribution까지 남길지.

## 12. Implementation Workplan

### 12.1 Phase A: Foundation (1차, 최소 변경)

1. `src/model/regime.rs` (신규)
   - `MarketRegime`, `RegimeSignal`, `RegimeConfig` 타입 추가
   - `calculate_regime(...) -> RegimeSignal` 구현
   - 하스테리시스(상태 전이 최소 유지 시간) 옵션 반영

2. `src/model/risk.rs` 또는 기존 risk 모듈
   - 레짐 신뢰도 점수 구조체 필드 추가
   - `PortfolioStateSnapshot` 또는 `portfolio` projection에 `regime`, `regime_confidence` 확장

3. `src/event.rs`
   - `AppEvent::RegimeUpdate` 신규 이벤트 추가
   - `portfolio` 상태 갱신 경로에서 레짐 반영 가능하도록 정의

### 12.2 Phase B: Runtime Integration (2차, 핵심 기능)

1. `src/main.rs`
   - 캔들/틱 갱신 시점에 `calculate_regime` 호출
   - 레짐 계산 결과를 `AppEvent::RegimeUpdate`로 전송
   - 기존 alpha 실행 경로에서 `regime` 전달값 wiring

2. `src/ui/mod.rs`
   - `AppState`에 레짐 상태 필드 추가
   - `apply(AppEvent::RegimeUpdate)` 핸들러 추가
   - 포트폴리오 패널에 `regime` + `confidence` 렌더링 추가

3. `src/runtime/portfolio_sync.rs` 또는 의사결정 모듈
   - decision 함수에 `regime` 입력 인자 추가
   - `size_multiplier_by_regime` 적용
   - `alpha_confidence` 및 `regime_confidence` 기반 최소 진입 게이트 적용

### 12.3 Phase C: KPI 분리 (2주 운영용)

1. `src/order_store.rs`
   - 레짐별 집계 컬럼/쿼리 추가(또는 기존 집계 로직 확장)
   - `regime` scope별 realized pnl/승률/거래수 집계 저장

2. `src/ui/mod.rs` (Portfolio/History 패널)
   - 레짐별 KPI 표시 라인 추가
   - 횡보/추세 구간별 거래 건수, 손익, 승률 표시

3. `src/predictor`/로그 계층
   - 레짐별로 `alpha` 로그 태깅 강화
   - 디버그 분석시 원인 추적 가능한 형태로 필드 확장

### 12.4 Phase D: Safety and Rollback

1. `config`에 feature flag 추가
   - `regime_gate_enabled`
   - `regime_multiplier_up/down/range`
   - `regime_confidence_min`, `alpha_confidence_min`

2. `src/runtime` 경로에서 멀티 경로 동작
   - flag가 off면 기존 가중치=1.0, 기존 의사결정 그대로
   - flag on이면 레짐 게이트 동작
   - 긴급 시 플래그 하나로 즉시 rollback 가능

## 13. Delivery Checklist

1. [ ] 레짐 계산이 매 tick 또는 캔들 종료 시 안정적으로 계산되는지
2. [ ] Portfolio 패널에 현재 레짐이 표시되는지
3. [ ] 레짐이 `trend_down/trend_up/range`별로 사이징 multiplier를 반영하는지
4. [ ] confidence 미달 시 진입 보류가 동작하는지
5. [ ] 레짐별 KPI가 기존 realized pnl과 분리되어 기록되는지
6. [ ] 플래그 off 상태에서 기존 동작과 동일 결과가 재현되는지
