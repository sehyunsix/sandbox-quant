# RFC 0007: 여러 전략 동시 ON 실행 전환

- Status: Draft
- Author: sandbox-quant
- Date: 2026-02-18
- Related:
  - `docs/rfcs/0001-multi-strategy-one-risk-module.md`
  - `docs/rfcs/0004-strategy-config-mutability.md`
  - `docs/rfcs/0005-strategy-lifecycle-metrics.md`

## 1. 문제 정의

현재 엔진은 실질적으로 `활성 전략 1개 + pause/resume` 모델이다.  
`Portfolio Grid`에서 전략별 ON/OFF를 보여도, 실제 주문 생성 루프는 단일 전략 중심이라 “여러 전략 동시 실행”이 불가능하다.

## 2. 목표

- 같은 심볼 또는 서로 다른 심볼에서 **복수 전략을 동시에 ON**
- 전략별 독립 ON/OFF 토글 지원
- 전략별 성과/런타임/리스크 판단을 분리 집계
- 기존 리스크 모듈(One RiskModule)과 충돌 없이 동작

## 3. 비목표

- 본 RFC는 새로운 전략 알고리즘 추가를 다루지 않는다.
- 브로커 계층 구조 변경은 최소화한다.

## 4. 제안 아키텍처

핵심 전환:
- `current_strategy_profile` 단일 상태 제거
- `enabled_strategies: HashSet<source_tag>` 도입
- 전략별 워커 상태를 독립적으로 유지

구성:

1. `Strategy Runtime Table`
   - key: `source_tag`
   - value: `{ profile, enabled, worker_id, last_signal_ts, ... }`

2. Tick Dispatch
   - 현재 `symbol -> worker_ids` 디스패치 유지
   - 각 워커는 자기 전략 파라미터로 신호 계산
   - disabled 워커는 신호/주문 제출만 차단(상태 유지 여부는 정책 선택)

3. Order Path
   - 기존처럼 `source_tag`를 order intent에 포함
   - RiskModule은 전략별 한도/쿨다운 판단을 source_tag 기준으로 처리

## 5. ON/OFF 의미 정의

- `ON`: 해당 전략 워커가 신호 생성 + 주문 제출 가능
- `OFF`: 해당 전략 워커는 주문 제출 불가
  - 옵션 A: SMA 업데이트는 유지 (warm state)
  - 옵션 B: 완전 정지

권고: 옵션 A(업데이트 유지)  
이유: ON 복귀 시 지표 급격 리셋을 줄일 수 있음

## 6. UI 변경

`Portfolio Grid`:
- `State` 컬럼을 전략별 실제 상태와 1:1 동기화
- `O` 키: 선택 전략 토글 (단일 전략이 아닌 개별 전략 토글)
- 다중 ON 상태에서 `Active Count` 표시 (예: `3 ON / 8`)

## 7. 데이터 모델 변경

필수:
- 세션 저장에 `enabled_strategies` 목록 추가
- lifecycle 누적(`running_ms`)을 전략별 ON/OFF 전이로 집계

권장:
- 전략별 최근 에러/거절 카운터 분리

## 8. 마이그레이션 계획

Phase 1. 내부 상태 전환
- 단일 활성 전략 의존 코드 제거
- `enabled_strategies` 기반으로 refresh/render 변경

Phase 2. 실행 루프 전환
- 전략별 워커 유지
- ON 전략만 주문 제출 허용

Phase 3. UI/저장 동기화
- 토글 즉시 UI 반영
- 종료/재시작 시 enabled 상태 복원

Phase 4. 안정화
- 다중 전략 충돌/중복 주문 보호 테스트
- 리스크 거절 코드 가시성 개선

## 9. 리스크 및 완화

리스크:
- 주문 빈도 급증으로 rate-limit 압력 증가
- 같은 심볼에서 전략 충돌 주문 발생
- 상태 복잡도 증가로 회귀 위험 상승

완화:
- 전략별/전역 주문 간격 제한 유지
- 동일 심볼 상충 신호 조정 정책(예: 최근 우선, netting 정책) 도입
- 단계별 feature flag 적용 (`multi_strategy_enable=true`)

## 10. 수용 기준

- 동시에 2개 이상 전략을 ON하고 주문 이벤트가 각 source_tag로 구분 기록된다.
- 특정 전략 OFF 시 다른 ON 전략은 계속 정상 주문한다.
- 재시작 후 ON/OFF 상태가 복원된다.
- Strategy Table의 ON/OFF가 실제 실행 상태와 일치한다.

## 11. 구현 준비 체크리스트

1. 단일 활성 전략 의존 지점 목록화 (`current_strategy_profile` 중심)
2. 전략 상태 저장 구조(`enabled_strategies`) 확정
3. 토글 입력(`O`)을 per-strategy 로직으로 전환
4. 세션 스키마 확장 및 하위호환 처리
5. 통합 테스트 작성:
   - 다중 ON 주문 분리
   - OFF 독립성
   - 재시작 복원
