# RFC 0047: Alpha-Only Portfolio Backtest Safety

- Status: Proposed
- Author: Project maintainer + Codex
- Date: 2026-03-04
- Related: 0040, 0041, 0046, 0043

## 1) 목적

`predictor alpha`만으로 다음 포지션 비율을 정하고, 실거래와 동일한 Portfolio 레이어에서 체결 가능성 판단을 수행하는 방식으로 백테스팅 체계를 정비한다.

핵심은 두 가지다.

1. 전략/주문 데이터 분리 저장
2. 과적합 방지를 기본 동작으로 적용한 walk-forward 실험

## 2) 현재 문제

- 백테스트는 기존 실험 파이프라인과 분리되어 실행되지 않고, 성능 지표와 주문 원장이 한 DB/한 테이블에 뒤섞일 수 있다.
- walk-forward를 하더라도 유효한 out-of-sample 경계를 명확히 보장하기 어렵다.
- 실행 중간에 과도한 파라미터 조합 탐색이 가능해 누수/과적합 리스크가 높다.

## 3) 제안 설계

### 3.1 모듈 경계

- `backtest_strategy` DB: fold/run 레벨 메타, 설정, fold 성능 메트릭만 저장
- `backtest_orders` DB: 주문 원장(체결 로그 스타일)만 저장
- 실행은 `run_walk_forward_backtest` 단일 엔트리포인트

### 3.2 의사결정 경로

- 예측 입력은 `predictor`에서 `alpha`를 산출한 값만 사용
- 실행 비율 결정은 `decide_portfolio_action_from_alpha`를 통해서만 수행
- 기존 `PortfolioDecision -> PortfolioExecutionIntent -> Signal` 흐름을 그대로 사용

### 3.3 평가 방식

- 기본 실행은 walk-forward:
  - train window
  - embargo
  - test window
  - 다음 fold 순차 이동
- fold 내에서 결정은 오직 test 구간만 반영
- 각 fold는 시작시 포지션 초기화 (독립 평가)

## 4) 과적합 방지(필수)

### 4.1 시간적 분리

- `train / embargo / test` 경계 고정
- fold 간 데이터 누수 금지
- 최종 성능 보고는 fold 평균뿐 아니라 worst-case/fold spread 함께 저장

### 4.2 모델 복잡도/파라미터 제약

- 예측기 엔진 스위칭 횟수 제한
- 시그널 임계치(`min_signal_abs`) 기본값은 보수적으로 유지
- 실험은 1개 이상의 비용/슬리피지 조건에서 재실행
- 전략 최적화는 기본적으로 `seed 고정 + 고정 규칙`으로 재현성 보장

### 4.3 검증 로직(권장)

1. `walk-forward` 성능과 단일 풀샘플 성능 비교
2. `sharpe_like`, max drawdown, fee-adjusted PnL 동시 확인
3. regime gate on/off AB 비교 (같은 fold)
4. fold별 표본 수가 적은 구간에서 과도한 성능 편향을 경고

## 5) DB 후보 비교

- SQLite (현재권장 시작점)
  - 장점: 운영 오버헤드 낮음, 단일 바이너리 배포만으로 동작
  - 단점: 동시 쓰기/대용량 동시성 한계
- PostgreSQL
  - 장점: 동시성, 인덱스 운영, 장기 히스토리 분석에 유리
  - 단점: 배포/운영 복잡도 증가
- ClickHouse
  - 장점: 대규모 시계열/이벤트 조회 성능 우수
  - 단점: 운영 비용/셋업 난이도 증가
- DuckDB
  - 장점: 분석형 쿼리에 강점, 파일형 영속화
  - 단점: 백엔드 서비스 운영 패턴엔 부적합

권장: **단일 디바이스 백테스팅은 SQLite 우선**, 운영 확장 시 PostgreSQL을 2차 전환점으로 둔다.

## 6) 수용 기준

1. `cargo run --bin backtest` 실행으로 `--bars` 기반 백테스트 수행 가능
2. 결과가 `backtest_strategy.sqlite`와 `backtest_orders.sqlite`에 분리 저장
3. fold별 표본 충돌 없이 walk-forward 지표 생성
4. 최소 1개 실험에서 과도한 outlier gain이 아닌 cost-aware 지표 기반으로 리스크를 설명

## 7) 롤아웃 순서

1. `backtest` CLI + CSV 파서 + DB 초기화
2. walk-forward 및 fold 메트릭 저장
3. portfolio/prediction 경로에 대한 회귀 테스트 추가
4. `docs/rfcs`의 제약 조건을 적용한 운영 가이드 확정
