# RFC-0054: 파일 길이/의존성 감사 및 다음 분해 우선순위 (2026-03-05)

## 목표
- 요청한 “모든 Rust 코드 파일 1000줄 이하” 상태를 검증한다.
- 의존성 맵을 만들고, 의심 구간(응집도/결합도/역사적 의존)을 정리한다.
- 다음 리팩터링을 바로 실행 가능한 우선순위로 제시한다.

## 결과 요약
- 현재 `src`, `tests`의 Rust 파일 중 최장 파일은 **960줄**이다.
- 1000줄 초과 파일은 없었다.
- 따라서 “파일 길이 1000줄 이하” 조건은 현재 기준으로 충족됨.
- 실제로 900줄대 파일이 다수 존재해 분할 포인트 후보는 있으나, 즉시 우선순위는 낮다.

## 산출물
- [docs/reports/rust_file_line_counts_2026-03-05.md](docs/reports/rust_file_line_counts_2026-03-05.md)
  - `src`, `tests` 전체 Rust 파일의 라인 수 테이블
- [docs/reports/dependency_edges_2026-03-05.md](docs/reports/dependency_edges_2026-03-05.md)
  - `mod`/`use crate::`/`use super::` 기반의 의존성 스냅샷 (비 테스트 블록)

## 핵심 관찰

1) 라인수 근처 임계 구간 집중도
- 900~960줄 파일 목록
- `src/bin/backtest_tui_parts/tui_app.rs`
- `src/strategy_catalog.rs`
- `src/doctor.rs`
- `src/predictor/models_regime.rs`
- `src/predictor/models_trend.rs`
- `src/bin/etl_pipeline_parts/pipeline_discovery.rs`
- `src/predictor/models_core.rs`
- `src/backtest/backtest_types.rs`
- `src/bin/etl_pipeline_parts/pipeline_config_args.rs`
- `src/binance/rest_client_requests.rs`
- `src/main/runtime_bootstrap.rs`
- `src/main/runtime_task_bootstrap.rs`
- `src/ui/app_state_types.rs`
- `src/ui/render_layout.rs`
- `src/ui/render_panels.rs`

2) 모듈 경계/결합성 이슈 후보
- `order_manager.rs`는 `include!`를 통해 `order_manager_core_types.rs`와 `order_manager_core_runtime.rs`를 한 모듈로 결합한다.
  - 현재는 동작은 되지만, 모듈 경계 시각화가 약하다.
  - 의도치 않은 이름/임포트 의존이 숨겨질 수 있다.
- `runtime/strategy_runtime.rs`는 16개 전략 타입을 직접 임포트한다.
  - 전략 추가/비활성화 비용이 크고, `runtime`이 전략 카탈로그에 과도하게 결합된다.
- `event.rs`는 `order_manager`를 직접 참조한다.
- `ui`는 여전히 여러 도메인 타입을 직접 의존하며, 렌더링 계층-도메인 계층 간 결합이 높다.

3) 중복 제거 진행 중 반영 반영 포인트
- `source_label_from_client_order_id`, `parse_source_tag_from_client_order_id`, `split_symbol_assets`는 공통 모듈로 이관됨.
- `execution_intent_flow / internal_exit_flow / order_history_sync_flow` 로그 이벤트 helper는 `runtime/logging.rs` 공통화됨.
- 동일 작업이 동일 로직 반복을 감소시키고 추후 테스트 집중을 쉽게 한다.

## 제안(다음 단계)

### A. 전략/레지스트리 분리 (우선순위 높음)
- `runtime/strategy_runtime.rs`에서 현재 전략 타입 직접 임포트 목록을 registry/팩토리로 축소한다.
- 목표:
  - `runtime`에서 구체 전략 타입 import 감소
  - 전략 등록/해제/비활성화를 설정 기반으로 수행

### B. ui 모듈 분리 강화 (우선순위 중간)
- `ui`가 현재 의존하는 도메인 타입을 DTO 변환 계층(`ui_projection`/`app_state_*`)으로 한 번 더 분리
- 목표:
  - UI 테스트 안정성 향상
  - 도메인 타입 변경이 UI 렌더 파이프라인에 직접 파급되는 범위 축소

### C. include/모듈 경계 명시화 (우선순위 중간)
- `order_manager` 쪽은 현재 include 방식 유지 가능하지만,
  - 공용 의존 리스트를 명시적으로 재정렬/정리
  - 숨겨진 의존 포인트(예: 모듈 레벨 use 공개)를 주석으로 문서화

## 주의
- 현재 산출물은 현재 상태의 텍스트 기반 추출이므로, 정확한 순환 의존 분석은 `cargo` 메타데이터/컴파일러 의존그래프 기반으로 1차 검증이 필요하다.
- 중단 없는 진행을 위해 현재는 **동작 보존 + 우선 분해** 전략으로 진행한다.
