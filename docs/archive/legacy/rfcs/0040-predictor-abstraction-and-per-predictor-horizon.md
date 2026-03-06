# RFC 0040: Predictor 추상화와 Predictor별 Horizon(1s/1m/1h) 설정

- Status: Draft
- Author: sandbox-quant
- Date: 2026-02-27
- Related:
  - `docs/rfcs/0038-y-price-model-split-spot-futures-ev.md`
  - `docs/rfcs/0039-current-buy-to-exit-runtime-cycle-with-ymodel.md`
  - `src/ev/y_model.rs`

## 1. 문제 정의

현재 런타임은 사실상 `EwmaYModel` 단일 구현에 결합되어 있다.

- EV 계산이 특정 모델 구현(`EWMA`)에 직접 종속됨
- 예측 horizon(예: 1s/1m/1h)이 모델/전략별로 분리되지 않음
- 전략별 신호 특성(초단타/스윙)에 맞는 예측 시간을 독립적으로 선택하기 어려움

결과적으로, 전략이 늘어날수록 동일한 horizon/동일한 모델을 강제하는 구조적 제약이 생긴다.

## 2. 목표

1. Predictor를 trait 기반으로 추상화해 다중 모델을 플러그인처럼 교체 가능하게 한다.
2. predictor별 기본 horizon을 설정하고, 필요하면 strategy 단에서 override 가능하게 한다.
3. EV 계산 입력(`YNormal` 또는 향후 확장 분포)이 predictor+horizon 단위로 일관되게 공급되게 한다.
4. 기존 EWMA 경로와의 하위호환(점진 마이그레이션)을 보장한다.

## 3. 비목표

- 본 RFC는 새로운 알파 전략을 추가하지 않는다.
- 본 RFC는 오프라인 학습 파이프라인 전체(MLOps)를 정의하지 않는다.
- 본 RFC는 즉시 하드 게이트 정책 변경을 강제하지 않는다.

## 4. 제안 아키텍처

### 4.1 Predictor 인터페이스

```rust
pub enum Horizon {
    Sec(u64),
    Min(u64),
    Hour(u64),
}

pub struct PredictContext<'a> {
    pub instrument: &'a str,
    pub market: MarketKind,         // Spot | Futures
    pub source_tag: &'a str,        // strategy id/tag
    pub signal: Signal,             // Buy/Sell/Hold
    pub horizon: Horizon,
    pub now_ms: u64,
}

pub trait Predictor: Send + Sync {
    fn id(&self) -> &'static str;
    fn observe_trade(&mut self, instrument: &str, price: f64, ts_ms: u64);
    fn predict_y(&self, ctx: &PredictContext<'_>) -> Option<YDistribution>;
}
```

- `YDistribution`은 1차로 `YNormal(mu,sigma)`를 포함한다.
- 향후 t-distribution/mixture 확장을 고려해 enum 형태로 둔다.

### 4.2 Predictor Registry

- 런타임이 predictor 인스턴스를 등록/조회하는 registry를 둔다.
- 키: `predictor_id`
- 값: predictor 구현체 + 메타(config, health)

예:
- `ewma-v1`
- `regime-ewma-v1`
- `garch-lite-v1` (future)

### 4.3 Horizon Resolver

horizon 결정은 아래 우선순위를 따른다.

1. `strategy.predictor.horizon` (전략별 명시)
2. `predictor.default_horizon` (모델 기본값)
3. 시스템 기본값 (`1m`)

해당 규칙으로 `PredictContext.horizon`을 생성하고 `predict_y`에 전달한다.

## 5. 설정 스키마 제안

```toml
[predictors]
default = "ewma-v1"

[predictors.models.ewma-v1]
kind = "ewma"
default_horizon = "1m"
alpha_mean = 0.08
alpha_var = 0.08
min_sigma = 0.001

[predictors.models.ewma-fast]
kind = "ewma"
default_horizon = "1s"
alpha_mean = 0.25
alpha_var = 0.20
min_sigma = 0.001

[[strategies]]
id = "c13"
predictor = "ewma-fast"
horizon = "1s"

[[strategies]]
id = "slw"
predictor = "ewma-v1"
horizon = "1h"
```

검증 규칙:
- `horizon`은 `1s/1m/1h` 형식 포함 기존 parser 규칙을 재사용
- strategy에 지정한 predictor id가 registry에 없으면 부팅 실패

## 6. 런타임 흐름 변경

1. Tick 수신 시 모든 활성 predictor에 `observe_trade` 전파
2. 전략 시그널 발생 시:
- strategy가 지정한 predictor 선택
- horizon resolver로 최종 horizon 결정
- `predict_y(ctx)` 호출
3. 예측 결과를 Spot/Futures EV 함수에 전달
4. `LiveEV`, `EntryEV` 이벤트에 `predictor_id`, `horizon` 메타를 함께 기록

## 7. 데이터/로그 계약 변경

추가 저장 필드(권장):
- `predictor_id`
- `predict_horizon`
- `y_model_kind`
- `y_mu`, `y_sigma` (분포별 파생값)

로그 예:
- `ev.predict.start symbol=BTCUSDT predictor=ewma-fast horizon=1s`
- `ev.predict.ok symbol=BTCUSDT mu=... sigma=...`
- `ev.predict.fallback reason=no_distribution`

## 8. UI 반영 제안

Position row 또는 상세뷰에 아래를 표시:
- `Pred`: `ewma-fast@1s` 형태
- `LiveEV`, `EntryEV`는 기존 유지

가로폭 문제를 고려해:
- 기본 grid는 축약값(`ewf@1s`) 사용
- 상세 패널에서 full predictor id 표시

## 9. 마이그레이션 단계

Phase 1:
- `EwmaYModel`을 `Predictor` trait 구현체로 래핑
- 기존 코드 경로를 `predictor_id=ewma-v1`, `horizon=1m`으로 매핑

Phase 2:
- 전략별 predictor/horizon 설정 활성화
- shadow 로그로 predictor별 EV 분포 비교

Phase 3:
- 운영 정책(게이트/청산)에서 predictor+horizon 메타를 의사결정/분석에 활용

## 10. 리스크와 완화

1. 리스크: horizon이 짧을수록 노이즈 과적합
- 완화: predictor별 최소 샘플 수, shrinkage, sigma floor

2. 리스크: horizon이 길수록 반응 지연
- 완화: 전략별 override 허용, 다중 horizon A/B 로그 비교

3. 리스크: predictor 다중화로 운영 복잡도 증가
- 완화: registry health metric, fallback predictor 정책(`default`)

## 11. 수용 기준

1. strategy별로 predictor/horizon 조합을 독립 설정할 수 있다.
2. EV 계산/저장/로그에 `predictor_id`, `horizon`이 누락 없이 남는다.
3. 기존 EWMA 단일 모델 설정만 있는 경우 동작이 깨지지 않는다.
4. UI에서 현재 포지션의 예측 기준(predictor+horizon)을 확인할 수 있다.

## 12. Open Questions

1. 하나의 전략에 다중 horizon predictor를 병렬 사용(ensemble)할지?
2. `Buy/Hold/Sell` 별로 predictor를 분리할지, 동일 predictor에서 condition만 둘지?
3. futures funding/liq risk를 horizon 길이에 따라 어떻게 정규화할지?

