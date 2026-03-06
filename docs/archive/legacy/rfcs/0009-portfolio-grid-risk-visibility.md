# RFC 0009: Portfolio Grid Risk Module 가시성 개선

- Status: Draft
- Author: sandbox-quant
- Date: 2026-02-18
- Related:
  - `docs/rfcs/0002-risk-module-visualization.md`
  - `docs/rfcs/0003-ui-transition-to-multi-asset-multi-strategy.md`
  - `docs/rfcs/0007-concurrent-multi-strategy-enable.md`

## 1. 문제 정의

현재 Portfolio Grid에서 Risk Module 지표는 하단 영역에 텍스트로 표시되어 시인성이 낮다.

- 전략/자산 테이블에 시선이 집중되어 Risk 행을 놓치기 쉽다.
- 임계치 근접/초과 상황이 색상/구조로 명확히 드러나지 않는다.
- 어떤 제한(전역/주문/계정/시세)이 병목인지 즉시 식별이 어렵다.

## 2. 목표

- Risk 상태를 Grid 사용 중에도 즉시 인지 가능하게 개선
- 임계치 기반 경고를 시각적으로 명확히 표시
- 병목 그룹을 1~2초 내 식별 가능하게 구성

## 3. 비목표

- Risk 정책/알고리즘 자체 변경은 범위 밖
- 새로운 리스크 규칙 추가는 다루지 않음

## 4. 제안 UI 구조

### 4.1 상시 Risk Summary Bar (항상 보임)

Portfolio Grid 상단 또는 Total bar 바로 아래에 1줄 요약 표시:

- `GLOBAL 45/120`
- `ORDERS 80/100`
- `ACCOUNT 20/50`
- `MARKET 55/80`

표시 규칙:
- 정상: 회색/청록
- 주의(>= 70%): 노랑
- 위험(>= 90%): 빨강 + `!`

### 4.2 Risk Panel (상세)

`V` 키(예시)로 Risk 상세 패널 토글:

- 그룹별 `used/limit/reset_in_ms`
- 최근 거절 코드 TOP N
- 최근 거절 이벤트 timestamp

### 4.3 테이블 연동 강조

ON 전략 테이블 헤더에 Risk 상태 뱃지 노출:
- `Risk: OK / WARN / CRIT`
- CRIT 상태면 ON/OFF 패널 border를 빨강 강조

## 5. 정보 설계

우선순위:
1. 현재 위험도 (정상/주의/위험)
2. 병목 그룹 (어느 limit이 먼저 꽉 찼는지)
3. 회복 시간 (`reset_in_ms`)

계산:
- `pressure = used / max(limit, 1)`
- `group_state`:
  - `< 0.70` => `OK`
  - `0.70..0.90` => `WARN`
  - `>= 0.90` => `CRIT`
- `overall_state = max(group_state)`

## 6. 키/조작 제안

- `V`: Risk 상세 패널 토글
- `Shift+V`(선택): 상세 보기 레벨 전환(요약/디버그)

기존 키와 충돌 시 우선순위:
- Grid 오픈 상태에서만 `V` 유효
- 일반 화면에서는 기존 동작 유지

## 7. 구현 단계

1. Risk 상태 모델 정리
- `RateBudgetSnapshot -> PressureState` 변환 헬퍼 추가

2. 요약 바 렌더 추가
- Portfolio Grid 상단 고정 라인에 상태/색상 반영

3. 상세 패널 추가
- `v2_grid_risk_open: bool` 상태 도입
- `V` 키 처리 및 상세 위젯 렌더

4. 경고 강조 연동
- ON/OFF 패널 border/제목에 상태 반영

5. 테스트
- 렌더 테스트: WARN/CRIT 색상 분기
- 키 입력 테스트: `V` 토글 동작

## 8. 수용 기준

- Grid 화면에서 Risk 상태를 별도 스크롤 없이 인지 가능
- 임계치 90% 이상에서 시각 경고가 즉시 표시
- 병목 그룹과 reset 시간이 한 화면에서 확인 가능
- 기존 전략 조작 흐름(J/K/O/Enter)은 회귀 없이 유지

## 9. 리스크 및 완화

- 리스크: 정보가 너무 많아 다시 복잡해질 수 있음
  - 완화: 기본은 1줄 요약, 상세는 토글로 분리
- 리스크: 색상 의존성으로 접근성 저하
  - 완화: 색상 + 텍스트 배지(`OK/WARN/CRIT`) 동시 제공
