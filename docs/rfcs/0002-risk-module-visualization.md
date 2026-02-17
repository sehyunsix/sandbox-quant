# RFC 0002: Multi-Strategy RiskModule 시각화/관측 UI

- Status: Draft
- Author: sandbox-quant
- Date: 2026-02-17
- Related: `docs/rfcs/0001-multi-strategy-one-risk-module.md`

## 1. 문제 정의

`Multi-Strategy + One RiskModule` 구조에서는 주문 성공/거절보다 더 중요한 것이 있다.

- 지금 어떤 리스크 정책이 활성화되어 있는지
- 어떤 전략/자산이 예산(RateLimit, 노출, 손실 한도)을 얼마나 소비했는지
- 주문 지연/거절의 원인이 risk인지 rate인지 broker인지

현재 화면은 포지션/체결 중심이어서, 운영자가 시스템 “통제 상태”를 즉시 파악하기 어렵다.

## 2. 목표

- 운영자가 3초 내에 “지금 막힌 이유”를 알 수 있어야 한다.
- 전략별 성과와 계정 전역 리스크 상태를 동시에 보여준다.
- 장애/병목 징후를 숫자와 타임라인으로 제공한다.

## 3. 비목표

- 본 RFC는 차트 미학 개선이 목적이 아님
- 본 RFC는 웹 대시보드 신규 구축까지 강제하지 않음 (TUI 우선)

## 4. 제안: Risk Cockpit (TUI 우선)

기존 화면 우측/하단 영역에 RiskModule 전용 패널을 추가한다.

### 4.1 패널 구성

- `Risk Summary` (전역)
  - Total Exposure(USDT), Daily PnL, Drawdown, Open Orders
  - Risk 상태등: `OK | WARN | BLOCK`
- `Rate Budget`
  - 분당 weight 사용량/잔여량/리셋 카운트다운
  - 엔드포인트 그룹별 사용량 (account/order/history)
- `Strategy Budget Table`
  - 전략별: 주문 시도/승인/거절, 최근 거절 코드, 누적 노출
- `Rejection Stream`
  - 최근 거절 이벤트 20건 (`risk.*`, `rate.*`, `broker.*`)
- `Execution Latency`
  - intent->decision, decision->submit, submit->fill 지연(ms)

## 5. 핵심 시각화 규칙

### 5.1 상태 컬러

- `OK`: 녹색
- `WARN`: 노란색 (한도 70% 이상)
- `BLOCK`: 빨강 (한도 초과, 주문 차단)

### 5.2 경고 임계치

- Rate budget 사용률 70%/90%에서 단계 경고
- 심볼 노출 한도 80% 이상 경고
- 일일 손실 한도 60%/85% 경고

### 5.3 거절 코드 표준화

- `risk.max_daily_loss`
- `risk.max_symbol_exposure`
- `risk.strategy_cooldown`
- `rate.global_budget_exceeded`
- `rate.endpoint_budget_exceeded`
- `broker.rejected`

## 6. 이벤트/메트릭 계약 (초안)

RiskModule이 UI로 전달할 이벤트:

- `RiskSnapshot`
  - `total_exposure_usdt`, `daily_pnl_usdt`, `drawdown_pct`, `risk_state`
- `RateSnapshot`
  - `global_used`, `global_limit`, `reset_in_ms`, `by_endpoint`
- `StrategyRiskSnapshot`
  - `strategy_id`, `attempted`, `approved`, `rejected`, `exposure_usdt`
- `RiskDecisionEvent`
  - `intent_id`, `strategy_id`, `symbol`, `decision`, `reason_code`, `latency_ms`

## 7. 화면 예시 (TUI)

```text
┌ Risk Summary ───────────────────────────────┐
│ State: WARN   Exposure: 1,240.20 USDT       │
│ DailyPnL: -32.11   DD: -2.4%  OpenOrd: 6    │
└──────────────────────────────────────────────┘
┌ Rate Budget ────────────────────────────────┐
│ Global: 840/1200 (70%)  reset 00:21         │
│ order:420  account:80  history:340           │
└──────────────────────────────────────────────┘
┌ Strategy Budget ────────────────────────────┐
│ MA(cfg)   A:18 R:2  Exp:420  last:risk.cool │
│ MA(fast)  A:22 R:7  Exp:610  last:rate.glob │
└──────────────────────────────────────────────┘
```

## 8. 단계별 구현

1. Phase 1 (필수)
   - Risk/Rate snapshot 데이터 구조 추가
   - 우측 패널에 `Risk Summary + Rate Budget` 렌더
2. Phase 2
   - 전략별 테이블/거절 스트림 추가
   - reason code 표준화
3. Phase 3
   - 지연 타임라인 및 경고 알림(로그/사운드/색 강조)
4. Phase 4 (선택)
   - 웹 관측 페이지로 동일 데이터 미러링

## 9. 수용 기준 (Acceptance Criteria)

- 운영 중 임의 주문 거절 시 reason code가 UI에 1초 내 표시
- Rate limit 90% 이상이면 상태가 `WARN/BLOCK`으로 명확히 전환
- 전략별 승인/거절 카운터가 주문 이벤트와 일치
- 리스크 패널 렌더가 켜져도 UI 프레임 드랍이 허용 범위 내 유지

## 10. 리스크 및 완화

- 메트릭 과다로 UI 노이즈 증가
  - 완화: 기본은 요약 표시, 상세는 토글 팝업
- 이벤트 폭주 시 렌더 지연
  - 완화: 샘플링/버퍼링, 마지막 상태 우선 렌더
- 코드 표준 불일치
  - 완화: `reason_code` enum 고정 및 검증 테스트

