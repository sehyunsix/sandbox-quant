# RFC 0003: 단일 화면에서 Multi-Asset / Multi-Strategy UI로 전환

- Status: Draft
- Author: sandbox-quant
- Date: 2026-02-17
- Related: `docs/rfcs/0002-risk-module-visualization.md`

## 1. 배경

현재 TUI는 `선택된 1개 심볼 + 선택된 1개 전략` 중심이다.  
`Multi Strategy One RiskModule` 체계로 전환하면, 운영자에게 필요한 질문도 바뀐다.

- 어떤 전략이 어떤 자산에서 리스크/예산을 많이 쓰는가?
- 병목은 전략, 심볼, 리스크 정책, RateLimit 중 어디인가?
- 지금 수동介입이 필요한 대상(자산/전략)은 무엇인가?

즉, 화면의 기본 단위가 `single focus`에서 `portfolio + drill-down`으로 바뀌어야 한다.

## 2. 목표

- 한 화면에서 다자산/다전략 상태를 동시에 관찰
- 키보드만으로 빠르게 필터, 정렬, drill-down 가능
- 리스크/레이트리밋/실행지연을 자산-전략 축으로 동시에 표시
- 기존 단일 심볼 차트 작업흐름도 유지(호환 모드)

## 3. 비목표

- 웹 UI 신규 구축 (TUI 우선)
- 고빈도 백테스트 시각화까지 본 RFC에서 포함하지 않음

## 4. 자료조사 요약

- `ratatui`는 레이아웃 분할/컴포넌트화가 명확해 대시보드형 패널 구성에 적합하다.  
  Ref: https://ratatui.rs/concepts/layout/
- `k9s`는 다중 리소스 운영을 키보드 중심 리스트 + 상세 패널 + 필터로 해결한다.  
  Ref: https://github.com/derailed/k9s/blob/master/docs/commands.md
- Binance는 요청 weight/interval 기반 rate-limit 모델을 명시하므로 UI에 budget 노출이 필수다.  
  Ref: https://developers.binance.com/docs/binance-spot-api-docs/testnet/websocket-api/rate-limits

## 5. 제안 UI 정보구조 (IA)

기본 모드: `Portfolio Grid`  
드릴다운 모드: `Focus View`

### 5.1 Portfolio Grid (기본)

상단 상태바:
- 계정 Equity / Daily PnL / Drawdown / Risk State / Rate Budget

중앙 2x2 패널:
- `A. Asset Table`  
  자산별 노출, 미실현/실현 PnL, 주문 수, 최근 거절
- `B. Strategy Table`  
  전략별 시도/승인/거절, 승률, PnL, 예산 사용률
- `C. Risk & Rate Heatmap`  
  행=자산, 열=전략, 셀=리스크압력(0~100)
- `D. Rejection Stream`  
  최근 거절 이벤트(reason_code, symbol, strategy, age)

하단:
- 시스템 로그 + 키바인딩

### 5.2 Focus View (드릴다운)

`Asset Table` 또는 `Heatmap`에서 선택한 `(symbol, strategy)` 기준으로 전환:
- 기존 차트 + 포지션 + 주문 히스토리 패널 재사용
- 단, 헤더에 상위 컨텍스트(`portfolio rank`, `risk pressure`) 표시

## 6. 상호작용 모델 (키바인딩)

- `Tab`: 패널 포커스 이동 (A->B->C->D)
- `f`: 필터 입력 (symbol/strategy/reason_code)
- `s`: 정렬 키 순환 (PnL, Exposure, RejectRate, Latency)
- `Enter`: 선택 항목 Focus View 진입
- `Esc`: Focus View에서 Grid 복귀
- `g`: 글로벌 위험 요약 팝업
- `r`: Risk reason_code 사전 팝업

## 7. 상태/데이터 모델 변경

현재 `AppState`는 단일 심볼 중심 필드가 많다. 다음으로 분해한다.

```text
AppStateV2
  portfolio: PortfolioSnapshot
  assets: HashMap<Symbol, AssetViewModel>
  strategies: HashMap<StrategyId, StrategyViewModel>
  matrix: HashMap<(Symbol, StrategyId), CellMetrics>
  focus: Option<FocusContext>   // None=Grid, Some=Focus
  ui: UiState                   // 선택/정렬/필터/스크롤
```

핵심 원칙:
- 렌더용 ViewModel과 실행/리스크 도메인 모델 분리
- 이벤트 수신 후 즉시 ViewModel 증분 갱신
- 리스트/테이블은 고정 폭 + 잘림 규칙으로 프레임 안정성 확보

## 8. 이벤트 계약 (RiskModule 연동)

필수 이벤트:
- `PortfolioSnapshotUpdated`
- `AssetMetricsUpdated { symbol, ... }`
- `StrategyMetricsUpdated { strategy_id, ... }`
- `CellMetricsUpdated { symbol, strategy_id, ... }`
- `RiskDecisionEvent { reason_code, approved, latency_ms, ... }`

UI는 이벤트를 받아 다음을 계산:
- `reject_rate_1m`
- `risk_pressure_score`
- `rate_budget_usage_pct`
- `action_needed` (운영介입 필요 플래그)

## 9. 렌더 성능/운영 기준

- 목표 FPS: 20+
- 틱당 렌더 예산: 16ms 이하
- 테이블 row cap: 자산 200, 전략 50, stream 200
- 큰 갱신은 배치(100ms window)로 묶어서 diff 적용

관측 메트릭:
- `ui.render_ms.p50/p95`
- `ui.event_lag_ms`
- `ui.dropped_rows`

## 10. 단계별 마이그레이션

1. Phase 0 (호환 레이어)
   - `AppStateV2` 추가, 기존 단일 뷰 유지
2. Phase 1 (Grid 최소판)
   - Asset/Strategy 테이블만 노출
3. Phase 2 (Risk/Rate 시각화)
   - Heatmap + Rejection Stream 추가
4. Phase 3 (Focus 통합)
   - 기존 단일 심볼 화면을 Focus View로 이식
5. Phase 4 (운영 안정화)
   - 키바인딩 튜닝, 렌더 지연/노이즈 개선

## 11. 수용 기준 (Acceptance Criteria)

- 10개 자산 x 3개 전략 동시 실행 시 Grid에서 상태 손실 없이 갱신
- 거절 이벤트 발생 후 1초 내 해당 셀/스트림에 반영
- Focus View 전환/복귀가 150ms 내 완료
- 단일 심볼 작업흐름(수동 주문, 히스토리 확인) 회귀 없음

## 12. 오픈 이슈

- Heatmap 점수식(`risk_pressure_score`) 가중치 표준
- 다중 전략이 동일 심볼을 동시에 주문할 때 셀 집계 규칙
- 작은 터미널(80x24)에서 패널 축약 규칙

