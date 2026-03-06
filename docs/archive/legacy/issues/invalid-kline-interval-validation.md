# Bug: 잘못된 kline_interval 값이 조용히 1m로 대체됨

## 요약
`config/default.toml`의 `binance.kline_interval`가 잘못돼도 앱이 실패하지 않고 내부적으로 1분봉(혹은 임의 기본값)으로 동작합니다.

## 재현 단계
1. `config/default.toml`에서 `binance.kline_interval = "1x"` 또는 `"0m"`로 설정
2. 앱 실행
3. 로그와 차트 동작 확인

## 기대 결과
- 잘못된 interval은 앱 시작 시 명확한 설정 오류로 실패해야 함
- 오류 메시지에 잘못된 값 원인이 포함되어야 함

## 실제 결과
- 잘못된 값이 묵살되고 기본 분 단위로 계산되어 실행됨
- 운영자가 오설정을 인지하기 어려움

## 영향 범위
- 전략/차트 타임프레임 왜곡
- 백테스트/실행 결과 신뢰도 저하
- 장애 원인 분석 지연

## 우선순위
- **High (P1)**: 핵심 전략 입력값 오해석

## 원인 가설
- `parse_interval_ms`가 파싱 실패/미지원 suffix를 에러로 반환하지 않고 fallback 값을 반환함

## 대응
- interval 파싱을 `Result<u64>`로 변경
- `Config::load()`에서 시작 시점 유효성 검증 수행
- 런타임 timeframe 전환 시에도 사용자 에러 메시지 출력
