# RFC 0032: Position Table을 자산 집계에서 주문/전략 단위로 전환

## 상태
- Draft

## 배경
- 현재 `Positions` 테이블은 `AssetPnlEntry` 기반 자산 집계 뷰다.
- 같은 자산에서 여러 전략이 거래해도 한 줄로 합쳐져, 다음 정보가 손실된다.
- 어떤 전략(`source_tag`)이 만든 포지션인지
- 어떤 주문(`order_id`, `client_order_id`)에서 시작됐는지
- 전략별 EV/Score/Gate/Stop의 시점별 추적

## 문제 정의
- 운영 관점에서 필요한 것은 "자산 상태"보다 "실행 단위 포지션 상태"다.
- 현재 구조에서는 전략별 포지션 관리, 청산 책임 추적, 사후 분석이 어렵다.

## 목표
- `Positions`를 주문/전략 단위 row 모델로 전환한다.
- row 단위로 `strategy`, `order_id`, `entry/exit lifecycle`, `risk fields`를 보존한다.
- 자산 집계는 별도 뷰(예: `Assets`)로 유지한다.

## 비목표
- 거래소 포지션 모델 자체 변경
- 기존 자산 요약 탭 제거

## 제안

### 1) 데이터 모델: Position Ledger 도입
- 신규 구조체(예시):
- `PositionLedgerEntry { position_id, symbol, market, source_tag, entry_order_id, entry_client_order_id, side, entry_price, qty_open, qty_closed, status(Open/Closing/Closed), stop_price, stop_type, ev_entry, p_win_entry, gate_mode_entry, gate_blocked_entry, opened_at_ms, closed_at_ms, close_reason }`
- 키:
- `position_id`를 1차 식별자로 사용
- `position_id`는 신규 진입(fill) 시 생성(현재 lifecycle 엔진의 `position_id` 재사용)

### 2) 이벤트 경로 확장
- `OrderUpdate::Filled(BUY)` 시:
- `PositionLedgerEntry` 생성 또는 기존 open row 증분
- `EvSnapshotUpdate`, `ExitPolicyUpdate`를 해당 `position_id`에 귀속
- `OrderUpdate::Filled(SELL)` 및 내부 청산 시:
- 해당 row의 `qty_closed/status/closed_at_ms/close_reason` 갱신

### 3) UI 분리
- `Assets`:
- 지금처럼 자산 집계 노출(운용 상태)
- `Positions`:
- 주문/전략 단위 row 노출(실행 상태)
- 권장 컬럼:
- `Symbol, Strategy, PositionId, EntryOrderId, Side, QtyOpen, Entry, Last, Stop, StopType, EV@Entry, Score@Entry, Gate@Entry, Status, UnrPnL, RlzPnL, Opened, CloseReason`

### 4) 저장소(선택)
- 세션 복원 필요 시 SQLite `position_ledger` 테이블 추가
- 최소 컬럼: 식별자, 전략, 주문 ID, 수량/가격, 상태, 타임스탬프, 리스크 필드

## 마이그레이션 계획
1. `AppState`에 `position_ledger_by_id` 추가 (in-memory)
2. 주문 fill 이벤트에서 ledger 업데이트 연결
3. `Positions` 탭 데이터 소스를 `assets_view` -> `position_ledger`로 전환
4. 기존 `AssetPnlUpdate` 기반 컬럼은 `Assets` 탭으로 한정
5. 필요 시 SQLite 영속화 추가

## 호환성/리스크
- 리스크:
- 기존 코드의 `symbol` 단위 매칭 로직이 `position_id` 단위로 일부 변경 필요
- 완화:
- 1차 릴리스에서 `symbol+source_tag` fallback 유지
- 점진적으로 `position_id` 귀속으로 전환

## 수용 기준
- 같은 `symbol`에서 서로 다른 `source_tag` 진입이 별도 row로 표시된다.
- 각 row에 `entry_order_id`가 존재한다.
- `EV/Score/Gate/Stop/StopType`이 row 단위로 표시된다.
- 청산 후 `status=Closed` 및 `close_reason`이 기록된다.

