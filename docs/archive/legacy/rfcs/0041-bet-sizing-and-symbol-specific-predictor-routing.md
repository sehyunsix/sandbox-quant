# RFC 0041: Bet Size 정의와 Symbol별 Predictor 라우팅 정책

- Status: Draft
- Author: sandbox-quant
- Date: 2026-02-27
- Related:
  - `docs/rfcs/0040-predictor-abstraction-and-per-predictor-horizon.md`
  - `docs/rfcs/0038-y-price-model-split-spot-futures-ev.md`
  - `docs/rfcs/0039-current-buy-to-exit-runtime-cycle-with-ymodel.md`

## 1. 문제 정의

현재 구조에서 남은 핵심 질문은 두 가지다.

1. `bet size`(주문 수량)를 어떤 원칙/수식으로 결정할 것인가?
2. predictor를 symbol별로 분리해야 하는가, 아니면 공유 predictor로 충분한가?

둘은 연결되어 있다. 예측 불확실성이 큰 symbol은 bet size를 줄여야 하고, 특정 symbol에서 예측 편향이 지속되면 predictor 라우팅을 분리해야 한다.

## 2. 목표

1. EV 기반 주문에서 재현 가능한 `bet size` 공식을 정의한다.
2. predictor 라우팅을 `global -> strategy -> symbol` 우선순위로 설정 가능하게 한다.
3. 데이터 부족/과적합 리스크를 줄이기 위한 분리 기준을 제공한다.

## 3. Bet Size 원칙

Bet size는 아래 4개 요소의 곱으로 정의한다.

`qty = base_notional * f_ev * f_conf * f_risk / price`

- `base_notional`: 계정/전략에 할당된 기본 위험 예산(USDT)
- `f_ev`: EV 강도 스케일
- `f_conf`: predictor 신뢰도 스케일
- `f_risk`: 심볼/포트폴리오 리스크 스케일

최종 수량은 거래소 필터(`stepSize`, `minQty`, `minNotional`) 적용 후 정규화한다.

## 4. Bet Size 상세 수식 (초안)

### 4.1 EV 스케일

`f_ev = clamp(EV / ev_ref, 0, f_ev_max)`

- `ev_ref`: 1x 크기의 기준 EV (예: 0.25 USDT)
- EV<=0이면 주문 차단(또는 `qty=0`)

### 4.2 Confidence 스케일

`f_conf = clamp(confidence, f_conf_min, 1.0)`

- confidence는 predictor가 제공(예: `1 / (1 + EV_std / scale)`)
- low confidence일수록 size 축소

### 4.3 리스크 스케일

`f_risk = min(f_symbol_cap, f_portfolio_cap, f_drawdown_cap)`

- `f_symbol_cap`: symbol 최대 노출 제한
- `f_portfolio_cap`: 총 노출/레버리지 제한
- `f_drawdown_cap`: 최근 손실 구간에서 자동 감쇠

## 5. 실무 운영 규칙

1. Hard floor:
- `notional < 1 USDT`는 기본 숨김 대상이며, 주문도 기본 차단 권장

2. Hard ceiling:
- symbol/strategy별 최대 notional 비율 설정 필수

3. Cooldown modulation:
- 동일 symbol 연속 손실 시 `base_notional` 자동 감쇠

4. Venue 분리:
- Spot/Futures는 비용/레버리지 구조가 다르므로 `ev_ref`, cap을 별도 운영

## 6. Predictor를 Symbol별로 다르게 둘지?

결론: "항상 분리"가 아니라 "기본 공유 + 조건부 분리"가 권장이다.

### 6.1 기본 정책 (권장)

- predictor 구현체(kind)는 공유한다. (운영 복잡도/데이터 효율성)
- predictor 상태(state)는 최소 `instrument` 단위로 분리한다.
- 필요 시 `symbol override`로 predictor id/horizon을 바꾼다.

즉:
- 모델 코드 공유
- 추정 상태는 symbol별 독립
- 라우팅만 정책으로 선택

### 6.2 Symbol 분리 기준 (Trigger)

아래 조건 중 1개 이상 만족 시 symbol override 검토:

1. Calibration 실패:
- 해당 symbol에서 예측구간 커버리지/오차가 지속적으로 악화

2. 구조적 미스매치:
- 변동성/미세구조가 다른 군집(예: BTC vs 알트 저유동성)

3. 비용 구조 차이:
- funding/슬리피지 체계가 달라 EV 오차가 누적

4. horizon mismatch:
- 동일 전략이라도 symbol별 반응 속도가 명확히 다름

## 7. 라우팅 우선순위

predictor 선택 우선순위:

1. `strategy.symbol_overrides[{symbol}]`
2. `strategy.predictor`
3. `predictors.default`

horizon 선택 우선순위:

1. `strategy.symbol_overrides[{symbol}].horizon`
2. `strategy.horizon`
3. `predictor.default_horizon`
4. 시스템 기본(`1m`)

## 8. 설정 예시

```toml
[predictors]
default = "ewma-v1"

[predictors.models.ewma-v1]
kind = "ewma"
default_horizon = "1m"

[predictors.models.ewma-fast]
kind = "ewma"
default_horizon = "1s"

[[strategies]]
id = "trend-a"
predictor = "ewma-v1"
horizon = "1m"
base_notional = 25.0
ev_ref = 0.25
f_ev_max = 2.0
symbol_max_notional_ratio = 0.10

[strategies.symbol_overrides.BTCUSDT]
predictor = "ewma-fast"
horizon = "1s"
base_notional = 40.0

[strategies.symbol_overrides.XRPUSDT]
predictor = "ewma-v1"
horizon = "1m"
base_notional = 10.0
```

## 9. 로그/관측 지표

주문 직전 반드시 기록:

- `predictor_id`, `horizon`, `symbol`
- `ev`, `confidence`
- `base_notional`, `f_ev`, `f_conf`, `f_risk`
- `qty_raw`, `qty_norm`, `blocked_reason`

지표:
- symbol별 EV decile vs realized PnL
- symbol별 calibration error
- size bucket별 Sharpe/PnL/DD

## 10. 롤아웃 계획

Phase 1:
- bet size 계산을 함수화하고 로그만 추가 (shadow)

Phase 2:
- 실제 주문 사이징에 반영, 단 cap 보수적으로 적용

Phase 3:
- symbol override 활성화 + 주간 calibration 리포트 기반 조정

## 11. 수용 기준

1. bet size 산식이 코드/로그에 일관되게 드러난다.
2. predictor 라우팅이 symbol별 override를 지원한다.
3. override 미사용 시 기존 동작(전략 기본 predictor)과 호환된다.
4. 주문 거절/오버사이징 비율이 운영 기준 이내로 유지된다.

## 12. Open Questions

1. `f_ev`를 선형 대신 비선형(`sqrt`, sigmoid)으로 둘지?
2. Kelly fraction을 부분 도입할지, 현재 cap 기반 접근을 유지할지?
3. symbol override를 수동 설정만 허용할지, 자동 승격(autopromotion)까지 허용할지?

