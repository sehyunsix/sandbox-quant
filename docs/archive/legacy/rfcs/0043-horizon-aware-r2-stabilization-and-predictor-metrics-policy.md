# RFC 0043: Horizon-Aware R² 안정화와 Predictor Metric 정책

- Status: Draft
- Author: sandbox-quant
- Date: 2026-02-27
- Related:
  - `docs/rfcs/0042-online-predictor-evaluation-and-live-quality-metrics.md`
  - `docs/rfcs/0040-predictor-abstraction-and-per-predictor-horizon.md`

## 1. 문제 정의

현재 Predictor 패널에서 아래 현상이 동시에 관찰된다.

1. `N=30`이어도 `R²`가 음수(`-0.06`, `-0.08`)로 표시됨
2. `1s` horizon은 `R²=+0.000`, `MAE=0.00000`처럼 과도하게 0 근처로 수렴
3. `1h`는 표본 부족으로 `WARMUP`이 길게 유지됨

이 자체가 계산 버그는 아닐 수 있으나, 운영 관점에서는 해석이 어렵고 오해를 유발한다.

## 2. 원인 가설

### 2.1 음수 R² (`N=30`)

- `R² = 1 - SSE/SST`에서 `SSE > SST`이면 음수는 정상적으로 발생한다.
- 즉, 모델이 단순 평균 예측보다 못할 때 음수가 된다.
- 문제는 “정상 계산”과 “운영 해석 가능성”이 분리돼 있다는 점이다.

### 2.2 1s의 0 근처 메트릭

- 초단기 수익률 분산(`SST`)이 매우 작아 수치가 0 근처로 압축된다.
- 표시 자릿수가 제한되어 `MAE 0.00000`처럼 보일 수 있다.
- 실제로는 0이 아니라 매우 작은 값일 가능성이 높다.

### 2.3 1h warmup 지연

- 동일 window 크기(30)를 horizon별로 동일 적용하면 1h는 최소 30시간 필요하다.
- horizon별 샘플 생성 속도 차이를 정책에서 반영하지 못하고 있다.

## 3. 목표

1. horizon별 metric을 해석 가능하게 만든다.
2. 음수 R²를 숨기지 않되, 오해되지 않도록 라벨링한다.
3. 1s/1m/1h의 warmup 정책을 분리한다.

## 4. 제안: Horizon-Aware Metric Policy

### 4.1 지표 체계 분리

패널의 핵심 지표를 아래처럼 분리 표시한다.

1. `R²_raw` (기존 값, 음수 허용)
2. `R²_clamped = max(R²_raw, 0.0)` (운영 요약용)
3. `Skill = 1 - SSE/SST_baseline` 또는 `MASE` 계열 대체 지표

기본 grid:
- `R²c`(clamped), `Hit%`, `MAE`, `N`, `State`

상세/툴팁:
- `R²_raw`, `SSE`, `SST`, `Var(y)` 노출

### 4.2 Horizon별 최소 샘플 분리

기존:
- `R² min samples = 30` (공통)

제안:
- `1s`: 120
- `1m`: 60
- `1h`: 24 (또는 12, 운영 정책 선택)

의도:
- horizon별 정보량/수렴 속도 차이를 반영

### 4.3 저분산 구간 처리

`SST < eps_h`일 때:
- `R²_raw` 계산은 유지하되 상태를 `LOW_VAR`로 표기
- `MAE`는 과학적 표기(`2.3e-5`) 옵션 제공
- `Hit%` 단독 판단 금지(보조 지표로만 사용)

### 4.4 상태 라벨 표준화

- `WARMUP`: 표본 부족
- `LOW_VAR`: 분산 부족
- `WEAK`: `R²_raw < 0`
- `OK`: 기준 통과

## 5. 수식/통계 검토

### 5.1 음수 R²의 해석

음수 R²는 수학적으로 정상이므로 제거 대상이 아니다.
다만 운영 UI에서는 “고장”으로 오해되므로 상태 라벨(`WEAK`)과 함께 제시해야 한다.

### 5.2 horizon 정규화 필요성

`Y_h = log(P_{t+h}/P_t)` 분산은 h에 의존한다.
따라서 horizon 혼합 비교에는 아래 중 하나가 필요하다.

1. horizon별 독립 순위 비교(권장)
2. `Y_h / sqrt(h)` 정규화 후 공통 비교(보조)

본 RFC는 1번을 기본 정책으로 채택한다.

## 6. UI 제안

Predictor grid 컬럼:
- `Symbol, Market, Predictor, Horizon, State, R²c, Hit%, MAE, N`

상세 패널:
- `R²_raw, SSE, SST, Var(y), WarmupTarget`

표시 규칙:
- `WEAK`면 R² 색상 `Yellow/Red`
- `LOW_VAR`면 `R²` 옆에 `~` 표시

## 7. 구현 단계

Phase 1:
- horizon별 `min_samples` 설정 지원
- `State` 계산 추가
- `R²_raw`와 `R²_clamped` 동시 계산

Phase 2:
- `LOW_VAR` 탐지 + MAE 과학적 표기
- UI 컬럼/색상 반영

Phase 3:
- bet size/route 정책과 연동 (`WEAK`일 때 size 감쇠)

## 8. 수용 기준

1. `N=30`인데 음수 R²인 케이스가 `WEAK`로 명확히 분류된다.
2. `1s` 저분산 구간이 `LOW_VAR`로 구분되어 0.00000 오해가 줄어든다.
3. `1h`는 horizon별 warmup 기준으로 상태가 일관되게 표시된다.
4. 운영자가 음수 R²를 버그로 오인하지 않도록 정보가 충분히 제공된다.

## 9. Open Questions

1. `R²_clamped`를 기본값으로 둘지, `R²_raw`를 기본값으로 둘지?
2. `min_samples` 기본값을 환경별로 다르게 둘지?
3. `WEAK` 상태에서 자동으로 predictor fallback을 걸지(또는 경고만 줄지)?

