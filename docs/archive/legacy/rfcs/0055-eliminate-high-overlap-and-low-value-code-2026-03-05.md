# RFC-0055: 고중복/저효율 영역 선별 및 제거 우선순위(2026-03-05)

## 요약
최근 감사 결과(파일 길이/의존성/include 분할)에서 드러난 구조적 피로도 중, 동작 변경 위험을 가장 낮게 갖고 바로 정리 가능한 항목을 정리한다.
목표는 “지금 실행 가능한 기능은 유지”하면서, 이해 비용을 줄이고 다음 refactor 단계의 리스크를 낮추는 것이다.

## 1) 즉시 정리 후보 (낮은 위험)

### 1-1. `include!` 경계의 불균형
- 대상:
  - `src/order_manager_core.rs` + `src/order_manager.rs`
  - `src/backtest/core.rs`
  - `src/ui/mod.rs`, `src/ui/app_state_core.rs`, `src/ui/render_root.rs`
  - `src/main/runtime_entry.rs`, `src/bin/backtest_tui.rs`
- 문제:
  - `include!` 자체는 금지할 필요는 없지만, 함수 본문을 물리 분할한 패턴이 남아 있어 수정 포인트 추적이 어렵다.
  - `order_manager_core_runtime.rs`처럼 조각이 앞쪽 import/문맥에 의존해 실수 시 컴파일 위험이 발생.
- 제안:
  - “컴파일 안전성”이 높은 순서로 분할 파일 정리:
    - (P0) `order_manager_core_runtime.rs` 맨 앞 context 의존 코드 제거 확인(이미 선행 조치 반영).
    - (P0) `order_manager_core.rs`에 include 경계 설명 주석을 붙여, 수정자 혼동을 줄임(진행 중).
    - (P1) 신규 큰 함수 단위만 작은 private helper 모듈로 분리.
    - (P2) `include!`를 유지하되, 모듈 경계 문서화(각 include 대상 하단에 함수/구조 블록 주석).

### 1-2. 이벤트/표기 문자열의 중복
- 대상:
  - `src/runtime/execution_intent_flow.rs`
  - `src/runtime/internal_exit_flow.rs`
  - `src/runtime/order_history_sync_flow.rs`
- 문제:
  - 공통 로그/이벤트 포맷은 통일되어 있지만, 각 플로우별 메시지 조합이 미세하게 분기.
- 제안:
  - `runtime` 공용 이벤트 템플릿 모듈로 “사전 정의 enum+formatter”로 통합.
  - 진행: `runtime/logging.rs`에 history 이벤트 helper 추가 완료 (`order_history_refresh_failed`, `order_history_sync_failed`).
  - 진행 대상 파일: `execution_intent_flow`, `internal_exit_flow`, `order_history_sync_flow`.
  - 추가 진행: `lifecycle_close_internal`, `order_history_sync_task_failed` 추가 및 사용 반영.
  - 이벤트 reason_code와 domain 태그를 상수/enum으로 강제.

### 1-3. 이름/기능 중복 유틸
- 대상:
  - `split_symbol_assets`, `source_label_from_client_order_id`, `parse_source_tag_from_client_order_id`
- 진행 상황:
  - `src/market_utils.rs`로 공통화되어 있음(추가 분산 없음).
- 잔여 작업:
  - 테스트 커버리지를 `tests/` 중심으로 강화해, 추후 재분산 시 회귀 탐지.

## 2) 중기 정리 후보 (중간 위험)

### 2-1. 전략 등록 체인 의존 고도화
- 대상:
  - `src/runtime/strategy_runtime.rs`
  - `src/strategy_catalog.rs`
  - 전략 생성자 호출부(bootstrap/task_bootstrap)
- 문제:
  - 새로운 전략 추가 시 전략 타입 import 체인이 여러 파일에서 반복.
- 제안:
  - 전략 등록 macro 또는 builder 기반 `registry`를 통해 “전략 정의 + 메타(이름/설명/리스크탭) + 생성자” 일괄 등록.

### 2-2. UI 상태 의존성 과다 결속
- 대상:
  - `src/ui/app_state_types.rs`
  - `src/ui/app_state_core.rs` / `src/ui/app_state_events.rs`
- 문제:
  - 상태 이벤트와 도메인 상태가 한꺼번에 묶여 있어 변경 범위가 확장됨.
- 제안:
  - UI 상태를 `도메인-이벤트`, `프레젠테이션`, `임시 뷰 캐시`로 분리.

### 2-3. 백테스트/실험 메타데이터의 다중 소유
- 대상:
  - `src/backtest/backtest_types.rs`
  - `src/backtest/backtest_arg_parser.rs`
  - `src/bin/backtest_tui.rs`
- 문제:
  - run_id, run_id_scheme, 심볼셋 표현, 요약 컬럼이 다중 파일에서 중복 정의/파생.
- 제안:
  - 공통 `backtest::run_metadata` 계층을 도입해 canonical model 하나로 정규화.

## 3) “우선 과감 제거” 후보 (신중 요구)

- `src/app_helpers.rs` 스타일/보일러플레이트 유사 집합:
  - 단일 진입점 테스트 기능과 공통 패턴에서만 유지되며, 현재 일부 함수는 대체 경로 존재.
  - 제거 전 `grep -R` 기반 사용 맵핑과 docs에서 사용례 정리 필요.

- `tests` 인라인/모듈 단위가 아닌 구현체 기반 샘플 경로:
  - 현재 일부 테스트가 구현 파일 내에 없고 별도 테스트 파일로 잘 관리되어 있음.
  - 새 테스트 추가 시 `tests/` 하단 규칙 엄수.

## 4) 10시간 창작/실험 단계에서의 적용 순서(권장)

1) `include!` 경계 안전성 점검 마감(P0, 낮은 코드량).
2) `runtime/logging/event` 템플릿 계층화(P1, 작은 리스크).
3) 전략 registry 분리(P2, 중간 리스크).
4) 백테스트 메타데이터 정합성 레이어(P3, 회귀 테스트 3개 이상).

## 기준/검증
- 각 단계 종료 시 `cargo check` + 핵심 컴포넌트 smoke run은 유지(요청 시 한 번에 수행).
- `tests/`에 신규 regression test 추가(특히 event_reason, strategy registry, run metadata).
