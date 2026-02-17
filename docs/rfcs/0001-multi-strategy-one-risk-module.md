# RFC 0001: Multi-Strategy One RiskModule

- Status: Draft
- Author: sandbox-quant
- Date: 2026-02-17

## 1. 배경

현재 구조는 단일 전략/단일 주문 흐름 중심이며, 주문 제출 시점의 검증이 분산되어 있다.  
이 구조는 다음 확장 요구를 어렵게 만든다.

- 한 프로세스에서 여러 전략을 동시에 실행
- 여러 자산을 동시에 거래
- 계정 전체 기준 리스크/주문 빈도/RateLimit을 일관되게 통제

## 2. 제안 요약

애플리케이션 시작 시 단일 `RiskModule`을 생성하고, **모든 전략 주문은 반드시 RiskModule을 경유**한다.  
RiskModule은 두 가지를 중앙 통제한다.

- Pre-trade risk check (포지션/노출/손실/주문 제한)
- Global rate-limit control (REST/WebSocket 액션 예산 관리)

목표 아키텍처는 **Multi-Strategy + One RiskModule**이다.

## 3. 목표 / 비목표

### 목표

- 전략 수와 종목 수가 늘어나도 일관된 리스크 정책 적용
- API rate limit 초과와 주문 폭주 방지
- 전략별 성과 분리 + 계정 전체 리스크 통합
- 추후 브로커 추가 시 공통 통제 계층 재사용

### 비목표

- 본 RFC에서 새로운 전략 알고리즘 자체를 정의하지 않음
- 본 RFC에서 완전한 분산 아키텍처(멀티 프로세스/멀티 노드)까지 다루지 않음

## 4. 제안 아키텍처

### 4.1 컴포넌트

- `StrategyWorkers`: 전략별 태스크. 시그널을 `OrderIntent`로 변환
- `RiskModule` (단일 인스턴스):
  - `RiskEngine`: 정책 검증/수량 조정/거절 사유 생성
  - `RateGovernor`: 전역 + 엔드포인트별 요청 예산 관리
  - `ExecutionQueue`: 승인 주문 직렬화/우선순위 처리
  - `PortfolioState`: 잔고/포지션/실현손익/노출 스냅샷
- `BrokerAdapter`: Binance REST/WS 실행 및 응답 수집

### 4.2 데이터 계약(초안)

- `OrderIntent { strategy_id, symbol, side, order_type, qty_mode, qty_value, ts }`
- `RiskDecision { approved, normalized_qty, reason, policy_hits }`
- `ExecutionReport { order_id, status, fills, fee, latency_ms }`
- `RateBudget { scope, remaining, reset_at }`

### 4.3 제어 흐름

1. 전략이 `OrderIntent` 발행
2. RiskModule이 최신 `PortfolioState` 기준으로 검증
3. `RateGovernor`가 현재 예산 확인
4. 승인 시 `ExecutionQueue`로 전달 후 BrokerAdapter 실행
5. 체결 결과/수수료/거절 사유를 전략 및 UI에 브로드캐스트
6. `PortfolioState` 갱신 후 다음 의사결정에 즉시 반영

## 5. 핵심 정책 (초안)

- 계정 전역 일일 최대 손실(USDT)
- 심볼별 최대 순노출(USDT)
- 전략별 최대 동시 주문 수
- 전략별/심볼별 쿨다운
- 주문 최소/최대 수량 및 notional 정규화
- API weight 기반 분당 예산 + 버스트 제한

## 6. 대안 비교

- 대안 A: 전략별 자체 리스크 체크
  - 장점: 구현 빠름
  - 단점: 정책 불일치, 중복 코드, 전역 통제가 어려움
- 대안 B: 현재 구조 유지 + 부분 보강
  - 장점: 변경 범위 작음
  - 단점: 다전략/다자산 확장 시 복잡도 급증
- 제안안: One RiskModule
  - 장점: 일관성/확장성/관측성 우수
  - 단점: 초기 리팩터링 비용 존재

## 7. 예상 효과

- 주문 거절/실패 원인의 표준화 (`risk`, `rate_limit`, `broker`)
- Rate limit 위반 감소
- 전략 간 자원 경합 시 공정한 실행(우선순위/큐 정책)
- 운영자가 “전략별 성과 + 계정 전체 리스크”를 동시에 모니터링 가능

## 8. 리스크 및 대응

- 단일 모듈 병목
  - 대응: 비동기 큐 + 경량 정책 평가 + 메트릭 기반 튜닝
- 상태 불일치(잔고/포지션 지연)
  - 대응: 주문 전/후 동기화, 재조회 백오프, 불일치 감지 로그
- 정책 과도 보수로 기회 손실
  - 대응: 단계적 롤아웃 + 정책 파라미터 외부화

## 9. 단계별 도입 계획

1. Phase 1: RiskModule 골격 + 기존 단일 전략 경유
2. Phase 2: RateGovernor 통합, 기존 직접 주문 경로 제거
3. Phase 3: 다전략 동시 실행(2개 이상), 전략별 제한 정책 적용
4. Phase 4: 다자산 동시 거래 + 운영 메트릭/알람 강화

## 10. 수용 기준 (Acceptance Criteria)

- 모든 주문 경로가 RiskModule을 통과함 (직접 REST 주문 금지)
- 동일 시점 다전략 주문에서도 전역 리스크 한도 위반이 없음
- 1분 기준 API 예산 초과 요청률이 목표 이하
- UI/로그에 거절 사유 분류와 적용 정책이 일관되게 표시됨

