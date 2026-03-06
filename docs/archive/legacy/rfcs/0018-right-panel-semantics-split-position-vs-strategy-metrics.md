# RFC 0018: Right Panel Semantics Split (Position vs Strategy Metrics)

- Status: Draft
- Author: sandbox-quant
- Date: 2026-02-18
- Related:
  - `src/ui/dashboard.rs`
  - `src/ui/mod.rs`
  - `docs/rfcs/0017-logging-consistency-and-traceability.md`

## 1. Problem Statement

The right-side panel currently mixes fields with different scopes:

- symbol/account scope: `Qty`, `Entry`, `UnrPL`, balances, equity
- strategy scope: `Trades`, `W/L`, `RlzPL`

When selecting a strategy from Grid and moving to chart view, users cannot trust
which numbers represent symbol position vs selected strategy performance.

## 2. Goals

- separate symbol position state from strategy performance state
- make scope explicit in UI labels and panel structure
- keep behavior consistent in Dashboard and Focus View

## 3. Non-Goals

- changing order execution or risk logic
- changing strategy PnL computation formulas
- redesigning chart rendering

## 4. Proposal

Split the right panel into two widgets:

1. `Position` (Symbol Scope)
- balances/equity and position fields only
- includes: `USDT`, `BTC`, `Eq$`, `EqΔ`, `EqROI`, `Price`, `Side`, `Qty`, `Entry`, `UnrPL`, `Fee`
- excludes strategy-specific counters

2. `Strategy Metrics` (Strategy Scope)
- includes: selected strategy label, `Trades`, `Win`, `Lose`, `WinRate`, `RlzPL`
- data source: `strategy_stats_for_item(..., selected_strategy)`

Apply the same split in Focus View.

## 5. Acceptance Criteria

- no strategy metric appears in `Position` widget
- strategy switch updates `Strategy Metrics` immediately
- symbol switch updates `Position` values independently
- Dashboard and Focus View use the same semantic split

## 6. Risks and Mitigations

- Risk: vertical space gets tighter on small terminals
  - Mitigation: prioritize compact text and fixed minimum height for `Strategy Metrics`

- Risk: users accustomed to old merged panel may need adaptation
  - Mitigation: clear widget titles and scope labels

## 7. Implementation Plan

1. add `StrategyMetricsPanel` widget
2. remove strategy fields from `PositionPanel` props/renderer
3. split right column layout into two rows in Dashboard and Focus View
4. update tests for new panel title/visibility
