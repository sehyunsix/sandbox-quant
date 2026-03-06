# RFC 0023: Main Runtime Orchestration Analysis (with Mermaid)

- Status: Draft
- Author: sandbox-quant
- Date: 2026-02-19
- Related:
  - `src/main.rs`
  - `docs/rfcs/0020-descope-and-core-stabilization.md`
  - `docs/rfcs/0021-strategy-expansion-first-roadmap.md`
  - `docs/rfcs/0022-strategy-creation-lifecycle-and-governance.md`

## 1. Context

`main` currently orchestrates:
- bootstrap/config/session restoration
- WebSocket worker lifecycle
- strategy + order runtime loop
- TUI input handling
- UI state mutation and event application

This works functionally, but orchestration complexity is now high and slows safe iteration.

## 2. Current Dynamic Flow (Mermaid)

```mermaid
flowchart TD
    A[Process Start] --> B[Load Config + Init Tracing]
    B --> C[Init Channels: app/tick/manual/shutdown/watch]
    C --> D[Restore Strategy Session + Build Catalog]
    D --> E[Create REST Client]
    E --> F[Ping + Preload Historical Klines]
    F --> G[Spawn WS Manager Task]
    F --> H[Spawn Strategy/Order Task]
    G --> G1[Watch enabled instruments]
    G1 --> G2[Spawn/Stop per-symbol WS workers]
    G2 --> I[Tick Channel]

    H --> H1[Init managers/strategies/stats]
    H1 --> H2{tokio::select! loop}
    H2 -->|tick_rx| H3[Update strategy + risk queue + asset snapshot]
    H2 -->|manual_order_rx| H4[Manual signal -> risk queue]
    H2 -->|risk_eval_rx| H5[Submit order + refresh history + balance]
    H2 -->|order_history_sync tick| H6[Aggregate strategy stats + asset pnl]
    H2 -->|watch changes| H7[Symbol/profile/enabled strategy updates]
    H3 --> J[AppEvent Channel]
    H4 --> J
    H5 --> J
    H6 --> J
    H7 --> J

    K[TUI Main Loop] --> K1[Render UI]
    K1 --> K2[Handle key input / popup state / grid state]
    K2 --> K3[May send watch updates and manual signals]
    K3 --> K4[Drain app_rx -> app_state.apply(evt)]
    J --> K4
    K4 --> K
```

## 3. What Is Good (Strengths)

1. Event-driven architecture is already in place.
- Runtime data updates are funneled through `AppEvent`, which is good for future modularization.

2. Runtime separation exists conceptually.
- WS ingestion and strategy/order execution are already split into spawned tasks.

3. Operational recoverability is decent.
- Periodic history sync and state refresh paths reduce stale-data risk.

4. Multi-symbol runtime is partially implemented.
- WS manager can dynamically fan out workers by enabled instruments.

## 4. What Is Risky (Current Pain Points)

1. `main` owns too many responsibilities.
- Lifecycle orchestration, domain logic, and UI interaction logic are heavily mixed.

2. State transition logic is duplicated across input paths.
- Similar updates (symbol/strategy enable/profile/watch sends/session persistence) are repeated in multiple key handlers.

3. Event semantics are mixed.
- `AppEvent` carries both domain events and UI/operational concerns, making invariants harder to reason about.

4. Backpressure and ordering are hard to validate.
- Multiple channels + periodic sync + immediate refresh can interleave updates in non-obvious order.

5. Testability is limited at orchestration layer.
- Most behavior is integration-level in `main` loop; deterministic unit tests for transitions are difficult.

## 5. Likely Failure Modes

1. Drift between selected symbol/strategy and emitted runtime snapshots.
2. Inconsistent UI updates when periodic sync races with immediate order refresh.
3. Regression risk when adding new key actions due to shared mutable state in a large match block.
4. Hard-to-debug production incidents from cross-task timing interactions.

## 6. Proposed Improvement Direction

### 6.1 Introduce Runtime Coordinator boundary

Create `RuntimeCoordinator` (or `AppController`) responsible for:
- watch-channel updates
- strategy/session mutation commands
- consolidated side effects (persist, ws instrument recalc, profile broadcast)

`main` should delegate commands instead of mutating everything inline.

### 6.2 Move key handling to command mapping

Convert raw key handling into:
- `UiCommand` parsing layer
- `CommandHandler` execution layer

This removes repeated mutation blocks and makes behavior table-driven.

### 6.3 Split event types by layer

Use separate enums/modules:
- `DomainEvent` (market/order/strategy/risk)
- `UiEvent` (popup/grid/nav)
- `OpsEvent` (logs, connectivity, diagnostics)

Then map to render state explicitly.

### 6.4 Make orchestration transitions testable

Add tests for:
- symbol switch command transition graph
- strategy enable/disable and session persistence side effects
- periodic sync + order fill race ordering expectations

### 6.5 Narrow `main` role

Target `main` responsibilities:
1. bootstrap dependencies
2. wire components
3. run loop

All domain mutations should live outside `main`.

## 7. Incremental Refactor Plan

### Phase 1: Extract command helper module
- move duplicated key-path mutation blocks into reusable functions
- no behavior changes

### Phase 2: Introduce coordinator
- centralize watch sends + persistence + catalog mutation side effects
- adapt key handlers to coordinator calls

### Phase 3: Event taxonomy split
- classify event streams and normalize update ordering rules

### Phase 4: Hardening tests
- add orchestration-focused tests in `tests/` only

## 8. Acceptance Criteria

1. `src/main.rs` line count and branch complexity are reduced meaningfully.
2. Symbol/strategy transition behavior is covered by deterministic tests.
3. Duplicate side-effect blocks in key handling are removed.
4. No regression in existing runtime behavior and `cargo test -q` remains green.

## 9. Trade-off Note

Refactor cost is non-trivial, but continuing feature expansion on current `main` shape increases regression probability faster than feature velocity gains. This RFC recommends refactoring now, before exchange expansion.
