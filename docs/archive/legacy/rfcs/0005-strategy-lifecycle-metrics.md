# RFC 0005: 전략 생성일과 총 실행시간 표시

- Status: Draft
- Author: sandbox-quant
- Date: 2026-02-18
- Related:
  - `docs/rfcs/0003-ui-transition-to-multi-asset-multi-strategy.md`
  - `docs/rfcs/0004-strategy-config-mutability.md`

## 1. 문제 정의

현재 UI는 전략의 성과(PnL, W/L/T)는 보여주지만, 다음 정보가 없다.

- 이 전략이 언제 생성되었는지
- 이 전략이 실제로 얼마나 오래 실행되었는지

그래서 운영자가 다음을 판단하기 어렵다.

- 성과가 충분한 관측 기간에서 나온 결과인지
- 새로 만든 전략과 오래 돌린 전략을 구분해 평가하고 있는지

## 2. 목표

- 전략별 `created_at`(생성 시각) 표시
- 전략별 `cumulative_running_time`(누적 실행시간) 표시
- 재시작 후에도 값이 복원되도록 영속화

## 3. 비목표

- 본 RFC는 성과 계산 공식(PnL/승률) 변경을 다루지 않는다.
- 백테스트 리포트 시스템 신규 구축은 범위 밖이다.

## 4. 용어/정의

- `created_at_ms`: 전략 프로필이 처음 생성된 UTC epoch milliseconds
- `running`: 전략이 활성 상태로 tick/신호 처리를 수행 중인 상태
- `cumulative_running_ms`: `running` 상태였던 시간의 누적값
- `last_started_at_ms`: 현재 running 세션 시작 시각(일시정지/재시작 계산용)

## 5. 제안 데이터 모델

`StrategyProfile`에 다음 필드 추가:

- `created_at_ms: i64`
- `cumulative_running_ms: u64`
- `last_started_at_ms: Option<i64>`

기본 규칙:

- 새 전략 생성 시(`fork-on-edit`, `new`) `created_at_ms = now`
- 앱 시작 후 현재 활성 전략은 `last_started_at_ms = now`
- 전략 pause/resume/switch 시 누적 시간 반영
- 앱 종료 시 현재 running 전략 누적 반영 후 저장

## 6. 실행시간 집계 규칙

1. 시작/재개:
   - `last_started_at_ms = now` 설정
2. 일시정지/전환/종료:
   - `delta = now - last_started_at_ms`를 `cumulative_running_ms`에 누적
   - `last_started_at_ms = None`
3. 화면 표시:
   - `display_running_ms = cumulative_running_ms + (now - last_started_at_ms)` (running이면)

예외 처리:

- 음수 delta 방지(시계 역행): `max(delta, 0)`
- 비정상 큰 delta(예: sleep/wake 장시간)는 상한 또는 경고 로그 고려

## 7. UI 노출 제안

- Strategy Table에 컬럼 추가:
  - `Created` (예: `2026-02-18`)
  - `RunTime` (예: `12h 31m`)
- Focus/Config 팝업에 상세 표시:
  - `CreatedAt: 2026-02-18 14:03 UTC`
  - `Total Running: 12h 31m 42s`

표시 원칙:

- 기본 테이블은 짧은 포맷(`D+HH:MM`) 사용
- 상세 팝업은 full 포맷 사용

## 8. 영속화/호환성

전략 세션 JSON(`data/strategy_session.json`)에 lifecycle 필드 저장.

하위 호환:

- 구버전 파일에 필드가 없으면
  - `created_at_ms = now` 또는 파일 mtime 기반 보정(선택)
  - `cumulative_running_ms = 0`
  - `last_started_at_ms = None`

## 9. 수용 기준

- 전략 생성 직후 생성일이 UI에 표시된다.
- 전략 pause/resume/switch 후 running time이 예상대로 증가한다.
- 앱 재시작 후에도 생성일/누적시간이 유지된다.
- 서로 다른 전략의 running time이 섞이지 않는다.

## 10. 리스크 및 완화

- 리스크: pause/resume 이벤트 누락 시 누적 오차
  - 완화: 상태 전이 함수 단일화 + 테스트
- 리스크: UI 폭 부족으로 가독성 저하
  - 완화: 테이블은 축약 포맷, 상세는 팝업 제공

## 11. 구현 단계 제안

1. 모델/세션 저장소 확장
2. 상태 전이 훅(pause/resume/switch/shutdown) 집계 반영
3. Strategy Table 컬럼 추가
4. 회귀 테스트 추가
   - lifecycle 직렬화/역직렬화
   - 누적 시간 계산 단위 테스트
