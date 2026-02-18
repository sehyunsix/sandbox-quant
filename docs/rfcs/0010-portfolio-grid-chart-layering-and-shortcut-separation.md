# RFC 0010: Portfolio Grid 레이어 분리 및 차트/단축키 표시 개선

- Status: Draft
- Author: sandbox-quant
- Date: 2026-02-18
- Related:
  - `docs/rfcs/0003-ui-transition-to-multi-asset-multi-strategy.md`
  - `docs/rfcs/0008-bottom-keybind-bar-simplification.md`
  - `docs/rfcs/0009-portfolio-grid-risk-visibility.md`

## 1. 문제 정의

현재 Portfolio Grid 활성 시, Grid 하단에 차트 단축키/기본 키바인드가 함께 노출되며 화면이 겹쳐 보인다.

- Grid 컨텍스트와 Chart 컨텍스트가 동시에 보여 시각적 충돌 발생
- 실제 조작 대상(Grid)과 안내 문구(Chart 단축키)가 불일치
- 좁은 터미널에서 텍스트 절단/중첩으로 가독성 급락

## 2. 목표

- Grid 모드에서 Grid 전용 정보만 보이도록 레이어 분리
- Chart 단축키는 Chart 컨텍스트에서만 노출
- 겹침 없는 일관된 팝업 우선순위(Overlay Z-order) 보장

## 3. 비목표

- 차트 엔진/전략 계산 로직 변경은 다루지 않음
- 키맵 재설계 전체를 한 번에 바꾸지 않음

## 4. 제안

### 4.1 모드별 하단 바 분리

- `v2_grid_open == true`:
  - Grid 전용 키만 표시
  - 예: `Tab(panel) J/K(select) O(toggle) N(new) C(config) X(delete) Enter(run) Esc(close)`
- `v2_grid_open == false`:
  - 기존 메인/차트 단축키 표시

### 4.2 Overlay 우선순위 명시

렌더 우선순위:
1. 편집/설정 팝업
2. Portfolio Grid
3. Focus/기타 팝업
4. 기본 대시보드 + chart keybind bar

원칙:
- 상위 Overlay가 열리면 하위 keybind bar는 렌더하지 않음

### 4.3 Grid 상단 차트 미리보기(옵션)

요구사항 반영 옵션으로, Grid 상단에 “현재 선택 심볼 미니 차트” 배치 가능.

- 기본은 OFF
- `Z` 등 토글 키로 활성화
- 활성화 시에도 메인 chart keybind는 숨김 유지

## 5. UX 규칙

- 한 화면에는 한 컨텍스트의 단축키만 표시
- 단축키 줄은 최대 1줄 유지(절단 시 축약형 사용)
- 겹침 대신 숨김/치환 원칙 적용

## 6. 구현 단계

1. 렌더 게이트 추가
- `render()`에서 Grid 오픈 시 메인 keybind 렌더 스킵

2. Grid 전용 keybind 바 정리
- `render_v2_grid_popup` 하단 한 줄만 유지
- Chart 단축키 문구 제거

3. 상태 테스트 추가
- Grid 오픈 시 chart keybind 문자열이 버퍼에 없어야 함
- Grid 클로즈 시 chart keybind가 복귀해야 함

4. (선택) 미니 차트 슬롯 추가
- 레이아웃 충돌 없는 최소 높이 조건에서만 렌더

## 7. 수용 기준

- Grid 오픈 중 화면에서 차트 전용 단축키가 보이지 않는다.
- Grid 하단 안내와 실제 조작 키가 일치한다.
- 좁은 터미널에서도 키 안내가 겹치지 않는다.
- Grid 종료 후 기본 차트 단축키는 정상 복귀한다.

## 8. 리스크 및 완화

- 리스크: 단축키 노출 감소로 기능 발견성 저하
  - 완화: Grid 하단에 `?` 도움말 힌트 제공
- 리스크: 모드 전환 시 안내 전환이 늦게 반영될 수 있음
  - 완화: 상태 변경 직후 강제 리렌더 트리거
