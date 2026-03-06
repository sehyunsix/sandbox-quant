# RFC 0035: EV는 시간가변이어야 하는가, 진입 시점에 고정해야 하는가

- Status: Draft
- Author: sandbox-quant
- Date: 2026-02-25
- Related:
  - `docs/rfcs/0034-ev-formula-validity-and-zero-sticky-diagnosis.md`
  - `docs/rfcs/0027-ev-integration-and-exit-orchestration-for-simple-signal-runtime.md`

## 1. Problem Statement

EV(기대값) 표시/게이팅 정책에서 다음 두 방식이 충돌하고 있다.

1. 시간가변 EV: 시장 변화에 따라 EV를 계속 재계산
2. 진입고정 EV: 매수(진입) 시점의 EV를 포지션 수명 동안 고정

둘 중 하나만 선택하면 각각 다음 문제가 있다.
- 시간가변만 사용: 진입 근거가 사후적으로 바뀌어 감사/설명성이 약해짐
- 진입고정만 사용: 현재 시장 변화를 반영하지 못해 리스크 대응이 느려짐

## 2. Goal

- 실행 품질(리스크 대응)과 설명 가능성(왜 진입했는지)을 동시에 만족하는 EV 운용 정책 정의
- UI/로그에서 혼동 없이 동일 의미로 표기 가능한 데이터 계약 확정

## 3. Non-Goals

- 전략별 EV 수식 최적화 자체
- 확률모형(ML) 도입 여부 결정

## 4. Options

### Option A: Fully Dynamic
- 정의: EV를 항상 최신 가격/상태로 재계산하고 과거 값은 보조 로그로만 유지
- 장점: 시장 급변 시 민감도 높음
- 단점: 진입 당시 의사결정 근거 추적이 어려움

### Option B: Fully Locked-at-Entry
- 정의: 진입 시 계산한 EV를 포지션 종료까지 고정 사용
- 장점: 감사/리플레이/설명성이 가장 좋음
- 단점: 현재 리스크 상태 반영이 늦음

### Option C: Dual EV (권고)
- 정의:
  - `ev_entry_locked`: 진입 시점 EV, 불변
  - `ev_live`: 현재 시점 EV, 주기 갱신
- 의사결정 분리:
  - 진입 품질/리뷰/성과 attribution: `ev_entry_locked`
  - 보유/축소/청산 정책: `ev_live`

## 5. Decision

`Option C (Dual EV)` 채택.

이유:
- 운영(실시간 대응)과 거버넌스(사후 감사)의 목적 함수가 다르다.
- 단일 EV로 두 목적을 동시에 만족시키기 어렵다.

## 6. Data Contract

포지션 단위 필드:
- `ev_entry_locked: f64` (immutable)
- `ev_live: f64` (mutable)
- `ev_live_updated_at_ms: u64`
- `ev_basis: forward` (현재 기준)

이벤트:
- `EntryFilled` 시 `ev_entry_locked` 저장
- 주기 sync/tick 시 `ev_live` 갱신

## 7. Policy Rules

1. Entry Gate
- 신규 진입 게이트는 `ev_live`(진입 직전 계산값) 사용
- 체결 완료 시 해당 값을 `ev_entry_locked`로 확정 기록

2. Position Risk Management
- 강제 청산/감축 조건은 `ev_live` 사용
- 단, 과민 반응 방지를 위해 연속 N회 또는 T초 유지 조건 도입 권장

3. Reporting / UI
- Position 테이블에 `EV`는 기본 `ev_live` 표시
- Drill-down 또는 상세 패널에서 `EntryEV`(=`ev_entry_locked`) 병기
- 로그 문구에서 `ev_live`/`ev_entry_locked`를 명시적으로 구분

## 8. Failure Modes and Mitigations

1. 과도 청산(노이즈)
- 완화: 히스테리시스(연속 조건), 최소 절대편차 임계치 적용

2. 사용자 혼동
- 완화: 컬럼명 고정(`EV`, `EntryEV`) + tooltip 설명 통일

3. 재현 불일치
- 완화: 체결 시점 `ev_entry_locked`를 order/position ledger에 영구 저장

## 9. Acceptance Criteria

1. 체결 건마다 `ev_entry_locked`가 저장되고 실행 중 값이 변경되지 않는다.
2. `ev_live`는 주기 갱신되며, 마지막 갱신 시각을 확인할 수 있다.
3. 청산 의사결정 로그에 어떤 EV를 사용했는지 (`live` vs `entry_locked`) 명시된다.
4. UI에서 두 EV를 구분해 확인 가능하다.

## 10. Rollout Plan

1. Phase 1: 데이터 필드 추가 및 로그 구분
2. Phase 2: UI에 `EntryEV` 노출
3. Phase 3: 청산 정책에 히스테리시스 적용
4. Phase 4: 운영 지표(과청산율, 반전 손실률)로 파라미터 튜닝

## 11. Open Questions

1. `ev_live` 갱신 주기: tick 기반 vs 고정 interval(예: 1s/5s)
2. 히스테리시스 기본값: `N`회/`T`초 기준
3. `EntryEV`를 기본 테이블 컬럼으로 항상 표시할지, 상세 보기로 제한할지

## 12. References

1. QuantConnect, "Alpha Key Concepts" (Algorithm Framework).  
   https://www.quantconnect.com/docs/v2/writing-algorithms/algorithm-framework/alpha/key-concepts
2. QuantConnect, "Alpha Creation" (v1 docs).  
   https://www.quantconnect.com/docs/v1/algorithm-framework/alpha-creation
3. Interactive Brokers, "Order Types and Algos - Arrival Price".  
   https://www.interactivebrokers.com/en/trading/ordertypes.php
4. Interactive Brokers, "Arrival Price | Trading Lesson".  
   https://www.interactivebrokers.com/campus/trading-lessons/arrival-price/
5. CME Group, "Money Calculations for Futures and Options".  
   https://www.cmegroup.com/education/articles-and-reports/money-calculations-for-futures-and-options
6. CME Group, "Money Calculations for CME-cleared Futures and Options (PDF)".  
   https://www.cmegroup.com/clearing/files/CME-Money-Calculations-Futures-and-Options.pdf
7. NBER Working Paper 5857, "Consumption and Portfolio Decisions When Expected Returns are Time Varying".  
   https://www.nber.org/papers/w5857
8. NBER Working Paper 9547, "Strategic Asset Allocation in a Continuous-Time VAR Model".  
   https://www.nber.org/papers/w9547
