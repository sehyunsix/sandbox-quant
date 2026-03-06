# RFC-0052: Refactoring Design Patterns Search + 적용 후보안

## 개요

이 RFC는 `sandbox-quant`의 현재 구조를 기준으로, **적용 가능한 디자인 패턴 후보를 검색/정리하고**
지금 코드에서 **실제로 붙일 수 있는 후보**를 정리한다.

목표는 “패턴 나열”이 아니라,

- 어떤 패턴이 어디서 필요한지,
- 적용했을 때 어떤 의존성 문제가 줄어드는지,
- 위험도/적용 난이도는 얼마인지,

를 실행 가능한 형태로 정리하는 것이다.

## 검색 근거(외부 레퍼런스)

- Strategy, Factory Method, Facade, Observer, Command: Refactoring.Guru 디자인패턴 문서  
  https://refactoring.guru/design-patterns/strategy  
  https://refactoring.guru/design-patterns/factory-method  
  https://refactoring.guru/design-patterns/facade  
  https://refactoring.guru/design-patterns/observer  
  https://refactoring.guru/design-patterns/command
- CQRS: Microsoft Learn 패턴 설명(명령/조회 분리)  
  https://learn.microsoft.com/en-us/azure/architecture/patterns/cqrs
- Service Layer (Martin Fowler): 서비스 경계와 오퍼레이션 조합  
  https://martinfowler.com/eaaCatalog/serviceLayer.html
- Rust 모듈/가시성/`use` 구조  
  https://doc.rust-lang.org/book/ch07-02-defining-modules-to-control-scope-and-privacy.html
- Extract Method (리팩터링 기초)  
  https://refactoring.com/catalog/extractMethod.html
- Dependency Inversion Principle(의존성 역전)  
  https://en.wikipedia.org/wiki/Dependency_inversion_principle
- Strangler Fig (점진적 교체)  
  https://martinfowler.com/bliki/StranglerFigApplication.html

## 현재 코드에서 확인된 적용 압축 포인트

지금 리포지토리에서 `rg` 기반 정적 분석으로 얻은 핵심 수치

- import 수가 많은 상위 모듈
  - `src/main/runtime_bootstrap.rs`: 38개
  - `src/ui/app_state_types.rs`: 23개
  - `src/runtime/strategy_runtime.rs`: 19개
  - `src/backtest/backtest_types.rs`: 16개
- include! 분할이 집중된 파일
  - `src/predictor/mod.rs`, `src/backtest/core.rs`, `src/bin/backtest_tui.rs`,
    `src/bin/etl_pipeline.rs`, `src/ui/mod.rs`
- 중복 함수가 반복된 구간
  - `split_symbol_assets`: 5곳 이상
  - `source_label_from_client_order_id`, `parse_source_tag_from_client_order_id`: 여러 군데
  - `log_event` 헬퍼: `src/runtime/{execution_intent_flow.rs,internal_exit_flow.rs,order_history_sync_flow.rs}`
  - `print_usage`: `src/bin/fetch_dataset.rs`, `src/bin/etl_pipeline_parts/pipeline_utils.rs`, `src/ui_docs.rs`
- 런타임/UI 경계 혼합
  - `src/event.rs`가 `order_manager`, `risk_module`, `model` 모두 참조
  - `src/ui/app_state_types.rs`가 runtime/상태/비즈니스 타입과 UI 타입을 함께 의존

## 제안 패턴 매핑표 (핵심)

### 1) Facade + Service Layer
- 패턴: Facade
- 적용 근거: `runtime_bootstrap`가 너무 많은 하위 모듈을 직접 다루며 시작/동기화/구성/이벤트/UI 바인딩까지 한 파일에서 처리
- 적용 후보 파일
  - `src/main/runtime_bootstrap.rs`
  - `src/main/runtime_entry.rs`
  - `src/main/runtime_task_bootstrap.rs`
  - `src/main/runtime_event_loop.rs`
- 적용 아이디어
  - 부트스트랩을 단계별 Service로 분리 (`RuntimeBootstrapOrchestrator` 같은 상위 인터페이스)
    - `startup`, `start_background_sync`, `build_runtime_state`, `run_event_loop`
  - 진입점은 “순서 + 옵션 + 실행 플래그”만 다루고, 내부 동작은 하위 서비스가 수행
- 기대효과
  - 38개 `use`의 광범위 의존 축소
  - 테스트 단위/실행 모드 단위 분해가 쉬워짐
- 위험도: 중간 (초기 파일이동량 큼, 런타임 호출 순서 검증 필요)

### 2) Strategy + Factory Method + Registry
- 패턴: Strategy, Factory Method
- 적용 근거: `runtime/strategy_runtime.rs`가 실제 전략 구현을 16개 전부 직접 임포트하고 있음
- 적용 후보 파일
  - `src/runtime/strategy_runtime.rs`
  - `src/strategy_catalog.rs`
  - `src/strategy/mod.rs`
- 적용 아이디어
  - `Strategy` trait 하나로 컨텍스트 바인딩
  - 전략 등록은 `strategy_factory::build_strategy(kind)` 또는 `registry`가 담당
  - `runtime/strategy_runtime.rs`는 전략 실행 인터페이스만 의존
- 기대효과
  - 새로운 전략 추가/제거 시 런타임 모듈 변경 폭 최소화
  - 전략별 테스트를 개별 모듈로 분리, 백테스트/실전 둘 다 동일 규약 사용
- 위험도: 낮음~중간 (테스트 커버리지가 동반되면 안정적)

### 3) CQRS + Projection + Command
- 패턴: CQRS, Command
- 적용 근거
  - UI에서 표시는 읽기 전용이지만 현재 상태 소유/도메인 연산이 섞여있음
  - 주문 이벤트(매수/매도/실행 로그)와 화면 상태가 같은 타입/채널로 섞일 수 있음
- 적용 후보 파일
  - `src/ui/app_state_types.rs`
  - `src/ui/render_panels.rs`
  - `src/runtime/portfolio_layer_state.rs`
  - `src/event.rs`
- 적용 아이디어
  - Command 핸들러(Write Path): 실제 주문/리스크/라이프사이클 변경은 명령 핸들러로만 수행
  - Read Model/Projection(대시보드용 스냅샷): UI는 변경 불가능한 읽기용 스냅샷만 렌더링
  - 이벤트로 projection 갱신
- 기대효과
  - UI/실행 동기화 버그 감소
  - "왜 포트폴리오 표시만 이상" 같은 상태 불일치 추적 용이
- 위험도: 중간~높음 (데이터 경계 설계가 선행되어야 함)

### 4) Observer/Event Publisher 분리
- 패턴: Observer
- 적용 근거: 현재 이벤트가 UI/런타임 양쪽에 바로 노출되며 구독/발행 구분이 모호
- 적용 후보 파일
  - `src/event.rs`
  - `src/runtime/*_flow.rs`
  - `src/ui/*`
- 적용 아이디어
  - 도메인 이벤트는 `DomainEvent` enum/trait 기반으로만 publish
  - UI는 이벤트 수신 구독자, 런타임은 발행자 역할 분리
  - 필요한 경우 채널 수(`mpsc`, watch) 분리
- 기대효과
  - 발행자-구독자 결합 완화
  - 새로운 뷰(테이블, 로그, 알림) 추가 시 런타임 수정 최소화
- 위험도: 중간

### 5) Adapter / DTO 경계 (Predictor-주문/리스크 격리)
- 패턴: Adapter + DTO
- 적용 근거: `src/predictor/models_core.rs`가 `order_manager::MarketKind`를 직접 참조
- 적용 후보 파일
  - `src/predictor/models_core.rs`
  - `src/model`
  - `src/risk_module.rs`
- 적용 아이디어
  - 예측기 입력은 predictor 전용 DTO로 정규화
  - 외부 도메인 타입은 어댑터에서 매핑
- 기대효과
  - 백테스트/예측 모듈이 주문/체결 의존에서 분리되어 재실행/모사/실험이 쉬워짐
- 위험도: 낮음 (매핑 함수와 타입 테스트만 선행)

### 6) DRY / Extract Module (공통 유틸 분리)
- 패턴: Extract Method / Extract Class
- 적용 근거: 공통 유틸이 UI/운영 도메인에 산발적 복제
- 적용 후보 파일
  - `src/order_manager_core_types.rs`
  - `src/order_store.rs`
  - `src/ui/render_utils.rs`
  - `src/ui/ui_projection.rs`
  - `src/risk_module.rs`
- 적용 아이디어
  - `market_symbol_utils` 모듈로 `split_symbol_assets`, 주문 태그 파서 이동
  - `log_event`를 공통 `runtime/logging.rs`로 통일
  - CLI usage 헬퍼는 `cli_help.rs`로 통합
- 기대효과
  - 중복 수정 비용 급감
  - 버그 재현 시 함수별 단위 테스트 추가 용이
- 위험도: 낮음

### 7) Pipeline 패턴(ETL/백테스트 실행 스트림 정형화)
- 패턴: Pipeline / Step Chain
- 적용 근거: ETL 파이프라인이 `pipeline_discovery`, `pipeline_config`, `pipeline_utils`에서 여러 파일 상태로 분산
- 적용 후보 파일
  - `src/bin/etl_pipeline.rs`
  - `src/bin/etl_pipeline_parts/*`
  - `src/bin/backtest_tui.rs`, `src/bin/backtest.rs`
- 적용 아이디어
  - `FetchStep -> FeatureStep -> BacktestStep -> EvalStep` 같은 명시적 파이프라인 단계 정의
  - 각 단계가 독립 `Step` trait/구조체를 구현
- 기대효과
  - 단계별 실패 격리, 로그 추적, 재시도 정책이 쉬워짐
- 위험도: 중간

### 8) Builder/Template-like Configuration
- 패턴: Builder(설정 객체), Template-like sequence
- 적용 근거: 옵션 파서/설정 생성이 스파게티 형태로 반복되어 인자 분기/재사용이 어려움
- 적용 후보 파일
  - `src/bin/etl_pipeline_parts/pipeline_config*.rs`
  - `src/bin/fetch_dataset.rs`
  - `src/bin/feature_extract.rs`
- 기대효과
  - 설정 기본값·검증 규칙 중앙화
  - 실험/백테스트 파라미터 조합 확장에 강함
- 위험도: 낮음~중간

## 패턴 적용 우선순위(실행 플랜)

### 1차 (즉시, 낮은 리스크)
1. 중복 유틸/로깅/usage 통합 (`split_symbol_assets`, `source_tag`, `log_event`, `print_usage`)  
2. `runtime_bootstrap` 기능 분해를 위한 Façade 경계 도입(인터페이스와 단계 함수)
3. `strategy_runtime`에서 전략 등록/생성 최소 인터페이스화

### 2차 (중간 리스크)
1. CQRS-like `AppProjection` 추가 (`runtime` write-model ↔ `ui` read-model 분리)
2. Predictor DTO/Adapter 도입으로 도메인 종속성 정리
3. Observer 계열 이벤트 발행/구독 채널 정리

### 3차 (큰 구조 개선)
1. ETL 단계 파이프라인 정형화
2. include! 재편성(점진적) + 모듈 경계 리라이팅
3. Service Layer 형태의 runtime API 정비(외부 연동 진입점 통일)

## 위험도별 정리

- **낮음**: Extract Method/모듈 분리, 유틸 합치기, 로그 헬퍼 통합, parse 함수 통합
- **중간**: 전략 팩토리 도입, Facade 서비스 경계, 전략 등록 리팩터링
- **높음**: CQRS/Command 완전 분리, 이벤트/채널 재설계, ETL 파이프라인 리라이트

## 과감한 점검 대상(적용하지 않거나 늦춰도 되는 항목)

- `include!` 자체를 금지할 필요는 없음. 다만 새 기능 추가/수정이 잦은 파일의 경우
  - `include!` 대신 명시적 모듈 import 방식이 추적성 측면에서 유리할 가능성이 높음.
- `order_manager.rs`에서 테스트 모듈 포함 방식은 기능에 따라 분리 고려 대상.
  - 테스트는 런타임 코드 경로에서 떨어뜨리는 방향이 바람직.

## 마무리

이 RFC는 구현 전 단계의 “패턴 후보 지도”다.  
다음 단계에서는 위 1차 3개 항목을 기준으로 작은 diff 중심으로 PR 단위 실행을 권장한다.
