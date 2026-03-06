# RFC 0004: Strategy Config를 불변으로 둘지, 런타임 변경 가능으로 둘지

- Status: Draft
- Author: sandbox-quant
- Date: 2026-02-18
- Related:
  - `docs/rfcs/0001-multi-strategy-one-risk-module.md`
  - `docs/rfcs/0003-ui-transition-to-multi-asset-multi-strategy.md`

## 1. 문제 정의

현재 `Strategy Config`(fast/slow/cooldown/symbol)는 UI에서 수정 가능하다.  
운영 안정성 관점에서는 불변(immutable)이 유리하고, 실험/튜닝 속도 관점에서는 변경 가능(mutabile)이 유리하다.

핵심 질문:
- 실거래/데모 운영 중 설정이 바뀌어도 되는가?
- 바뀐다면 어떤 범위까지 허용해야 안전한가?
- 바뀐 설정의 성과를 기존 전략과 섞지 않고 평가할 수 있는가?

## 2. 목표

- 운영 중 오동작/예상 밖 포지션 변화를 최소화
- 전략 실험 속도를 과도하게 희생하지 않음
- 변경 이력 추적과 재현성을 확보

## 3. 비목표

- 본 RFC는 전략 수학 모델 자체(예: MA 외 전략 추가) 논의를 다루지 않음
- 웹 관리 콘솔 설계는 범위 밖

## 4. 옵션

### Option A. 완전 불변 (Immutable)

정의:
- 프로세스 시작 시 로드된 전략 설정은 런타임에서 수정 불가
- 수정하려면 파일 변경 + 재시작 필요

장점:
- 재현성 최고
- 실수 변경 리스크 최소
- 테스트/감사 용이

단점:
- 운영 중 튜닝 불가
- 실험 사이클이 느림

### Option B. 완전 가변 (Mutable)

정의:
- 런타임에서 모든 전략 파라미터/심볼 즉시 변경 가능

장점:
- 실험 속도 최고
- 시장 변화 대응 빠름

단점:
- 실수 입력 즉시 손실 가능성
- 재현성/감사성 저하
- 상태 전이 버그 가능성 증가

### Option C. 제한적 가변 + Fork-on-Edit (권고)

정의:
- 런타임 편집은 허용하되, 기존 전략을 직접 수정하지 않고 새 버전을 생성함

권고 변경 가능 범위:
- 허용: `cooldown`, `fast/slow` (검증 통과 시), 실행 대상 `symbol` 변경
- 제한: 이미 체결/오픈 포지션이 큰 상태에서 급격한 파라미터 변경은 확인 절차 필요

권고 안전장치:
- 2단계 적용: `Edit` -> `Preview` -> `Apply`
- 범위 검증: `fast >= 2`, `slow > fast`, `cooldown >= 1`
- 변경 이력 저장: 누가/언제/무엇을/이전값/새값
- 핵심 정책: **In-place edit 금지, Fork-on-Edit 강제**
  - 예: `c01` 편집 시 `c01-v2` 생성
  - 기존 `c01`의 통계/히스토리는 절대 변경하지 않음
- 잠금 모드:
  - `config.locked=true`면 런타임 변경 금지

## 5. 운영 모드 제안

- `dev/demo`: Option C (fork-on-edit 기본 허용)
- `paper/live-like`: Option C (fork-on-edit 필수 + confirm 필수)
- `prod/live`: Option A 또는 Option C + 강한 승인 절차

## 6. 제안 결정

현 시점 권고: **Option C (제한적 가변 + Fork-on-Edit)**  
이유:
- 현재 프로젝트는 전략 탐색 단계이며, 완전 불변은 개발 속도를 과도하게 떨어뜨림
- 동시에 완전 가변은 운영 리스크가 큼
- 무엇보다 성과 집계 혼합 문제를 피하려면 in-place 변경을 금지해야 함
- fork-on-edit + 잠금모드로 속도/안정성/평가 신뢰성을 함께 만족 가능

## 7. 구현 스케치

1. `config/default.toml`에 플래그 추가
   - `strategy.runtime_edit.enabled = true|false`
   - `strategy.runtime_edit.fork_on_edit = true` (기본값 true, false 금지 권고)
2. UI `Strategy Config` 저장 경로에 `Preview + Confirm` 단계 추가
3. 변경 이벤트를 `data/strategy_config_audit.jsonl`에 append
4. `enabled=false` 또는 `locked=true`에서 UI 저장 시 거절 메시지 출력
5. 통계 저장 키를 `strategy_id + strategy_version + symbol`로 분리

## 8. 수용 기준

- 잠금 모드에서 런타임 변경 시도 시 반드시 거절되고 로그가 남는다.
- 변경 허용 모드에서 검증 실패값은 저장되지 않는다.
- 변경 성공 시 기존 전략은 유지되고 새 버전 전략이 생성된다.
- 성과/통계가 이전 전략 버전과 혼합되지 않는다.
- 재시작 후에도 버전별 전략/통계를 동일하게 복원할 수 있다.

## 9. 오픈 이슈

- 심볼 변경 시 기존 포지션 청산 정책(자동/수동/금지) 표준화
- fork 생성 시 버전 네이밍 규칙(`v2`, timestamp, hash) 표준화
- 운영자 권한 레벨(단일 사용자 CLI라면 최소 절차만 둘지) 결정
