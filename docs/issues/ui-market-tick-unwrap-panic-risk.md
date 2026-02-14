# Bug: MarketTick 처리 중 current candle unwrap 패닉 위험

## 요약
`AppState::on_event(AppEvent::MarketTick)` 처리 경로에서 `current_candle`에 대해 `unwrap()`을 사용합니다. 상태 불일치가 발생하면 앱이 즉시 패닉으로 종료될 수 있습니다.

## 재현 단계
1. 앱 실행 후 타임프레임 전환/심볼 전환을 반복하며 tick 이벤트를 연속 수신
2. `current_candle` 상태가 예상과 어긋나는 타이밍에서 `MarketTick` 분기 진입
3. `unwrap()` 호출 시 패닉 가능

## 기대 결과
- 상태 불일치가 있어도 앱은 복구 가능한 경로로 처리되어야 함
- 최소한 경고 로그를 남기고 계속 동작해야 함

## 실제 결과
- `current_candle.as_mut().unwrap()`로 인해 프로세스가 크래시될 수 있음

## 영향 범위
- TUI 런타임 안정성 저하
- 장시간 실행 중 예외 종료 가능성

## 우선순위
- **Medium (P2)**: 런타임 안정성 이슈

## 원인 가설
- `should_new` 계산 시점과 실제 업데이트 시점 사이에서 상태가 변경되면 `None` 경로가 가능

## 대응
- `unwrap()` 제거
- `None`일 경우 방어적으로 candle을 재생성하고 경고 로그 출력
