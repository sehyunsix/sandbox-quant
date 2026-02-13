# Bug: Alpaca US OPTION 선택 시 옵션 체인이 표시되지 않음

## 요약
Alpaca 연동에서 `US OPTION`으로 전환해도 우측 Option 패널에 체인 데이터가 뜨지 않고 `No option chain snapshot` 상태가 반복됩니다.

## 재현 단계
1. `config/default.toml`에서 `broker = "alpaca"`로 설정
2. 환경변수 `APCA_API_KEY_ID`, `APCA_API_SECRET_KEY` 설정 후 실행
3. 앱 실행 후 `T` → `US OPTION` 선택
4. 10~30초 대기하며 Option 패널 및 로그 확인

## 기대 결과
- Option 패널에 `CP/Strike/Theo/Bid/Ask/Δ/Θ` 행이 채워진 옵션 체인 스냅샷이 표시되어야 함

## 실제 결과
- 패널에 `No option chain snapshot`가 지속 표시됨
- 로그에 아래 경고가 반복될 수 있음
  - `[WARN] Alpaca option snapshot failed: ...`

## 영향 범위
- Alpaca 옵션 시세 확인 기능 사용 불가
- 수동 옵션 매매(B/S) 전 의사결정 데이터 부재
- 옵션 전용 UI/UX 신뢰도 저하

## 우선순위
- **High (P1)**: 핵심 기능(옵션 체인 조회) 불능

## 코드 기반 원인 가설
1. **옵션 스냅샷 feed 하드코딩**
   - `src/alpaca/rest.rs:417`에서 `feed=indicative` 고정
   - 계정/권한/시간대 조건에 따라 빈 결과 또는 에러 가능성
2. **latest trade 에러가 조용히 삼켜짐**
   - `src/alpaca/rest.rs:327`~`src/alpaca/rest.rs:330`에서 비정상 HTTP를 `Ok(None)` 처리
   - 실제 권한/심볼 오류가 사용자에게 충분히 드러나지 않음
3. **UI 메시지가 원인 비노출**
   - `src/ui/dashboard.rs:750`~`src/ui/dashboard.rs:753`는 단순 `No option chain snapshot`만 출력
   - 권한 부족/잘못된 심볼/피드 불일치 구분 어려움

## 검증 계획
1. `get_option_chain_snapshot` 응답의 HTTP status/body를 debug 로그로 추가하여 4xx/5xx 원인 확인
2. `feed`를 설정 기반으로 분리하고(`indicative`/`opra`), 실패 시 fallback 또는 명확한 에러 노출
3. `US OPTION` 선택 직후 체인 요청 파라미터(`underlying`, `limit`, feed) 로그 출력
4. 재현 케이스를 문서화(계정 권한 유/무, 장중/장외, underlying별 결과)

## 임시 대응
- 옵션 체인이 비어 있을 때 패널 하단에 마지막 실패 사유(HTTP 코드 + 요약)를 노출
- latest trade와 chain snapshot 실패를 분리 로그로 표시

## 영구 대응(제안)
- Alpaca 옵션 데이터 접근권한/피드 전략을 config로 명시
- 권한 미충족 시 사용자 친화 에러를 표준화(`Option data permission missing` 등)
- 옵션 체인 조회에 대한 통합 테스트(모킹) 추가
