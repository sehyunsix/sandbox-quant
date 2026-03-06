# RFC-0053: Linux Rust Module 패턴 검토 및 `sandbox-quant` 적용 고민

## 요약
요청하신 대로 Linux kernel Rust(Module) 쪽 패턴을 찾아서, 현재 코드베이스에 바로 적용 가능한지 선별했다.
핵심 결론은 두 가지다.
1) Linux Rust의 핵심 패턴 대부분은 **커널 라이프사이클/드라이버 등록용**으로 강하게 최적화돼 있어 지금 구조(유저 스페이스 바이너리)엔 과적용 위험이 크다.  
2) 다만 3~4개 패턴은 구조 정리 관점에서 그대로 재사용할 가치가 높다.

## 확인한 공식 레퍼런스
- Rust in kernel 문서 인덱스: https://docs.kernel.org/next/rust/
- Quick Start: https://docs.kernel.org/rust/quick-start.html
- Coding Guidelines: https://docs.kernel.org/rust/coding-guidelines.html
- Rust 커널 크레이트 문서: https://rust.docs.kernel.org/kernel/
- 매크로 인덱스: https://rust.docs.kernel.org/macros/index.html
- `module!` 매크로 예시: https://rust.docs.kernel.org/next/macros/macro.module.html
- `#[vtable]` 매크로: https://www.kernel.org/doc/rustdoc/latest/macros/attr.vtable.html
- `module_platform_driver`/`module_phy_driver` 예시: https://rust.docs.kernel.org/kernel/macro.module_platform_driver.html, https://rust.docs.kernel.org/kernel/macro.module_phy_driver.html
- `module_param` 타입 접근: https://rust.docs.kernel.org/next/kernel/module_param/struct.ModuleParamAccess.html
- 샘플 모듈 목록(커널 포함): https://kernel.googlesource.com/pub/scm/linux/kernel/git/libata/linux/%2B/refs/tags/v6.16-rc2/samples/rust/
- `rust_minimal` 샘플 코드: https://kernel.googlesource.com/pub/scm/linux/kernel/git/libata/linux/%2B/refs/tags/v6.16-rc2/samples/rust/rust_minimal.rs

## Linux Rust Module에서 배운 패턴

1. **Entry/Module trait + Drop 기반 생명주기**
- `module!`는 `kernel::Module`을 구현하도록 요구하고, `init` + `Drop`으로 초기화/해제를 표현한다.
- 샘플(`rust_minimal`)은 `impl kernel::Module` + `init` + `impl Drop` 형태를 사용한다.

2. **매크로 기반 특화 등록(entry registration)**
- `module_platform_driver`, `module_phy_driver` 등은 특정 서브시스템(드라이버) 노출을 매크로가 감싸고, 공통 boilerplate를 제거한다.
- 모듈 진입점에서 반복되는 설정/등록 코드를 줄이고 실패 처리 지점을 제한한다.

3. **`#[vtable]` 기반 선택적 콜백 인터페이스**
- C-style vtable 동작(미구현 함수는 NULL 처리)을 Rust trait 추상화에 맞춰 모델링.
- 런타임에서 전략/파이프라인 동작 집합을 선택적 callback 형태로 관리할 때 유효.

4. **모듈 파라미터를 타입화된 값으로 노출 (`module_param`)**
- `module!` + `module_param` 조합은 동적 값 주입을 타입 안전하게 다루려는 설계다.
- 현재 repo의 CLI/Config 파싱 흐름과 설계 의도는 유사.

5. **커널식 에러/안전성 규율**
- `panic` 지양, `Result` 선호, `unsafe` 앞의 `// SAFETY:` 명시, 문서 규약 정비.
- 큰 구조 변경 시 정적 안정성 비용을 낮추려는 가이드가 매우 강함.

## 우리 코드에 “직접/간접” 적용 후보

### 바로 적용 가능한 후보 (우선순위 높음)

1. **RuntimeLifecycle 컨텍스트에 `RunContext` + Drop 정리 도입**
- 대상: `src/main/runtime_entry.rs`, `src/main/runtime_bootstrap.rs`, `src/main/runtime_task_bootstrap.rs`, `src/main/runtime_event_loop.rs`
- 적용 아이디어:
  - 현재 start/stop 경로에 분산된 자원 정리를 `RunContext`(또는 `OrchestratorHandle`)로 모으고,
  - 종료 시 `Drop` 또는 `shutdown` 메서드로 정리.
- 기대효과:
  - 현재 backtest/ETL/데이터 fetch 중간 중단 시 리소스 누수/중복 종료 위험 감소.
  - Linux Module의 init/drop 분리와 동일한 방향.

2. **전략 실행 경로에 “Registration + 플러그인” 패턴 강화**
- 대상: `src/runtime/strategy_runtime.rs`, `src/strategy`, `src/strategy_catalog.rs`
- 적용 아이디어:
  - 현재 16개 전략 직접 import 구조는 유지하되, 전략별 실행체 생성/등록을 `register_strategy::<T>()`로 통일.
  - Linux `module_*_driver`가 등록 boilerplate를 감싸듯 `macro` 또는 빌트인 `registry` 레이어로 통일.
- 기대효과:
  - `runtime_bootstrap` 쪽 strategy 의존성 감소.
  - 신규 전략 추가/비활성화/테스트가 쉬워짐.

3. **옵션/파라미터 계층에 “typed param profile” 레이어 추가**
- 대상: `src/bin/etl_pipeline.rs`, `src/bin/fetch_dataset.rs`, `src/bin/feature_extract.rs`, `src/bin/backtest.rs`
- 적용 아이디어:
 - CLI 문자열 파싱을 그대로 쓰되, 최종 실행에는 `TypedRunParams`로 정규화하여 전달.
 - `module_param`의 타입 기반 접근에서 아이디어를 차용.
- 기대효과:
 - 실험 설정이 분산되어도 기본값/유효성 검사/로그가 일관화됨.

4. **공통 동작 인터페이스를 vtable-like trait로 정리**
- 대상: `src/runtime/execution_intent_flow.rs`, `src/runtime/internal_exit_flow.rs`, `src/runtime/order_history_sync_flow.rs`의 `log_event`/이벤트 송출 패턴
- 적용 아이디어:
  - 현재 반복되는 로깅/이벤트 템플릿을 인터페이스로 묶고, flow별 구현으로 분리.
  - `#[vtable]` 자체를 그대로 가져오기보다 “선택적 메서드 플래그+기본 no-op” 형태로 구현.
- 기대효과:
  - `log_event` 중복 함수 3곳을 통합.
  - 이벤트 라우팅 정책 교체가 쉬워짐.

### 적용 범위가 제한되어야 할 후보 (낮은 우선순위)

1. **`#[vtable]` 매크로 직접 도입**
- 현재 커널용 매크로와 실행 모듈 ABI 관점이 다름. 직접 port는 과도.
- 대신 trait + enum/옵션 flag로 유사 효과를 유저-스페이스 용도에 맞게 구현.

2. **`module_platform_driver` 류 드라이버 등록 매크로**
- 현재 프로젝트는 커널/디바이스 드라이버가 아닌 유저-스페이스 전략 실험 엔진.
- 해당 패턴의 직접 복제는 오히려 구조 혼동을 유발.

3. **`no_std` 패턴**
- Linux 쪽은 커널 제약(`#![no_std]`)이 본질이므로, 현재 tokio/crossterm/ratatui 기반 런타임과 충돌.
- 오히려 추후 embedded/커널 호환 버전으로 분기할 때만 고려.

## 바로 적용 가능한 설계 방향(요약)
- `초기화/종료 분리(Init/Drop)`를 최상단 3개 엔트리(`runtime_entry`, `runtime_bootstrap`, `backtest_tui`)에 먼저 적용.
- 전략/모듈 등록은 문자열/enum 매칭이 아니라 registry builder로 전환.
- 공통 파라미터와 공통 로깅/이벤트를 typed profile + trait 템플릿으로 통합.

## 위험/부작용
- `Drop` 기반 정리 추가 시 비동기 task join/Abort 타이밍을 잘못 설정하면 shutdown hang/누수 유발.
- registry 패턴은 초기 리팩터 초기에 테스트가 늘어나야 동작 안정.
- 타입 파라미터 정규화는 기존 CLI 호환성(옵션 허용 범위)을 유지하면서 점진적 마이그레이션 필요.

## 다음 액션(권장)
1) `RFC-0052`의 1차 항목에 위 4개를 붙여서 `small PR` 1개씩 진행:  
   - PR-A: `RunContext` + 종료 정리 경로 통합  
   - PR-B: 전략 registry + 전략 import 정리  
   - PR-C: typed parameter profile 도입  
   - PR-D: 공통 이벤트 템플릿 통합
2) 각 PR마다 `tests/` 중심의 회귀 테스트 추가(기능은 사용자 요청 규칙: tests/ 고정).

