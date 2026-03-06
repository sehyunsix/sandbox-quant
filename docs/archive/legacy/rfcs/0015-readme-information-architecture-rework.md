# RFC 0015: README 정보구조(IA) 재정비

- Status: Draft
- Author: sandbox-quant
- Date: 2026-02-18
- Related:
  - `docs/rfcs/0014-ui-docs-automation-with-rust-bin-and-action-scenarios.md`
  - `README.md`

## 1. 문제 정의

현재 README는 기능 설명, 실행 방법, 캡처 안내, 문서 링크가 한 흐름에 섞여 있어
처음 보는 사용자가 핵심 정보를 빠르게 찾기 어렵다.

관찰된 문제:
1. 섹션 순서가 사용자 여정과 불일치
2. 유사 정보(스크린샷/캡처/문서 링크)가 분산
3. 핵심 실행 경로(설치 -> 실행 -> 조작)가 길게 묻힘
4. 유지보수 시 수정 포인트가 많아 일관성 저하

## 2. 목표

1. README를 "처음 시작하는 사람" 기준으로 재배치
2. 빠른 시작 경로를 최상단에 고정
3. UI/문서/운영 세부는 하위 섹션으로 분리
4. 자동 생성 블록(`UI_DOCS`)과 수동 설명을 명확히 분리

## 3. 비목표

1. 프로젝트 전반 문서 체계(docs-site) 전체 개편
2. 세부 기술 설명을 README에 과도하게 확장

## 4. 제안 구조 (신규 TOC)

1. What is sandbox-quant? (3~5줄)
2. Quick Start (복붙 가능한 최소 명령)
3. Key Controls (운영 최소 키맵)
4. UI Overview (자동 생성 `UI_DOCS` 블록)
5. Configuration (`config/default.toml` 핵심만)
6. Documentation Links (`docs.rs`, `mdBook`, `TESTING.md`)
7. Development Workflow (테스트/릴리즈/기여)
8. Appendix (캡처 원본/추가 참고)

## 5. 섹션별 규칙

### 5.1 Quick Start
- 60초 내 실행 목표
- 필수 명령만 제공
- 선택 정보(고급 설정)는 링크로 분리

### 5.2 Key Controls
- 조작 키를 1차/2차로 구분
- Grid/Selector/Popup 키를 한 표로 정리

### 5.3 UI Overview
- `<!-- UI_DOCS:START -->` ~ `<!-- UI_DOCS:END -->` 구간은 자동 생성 전용
- 수동 설명 문구와 혼합 금지

### 5.4 Documentation Links
- 문서 URL을 한곳으로 모음
- README 내 중복 링크 제거

## 6. 구현 계획

Phase 1. 구조 정리
1. README 헤더/섹션 순서 재배치
2. 중복 문구 제거 및 길이 축소

Phase 2. 표준화
1. Key Controls 표 추가
2. Configuration 최소 예제 추가

Phase 3. 운영 규칙 고정
1. UI_DOCS 자동 블록 설명 1줄 고정
2. README 변경 체크리스트(`docs/` 또는 PR 템플릿) 도입

## 7. 수용 기준

1. 신규 사용자가 README 상단 2개 섹션만 보고 실행 가능
2. UI 이미지/캡처 정보는 `UI Overview` 단일 섹션에만 존재
3. 동일 링크/설명이 2회 이상 반복되지 않음
4. README 길이를 줄이면서(중복 제거) 정보 접근성은 개선됨

## 8. 리스크 및 완화

리스크:
1. 기존 사용자에게 섹션 위치 변경으로 일시적 혼란

완화:
1. 릴리즈 노트에 README 구조 변경 안내
2. 기존 주요 키워드(Quick Start, Usage, Docs)를 유지해 탐색 부담 최소화

## 9. 오픈 이슈

1. README를 한국어/영어 이중 언어로 유지할지 여부
2. Key Controls를 README와 `TESTING.md` 중 어디를 기준 문서로 둘지
