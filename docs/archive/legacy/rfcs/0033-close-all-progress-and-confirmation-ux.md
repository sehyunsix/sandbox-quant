# RFC 0033: Close-All 확인 후 진행 상태 가시화

## 상태
- Draft

## 배경
- 현재 `Z` -> 확인(`Y/N`) 플로우는 도입되었지만, `Y`를 누른 뒤 실제 청산이 진행 중인지 UI에서 명확히 보이지 않는다.
- 사용자 입장에서는 입력이 먹지 않았는지, 네트워크 지연인지, 일부 심볼만 처리 중인지 구분하기 어렵다.

## 문제 정의
- `Close-All`은 고위험 액션이며, 실행 상태 피드백이 없으면 오조작/중복입력/불신을 유발한다.
- 단순 로그 한 줄만으로는 진행률/성공/실패/잔여 대상을 즉시 파악하기 어렵다.

## 목표
- `Y` 확인 직후부터 종료까지 `Close-All` 작업 상태를 명확히 노출한다.
- 진행률(총 대상 대비 완료/실패), 현재 처리 심볼, 최종 결과를 한눈에 보여준다.
- 실패 심볼이 있을 때 재시도 동선을 제공한다.

## 비목표
- 개별 심볼 청산 정책 자체 변경
- 리스크 엔진/주문 엔진 로직 재설계

## 제안

### 1) 실행 상태 모델 추가
- `AppState`에 `close_all_job` 추가:
- `CloseAllJobState { id, status(Idle/Confirming/Running/Done/Partial/Failed), requested_at_ms, total, queued, success, failed, current_symbol, failed_symbols }`

### 2) 이벤트 추가
- 신규 `AppEvent`:
- `CloseAllRequested { id, total }`
- `CloseAllProgress { id, symbol, result(Queued|Success|Failed), reason }`
- `CloseAllFinished { id }`

### 3) 런타임 처리 규칙
- `Y` 확인 시:
- open 포지션 스냅샷을 만들고 `id` 발급
- `CloseAllRequested` 송신 후 심볼별 enqueue
- 내부 청산 결과 수신 시 `CloseAllProgress` 업데이트
- 모든 대상 처리 후 `CloseAllFinished` 송신
- 중복 실행 방지:
- `Running` 상태에서 `Z` 입력 시 새 작업 생성 대신 `"close-all already running"` 안내

### 4) UI 표시
- 메인 하단 또는 상단 상태영역에 `Close-All Status` 배지 표시:
- 예: `CLOSE-ALL RUNNING 3/7 (ok:2 fail:1) current=BTCUSDT`
- 완료 시 5초간 요약 표시:
- `DONE 7/7` 또는 `PARTIAL 5/7 (failed:2)`
- 실패가 있으면 System Log에 심볼+에러코드 출력
- 선택 기능(후속): `[R]etry failed`

### 5) 로그 표준화
- 로그 이벤트명:
- `position.close_all.start`
- `position.close_all.progress`
- `position.close_all.done`
- `position.close_all.partial`

## UX 시나리오
1. 사용자 `Z` 입력 -> 확인 팝업 노출
2. `Y` 입력 -> 즉시 `RUNNING 0/N` 표시
3. 심볼별 진행 시 `k/N` 갱신
4. 종료 후 `DONE` 또는 `PARTIAL` 결과 표시 + 실패 상세 로그

## 리스크/트레이드오프
- 장점:
- 사용자 신뢰도 상승, 중복 입력 감소, 운영 가시성 향상
- 단점:
- 이벤트/상태 필드 추가로 코드 경로가 증가
- 완화:
- `close_all_job` 상태를 단일 구조체로 제한하고 이벤트명을 고정

## 수용 기준
- `Y` 입력 후 300ms 이내에 `Running` 상태가 UI에 표시된다.
- 진행률(`완료/총대상`)이 실시간 갱신된다.
- 실패 심볼이 있을 때 최종 상태가 `Partial` 또는 `Failed`로 표시된다.
- 동작 중 `Z` 재입력 시 중복 작업이 생성되지 않는다.
