# RFC-0051: 파일 의존성 그래프 및 구조 복잡도 감사

## 요약
- 목표: `sandbox-quant`의 파일별 의존성을 한 번에 파악하고, 현재 왜 복잡해 보이는지 원인·위험을 정리한다.
- 범위: `src/`와 `src/bin/` 기준 정적 의존성(모듈 `use`, `include!` 기준)만 분석.
- 핵심 결론:
  - 구조는 **메인 런타임 오케스트레이션(bootstrap/event-loop) 중심의 고결합 그래프**로 수렴.
  - `runtime_bootstrap`, `backtest_types`, `runtime_strategy_runtime`, `ui/app_state_types`가 가장 복잡한 허브 역할.
  - 라인 수 제한(1000라인) 때문에 발생한 파일 분할(`include!`)이 "모듈 분리"보다 "물리적 분할"에 가까워 이해 비용이 큼.

## 파일 의존성 그래프(요약)
```mermaid
flowchart TD
  subgraph entry["엔트리"]
    L["src/lib.rs"]
    M["src/main.rs"]
    BE["src/bin/backtest.rs"]
    BT["src/bin/backtest_tui.rs"]
    EPL["src/bin/etl_pipeline.rs"]
    DBP["src/bin/demo_broker_probe.rs"]
  end

  subgraph runtime_mod["런타임 계층"]
    Rentry["src/main/runtime_entry.rs"]
    Rboot["src/main/runtime_bootstrap.rs"]
    Rtask["src/main/runtime_task_bootstrap.rs"]
    Rev["src/main/runtime_event_loop.rs"]
    Rt["src/runtime/mod.rs"]
    RtAlpha["src/runtime/alpha_portfolio.rs"]
    RtExec["src/runtime/execution_intent_flow.rs"]
    RtExit["src/runtime/internal_exit_flow.rs"]
    RtOrderHist["src/runtime/order_history_sync_flow.rs"]
    RtPort["src/runtime/portfolio_layer_state.rs"]
    RtPortSync["src/runtime/portfolio_sync.rs"]
    RtPredictEval["src/runtime/predictor_eval.rs"]
    RtRegime["src/runtime/regime.rs"]
    RtReg["src/runtime/strategy_registry.rs"]
    RtRun["src/runtime/strategy_runtime.rs"]
  end

  subgraph model_mod["도메인 공통"]
    Cfg["src/config.rs"]
    Errm["src/error.rs"]
    Event["src/event.rs"]
    Bnc["src/binance/mod.rs"]
    BncR["src/binance/rest.rs"]
    BncW["src/binance/ws.rs"]
    BncT["src/binance/types.rs"]
    In["src/input.rs"]
    Mdl["src/model/mod.rs"]
    Str["src/strategy_catalog.rs"]
    OM["src/order_manager.rs"]
    OS["src/order_store.rs"]
    Prd["src/predictor/mod.rs"]
    Risk["src/risk_module.rs"]
    Lf["src/lifecycle/mod.rs"]
    LfEngine["src/lifecycle/engine.rs"]
    LfExit["src/lifecycle/exit_orchestrator.rs"]
    BackMod["src/backtest/mod.rs"]
    Sesh["src/strategy_session.rs"]
    UI["src/ui/mod.rs"]
  end

  subgraph ui_mod["UI"]
    UIApp["src/ui/app_state_core.rs"]
    UITypes["src/ui/app_state_types.rs"]
    UIRoot["src/ui/render_root.rs"]
    UIProj["src/ui/app_state_types.rs"]
    UIPan["src/ui/render_panels.rs"]
    UIPop["src/ui/render_popups.rs"]
  end

  subgraph back_mod["백테스트"]
    BCore["src/backtest/core.rs"]
    BWalk["src/backtest/backtest_walk_forward.rs"]
    BSim["src/backtest/backtest_walk_simulation.rs"]
    BFin["src/backtest/backtest_fold_finalize.rs"]
    BArg["src/backtest/backtest_arg_parser.rs"]
    BTypes["src/backtest/backtest_types.rs"]
    BTuiApp["src/bin/backtest_tui_parts/tui_app.rs"]
    BTuiRender["src/bin/backtest_tui_parts/tui_render.rs"]
  end

  subgraph strat["전략군"]
    StratMod["src/strategy/mod.rs"]
    S01["src/strategy/ma_crossover.rs"]
    S02["src/strategy/ma_reversion.rs"]
    S03["src/strategy/macd_crossover.rs"]
    S04["src/strategy/atr_expansion.rs"]
    S05["src/strategy/volatility_compression.rs"]
    S06["src/strategy/regime_switch.rs"]
    S07["src/strategy/ema_crossover.rs"]
    S08["src/strategy/ensemble_vote.rs"]
    S09["src/strategy/aroon_trend.rs"]
    S10["src/strategy/bollinger_reversion.rs"]
    S11["src/strategy/channel_breakout.rs"]
    S12["src/strategy/donchian_trend.rs"]
    S13["src/strategy/opening_range_breakout.rs"]
    S14["src/strategy/roc_momentum.rs"]
    S15["src/strategy/rsa.rs"]
    S16["src/strategy/stochastic_reversion.rs"]
  end

  subgraph pred["예측기"]
    PM["src/predictor/models_core.rs"]
    PT["src/predictor/models_trend.rs"]
    PR["src/predictor/models_regime.rs"]
    PF["src/predictor/models_features.rs"]
  end

  L --> Lf
  L --> Bnc
  L --> Cfg
  L --> Errm
  L --> Event
  L --> In
  L --> Mdl
  L --> OM
  L --> OS
  L --> Prd
  L --> Risk
  L --> Rt
  L --> StratMod
  L --> Str
  L --> Sesh
  L --> UI

  M --> Rentry
  Rentry --> Rboot
  Rentry --> Rtask
  Rentry --> Rev

  Rboot --> "main/app_helpers.rs"
  Rboot --> "main/ui_handlers.rs"
  Rboot --> Bnc
  Rboot --> Cfg
  Rboot --> "src/doctor.rs"
  Rboot --> Errm
  Rboot --> Event
  Rboot --> In
  Rboot --> Lf
  Rboot --> "src/model/mod.rs"
  Rboot --> "src/order_history_sync_gate.rs"
  Rboot --> OM
  Rboot --> OS
  Rboot --> Prd
  Rboot --> Rt
  Rboot --> Str
  Rboot --> Sesh
  Rboot --> UI

  BackMod --> BCore
  BCore --> BArg
  BCore --> BWalk
  BWalk --> BSim
  BWalk --> BFin
  BSim --> BTypes

  BT --> BTuiApp
  BT --> BTuiRender
  BTuiApp --> "src/backtest/mod.rs"
  BE --> "src/backtest/mod.rs"

  Prd --> PM
  Prd --> PT
  Prd --> PR
  Prd --> PF

  Rt --> RtAlpha
  Rt --> RtExec
  Rt --> RtExit
  Rt --> "src/runtime/manage_state_flow.rs"
  Rt --> RtOrderHist
  Rt --> RtPort
  Rt --> RtPortSync
  Rt --> RtPredictEval
  Rt --> RtRegime
  Rt --> RtReg
  Rt --> RtRun

  RtRun --> StratMod

  UI --> UIApp
  UIApp --> "src/ui/app_state_types.rs"
  UIApp --> "src/ui/app_state_events.rs"
  UIRoot --> UIPan
  UIRoot --> UIPop

  BTypes --> Event
  BTypes --> Mdl
  BTypes --> RtAlpha
  BTypes --> RtPredictEval
  BTypes --> RtRegime
  BTypes --> Prd

  OM --> "src/order_manager_core.rs"
  OM --> "src/order_manager_tests.rs"
  "src/order_manager_core.rs" --> "src/order_manager_core_types.rs"
  "src/order_manager_core.rs" --> "src/order_manager_core_runtime.rs"
  "src/order_manager_core_types.rs" --> Bnc
  "src/order_manager_core_types.rs" --> Cfg
  "src/order_manager_core_types.rs" --> Mdl
  "src/order_manager_core_types.rs" --> OS
  "src/order_manager_core_types.rs" --> Risk

  Event --> Mdl
  Event --> OM
  Event --> Risk
  OM --> Mdl

  StratMod --> S01
  StratMod --> S02
  StratMod --> S03
  StratMod --> S04
  StratMod --> S05
  StratMod --> S06
  StratMod --> S07
  StratMod --> S08
  StratMod --> S09
  StratMod --> S10
  StratMod --> S11
  StratMod --> S12
  StratMod --> S13
  StratMod --> S14
  StratMod --> S15
  StratMod --> S16

  Bnc --> BncR
  Bnc --> BncW
  BncR --> "src/binance/rest_client_runtime.rs"
  BncR --> "src/binance/rest_client_rules.rs"
  "src/binance/rest_client_runtime.rs" --> "src/binance/rest_client_requests.rs"
  "src/binance/rest_client_runtime.rs" --> "src/binance/rest_client_pagination.rs"
```

### 파일별 핵심 use 의존성(요약)
- `main/runtime_bootstrap.rs`
  - `binance`, `config`, `doctor`, `error`, `event`, `input`, `lifecycle`, `model`, `order_history_sync_gate`, `order_manager`, `order_store`, `predictor`, `runtime`, `strategy_catalog`, `strategy_session`, `ui`
- `runtime/strategy_runtime.rs`
  - `model`, `strategy`, `strategy_catalog`
- `ui/app_state_types.rs`
  - `event`, `model`, `order_manager`, `order_store`, `risk_module`, `strategy_catalog`, `ui`
- `backtest/backtest_types.rs`
  - `event`, `model`, `predictor`, `runtime`
- `predictor/models_core.rs`
  - `model`, `order_manager`
- `event.rs`
  - `model`, `order_manager`, `risk_module`

## 문제점/리스크 분석
1. **`main/runtime_bootstrap.rs`가 거의 모든 상위 모듈을 직접 끌어들임**
   - 부트스트랩 단위 테스트/리뷰 시 책임 경계가 흐릿하고, 변경 시 영향 범위를 추정하기 어렵다.
   - 초기 기동, 환경 설정, 이벤트 루프, 주문 히스토리 동기화, 전략/포트폴리오 정책 조합이 한 파일에서 결합됨.

2. **`ui/app_state_types.rs`와 `runtime`이 데이터/도메인 과다 공유**
- UI 상태가 이벤트/주문/리스크/전략 카탈로그까지 모두 직접 참조.
  - 실시간 화면용 state와 실행 로직 state가 결합되어, 화면 변경이 런타임 결함을 유발할 가능성 상승.

3. **`event` 모듈이 상호 참조 지점이 너무 큼**
   - 이벤트가 `order_manager`, `risk_module`를 직접 의존.
   - 이벤트를 core bus로 쓰는 경우에는 내부 데이터 모델의 변경이 여러 도메인으로 파급됨.

4. **`predictor/models_core.rs`가 `order_manager::MarketKind`를 참조**
   - 예측기 계층이 execution/거래 계층과 결합되어 테스트/실험/시뮬레이션에서 오염 우려.
   - 이상적으로는 예측기 입력에 필요한 최소한의 추상 정보만 받는 편이 바람직.

5. **`strategy` 하위 16개 모듈을 `runtime/strategy_runtime.rs`가 전부 직접 import**
   - 신기능 추가/삭제 시 런타임 빌드 영향도가 커지고, 전략별 격리/실험이 어려워짐.
   - 전략 등록 방식 변경(캡슐화) 필요.

6. **`backtest/backtest_types.rs`의 역할 과다**
- 모델/설정/리그레션/런타임 연동/저장 계층/메트릭 집계가 뒤섞임.
  - 한 파일이 길지 않더라도 책임이 과중.

7. **`include!` 기반 분할 방식의 비용**
   - 현재 일부 대용량 파일 분할이 `include!`로 관리됨.
   - 장점: 라인 수 제어.
   - 단점: 모듈 경계와 소유권/캡슐화가 흐릿하고, `grep`/IDE 탐색 시 편집 맥락이 분절됨.
   - `main/runtime_bootstrap`/`bin/backtest_tui`/`backtest`/`order_manager`에서 특히 체감됨.

## "필요도/중복성 관점" 추가 판정 (요청 반영)

### 가장 필요 없어 보이는 부분(삭제/재위치 우선순위)
아래 항목은 기능의 존재 가치를 부정한다기보다, 현재 구조상 **핵심 책임이 약하고, 이동/축소로 가시성/유지보수 비용이 크게 줄 수 있는 부분**이다.

1. **`order_manager_tests`를 루트 모듈에서 직접 include**
   - `src/order_manager.rs`는 `order_manager_core.rs`와 `order_manager_tests.rs`를 항상 `include!`한다.
   - 테스트가 `#[cfg(test)]`로 감싸여 있다 해도 파일 존재가 항상 의존성 그래프에 잡혀서 탐색/편집 흐름이 길어짐.
   - `cfg(test)` 하위 모듈로 옮겨 `tests/` 위치(또는 `src/order_manager_tests.rs`를 `#[cfg(test)] mod tests;`)로 정리 가능.

2. **중복 유틸의 중복 배치**
   - `split_symbol_assets`가 5곳 이상에 분산(주요: `risk_module.rs`, `order_manager_core_types.rs`, `order_store.rs`, `ui/render_utils.rs`, `ui/ui_projection.rs`).
   - 동일 의미의 `source_label_from_client_order_id`/`parse_source_tag_from_client_order_id`가 `order_manager_core_types.rs`, `order_store.rs`, `ui/render_utils.rs`로 분산.
   - 이들은 런타임·스토어·UI를 가로지르는 공통 유틸이라 도메인 경계가 불분명해지고, 변경 시 세 군데 동시 수정이 필요.

3. **`log_event` 헬퍼 복제**
   - `runtime/execution_intent_flow.rs`, `runtime/internal_exit_flow.rs`, `runtime/order_history_sync_flow.rs`에 동일 시그니처의 `log_event`가 중복.
   - 2~3줄짜리 함수라더라도 모듈 경계를 넘는 로그 정책이 분산되어 운영 정책 일관성 유지가 어려워짐.

4. **에러 메시지/usage 텍스트 패턴 분산**
- `print_usage` 유사 함수가 `ui_docs.rs`, `bin/fetch_dataset.rs`, `bin/etl_pipeline_parts/pipeline_utils.rs`에서 반복.
- 단순 문자열 템플릿임에도 명령별 중복 유지 시 스펙 불일치 위험(동일 파라미터 이름/설명). 공통 헬퍼로 분리 가능.

### 겹침이 가장 큰 부분(우선 리팩터 후보)
아래는 의존성/구현 패턴 기준으로 겹침이 두드러진 부분이다.

1. **런타임 제어부의 결합 과잉**
   - `main/runtime_bootstrap.rs`는 `binance/config/doctor/error/event/input/lifecycle/model/order_manager/predictor/runtime/ui`까지 폭넓게 import.
   - 부트스트랩 책임이 크고, 모듈 간 경계가 흐려서 변경 영향이 크다.
   - 분리 후 각 단계(시작/동기화/정책/루프) 단위로 책임 이동 필요.

2. **전략 모듈 임포트 패턴의 과도한 공유**
   - 대부분의 전략 파일이 `indicator + model`만으로 구성되는 매우 유사한 import 구조.
   - 전략 자체는 분리된 로직이지만, 구성 템플릿(파이프라인 입력/출력 타입, 공통 헬퍼, 검증/클램프 처리)이 반복될 가능성이 높다.
   - `strategy_runtime.rs`가 16개 전략 타입을 전부 직접 임포트하는 구조와 맞물려 신규 전략 추가/제거 시 리스크가 큼.

3. **UI와 런타임/이벤트 도메인의 공통 의존세트 중복**
   - `ui/app_state_types.rs`는 `event/model/order_manager/order_store/risk_module/ui/strategy_catalog`를 모두 직결.
   - `ui/dashboard.rs`, `runtime/portfolio_layer_state.rs`, `runtime/order_history_sync_flow.rs`의 import 집합이 상당히 유사함.
   - 실질적으로는 UI 표시 전용 Projection 레이어와 런타임 상태 소유 레이어가 뒤섞인 구조로 읽기.

4. **백테스트 타입/저장 경로 역할 혼재**
   - `backtest/backtest_types.rs`는 타입/저장/메트릭/러닝타임 설정 연동이 섞여 있음.
   - 지금은 분할 후 1개 파일이지만, 역할이 여러라서 변화 시 테스트/런타임/저장 각기 다른 점검이 필요.

### 정량 기준(내부 감사 근거)
- `use` 의존성 out-degree 상위:
  - `main/runtime_bootstrap.rs`: 16개 모듈
  - `runtime/strategy_runtime.rs`: 3개 모듈
  - `ui/app_state_types.rs`: 7개 모듈
  - `order_manager_core_types.rs`: 5개 모듈
- `include!` 분할 수가 높은 파일:
  - `predictor/mod.rs`, `backtest/core.rs`, `bin/etl_pipeline.rs`, `ui/mod.rs`, `main/runtime_entry.rs`, `backtest/backtest_walk_forward.rs`, `ui/app_state_core.rs`, `bin/backtest_tui.rs`, `binance/rest.rs`, `order_manager.rs`.
- 중복 후보 이름(구조상 겹침 신호):
  - `split_symbol_assets`(5개 파일), `source_label_from_client_order_id`/`parse_source_tag_from_client_order_id`(다중 파일), `log_event`(3개 런타임 파일).

## "왜 코드가 복잡해졌는가"에 대한 추정 원인
- 실험/운영 기능 추가가 기능 단위가 아니라 파일 단위로 축적됨.
- 백테스트·실시간 런타임·UI가 같은 라이브러리 트리에 과거 데이터와 실시간 데이터를 공통 타입으로 공유하면서, 핵심 경계가 느슨해짐.
- 이전 라인 수 제한 대응이 구조 리팩터보다 우선되어 물리 분할이 먼저 왔다.
- 전략 수·유형이 늘어날수록 런타임 등록부가 커지며 의존성 중심부가 확대됨.

## 개선안 (RFC 제안)

### 1) 런타임 부트스트랩 정리(우선순위 높음)
- `src/main/runtime_bootstrap.rs`를 최소 제어부 + 기능 핸들러 호출부로 분리.
- 신규 구조:
  - `runtime_bootstrap.rs`: CLI 파싱, AppState 구성, 이벤트 채널 구성, 시작/종료 시퀀스만 담당.
  - `runtime/bootstrap_*.rs`: 주문 히스토리/리스크/전략/포트폴리오/시장데이터 준비를 독립 모듈로 분리.
- 단일 파일이 아니라 "흐름 단계"별 모듈 경계를 설정.

### 2) 전략 실행 레이어 분리
- `runtime/strategy_runtime.rs`는 전략 registry 인터페이스만 사용하고, 실제 전략 인스턴스 생성은 `strategy_factory` 또는 `strategy_registry`로 이동.
- 새 전략 등록 규격:
  - `pub trait StrategyPlugin { ... }`
  - `enum StrategyType` + `from_label` + `build`.
- 기존 `runtime::strategy_runtime`에서 `include/use`를 줄이고 런타임-등록 분리.

### 3) 이벤트 모델 경계 정리
- `event.rs`에서 `order_manager` 타입 직접 의존 제거:
  - `OrderHistorySnapshot`, `OrderUpdate`를 이벤트 payload는 별도 dto로 분리.
  - `order_manager`는 `AppEvent`를 consume/emit 하는 adapter 경계만 알게 하고, event bus는 식별자 중심 타입으로 통일.

### 4) 백테스트/러닝타임 타입 분리
- `backtest/backtest_types.rs`를:
  - `config.rs` / `models.rs` / `metrics.rs` / `io.rs` / `walk.rs` / `storage.rs`로 분리.
- 공통 타입만 최소 `backtest/types.rs`로 정리, 실행/저장/평가를 분리.

### 5) UI 상태 최소화
- `ui/app_state_types.rs`를 `AppStateSnapshot`, `UiState`, `RuntimeProjection`으로 분리.
- UI는 `RuntimeProjection`(읽기 전용)만 소비하도록 하고, 런타임 상태를 직접 수정하지 않게 유지.

### 6) 의존성 규칙 도입
- CI 혹은 `make` 타겟으로 파일 의존성 다이어그램 생성 + 사이클 점검.
- 규칙 예시:
  - `event`는 UI 모듈을 import 금지.
  - `predictor`는 `order_manager` 타입 직접 import 금지.
  - `strategy`는 UI 타입을 직접 import 금지.

## 10시간 안쪽 실행 로드맵(리팩터 전개안)
1. **1–2시간**: 자동화 스크립트 추가(의존성 추출 + 다이어그램 생성 + 임계치 경보).
2. **3–4시간**: `runtime_bootstrap`과 `strategy_runtime`의 책임 분해(테스트 기준 유지).
3. **2–3시간**: `event` 페이로드 DTO 추출, `predictor` 경계 정리.
4. **1–2시간**: UI projection 분리 + 영향 범위 테스트/문서화 업데이트.

## 참고
- 이 RFC는 현재 코드 정적 구조 기준 분석이며, 런타임 동작 시점 동작 변경 검증은 별도 릴리즈 단계에서 반영한다.
