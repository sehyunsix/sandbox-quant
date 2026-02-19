# RFC 0020: De-scope and Core Stabilization for Next Expansion

- Status: Draft
- Author: sandbox-quant
- Date: 2026-02-19
- Related:
  - `docs/rfcs/0016-app-state-v2-cleanup-and-ui-state-refactor.md`
  - `docs/rfcs/0017-logging-consistency-and-traceability.md`
  - `docs/rfcs/0018-right-panel-semantics-split-position-vs-strategy-metrics.md`
  - `docs/rfcs/0019-network-monitoring-rate-and-sli-upgrade.md`

## 1. Problem Statement

The product has grown quickly across UI views, legacy-compatible state paths, and observability features. While functionality increased, operational complexity also increased:

- duplicated information across views
- mixed old/new state paths in `AppState`
- noisy logs reducing signal-to-noise ratio
- fallback compatibility branches that obscure the canonical data model

This reduces maintainability and slows down the next phase (multi-symbol runtime split, stronger risk controls, and network diagnostics expansion).

## 2. Goal

Reduce non-essential surface area now to improve correctness, readability, and delivery speed for the next expansion.

## 3. Non-Goals

- introducing new trading logic
- changing exchange API integration behavior
- redesigning visual identity/theme

## 4. Scope: What We Intentionally Remove or De-scope

### 4.1 Remove Focus View

Rationale:
- strategy-entry flow already supports chart + trade history drill-down
- Focus View currently duplicates value with additional rendering/state complexity

Action:
- remove Focus View popup and key-paths
- keep direct strategy drill-down as the single interaction model

### 4.2 Remove Legacy/Transitional UI State Duplication

Rationale:
- transitional compatibility fields increase bug surface and reasoning cost

Action:
- converge on one canonical state path for grid/selectors/popups
- remove read/write paths that exist only for backward compatibility

### 4.3 Remove Noisy Default Logs

Rationale:
- repeated connection and high-frequency signal logs hide actionable events

Action:
- default log level in UI should show actionable operation logs first (`WARN/ERR` + selected `INFO`)
- move verbose diagnostics to an explicit debug mode toggle

### 4.4 Remove Legacy Strategy-Stats Lookup Fallbacks (Scheduled)

Rationale:
- strategy stats are now symbol-scoped (`symbol::strategy_tag`)
- fallback lookups (`item`, raw tag variants) may mask data-shape issues

Action:
- keep fallback temporarily behind migration flag
- remove fallback after one stabilization cycle

### 4.5 De-scope UI Docs Snapshot Coverage

Rationale:
- full-surface screenshot regeneration is expensive and slows iteration

Action:
- keep a minimal critical scenario set
- generate full set only for release/documentation milestones

### 4.6 De-scope Bottom Keybind Clutter

Rationale:
- static global key lists are crowded and low-utility per context

Action:
- render context-aware key hints only for active screen/popup

## 5. Proposed Phases

### Phase 1: Safe De-scope (No behavior-risky removals)
- remove Focus View UI path
- reduce keybind hints to context-aware subset
- tighten default log verbosity in UI rendering

### Phase 2: Data-path Convergence
- remove duplicated transitional state read/write paths
- document canonical ownership for each displayed metric

### Phase 3: Compatibility Cleanup
- remove legacy strategy-stats fallback lookup
- enforce symbol-scoped key invariants in tests

## 6. Acceptance Criteria

1. UI code complexity reduction
- measurable reduction in conditional branches and popup-specific handlers

2. State model simplification
- fewer mutable state fields in `AppState`
- one canonical source per displayed metric

3. Operational readability improvement
- reduced repetitive logs in normal operation
- critical events visible without scroll pressure

4. Stability retained
- `cargo test -q` remains green
- existing core runtime behavior remains unchanged

## 7. Risks and Mitigations

Risk: hidden dependency on removed fallback paths
- Mitigation: staged removal + focused regression tests for strategy/asset pnl alignment

Risk: operator confusion after keybind/visibility changes
- Mitigation: short migration note in README and release notes

Risk: over-pruning diagnostics
- Mitigation: keep explicit debug mode and structured log records

## 8. Rollback Plan

- each phase is separable and can be reverted independently
- keep migration commits small and scoped by feature flag where practical

## 9. Next Expansion Readiness Checklist

Before starting the next feature wave, confirm:
- symbol-scoped strategy statistics are the only production lookup path
- UI navigation has one canonical drill-down path
- log policy is documented and enforced in code review
- UI docs pipeline has a lean default mode
