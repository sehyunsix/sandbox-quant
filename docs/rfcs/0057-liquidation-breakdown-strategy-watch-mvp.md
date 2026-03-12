# RFC 0057: Liquidation Breakdown Strategy Watch MVP

## Status
Draft

## Summary
This RFC adds the first strategy-facing runtime surface to `sandbox-quant` without breaking the exchange-truth execution core.

The MVP introduces:

- a strategy template catalog
- a `strategy` CLI command group
- in-memory strategy watches
- an initial template named `liquidation-breakdown-short`

The strategy watch model is intentionally narrow:

- event-driven
- one-shot
- futures-oriented
- exchange-native protection after entry

This RFC does not attempt to fully deliver automated execution in one step. It defines the operator story and the MVP runtime scaffolding so the system can evolve toward live liquidation-triggered entry.

## Goals

- Add a user-visible strategy workflow to the current CLI.
- Preserve the current separation between strategy decision-making and execution submission.
- Represent the agreed operator story in code before the full liquidation stream trigger is implemented.
- Capture open quantitative thresholds as explicit MVP follow-ups instead of pretending they are already solved.

## Operator Story

1. View available strategy templates.
2. Start a watch on a symbol with risk parameters.
3. Inspect currently armed watches.
4. When the trigger fires, the system attempts one short entry.
5. After actual fill, the system places exchange-native reduce-only stop loss and take profit.
6. Once exchange protection is live, the strategy responsibility ends.
7. Inspect completed or failed runs via history.

CLI surface:

```text
/strategy templates
/strategy start liquidation-breakdown-short BTCUSDT --risk-pct 0.005 --win-rate 0.8 --r 1.5 --max-entry-slippage 0.001
/strategy list
/strategy show <watch_id>
/strategy stop <watch_id>
/strategy history
```

## Template Definition

Initial template:

- `liquidation-breakdown-short`

User-facing steps:

1. Find a liquidation cluster above current price
2. Wait for price to trade into that cluster
3. Detect failure to hold above the sweep area
4. Confirm downside continuation
5. Enter short from best bid/ask with slippage cap
6. Place reduce-only stop loss and take profit from actual fill
7. End the strategy after exchange protection is live

## Market Data Source

Primary trigger source:

- Binance USD-M futures liquidation stream
- `<symbol>@forceOrder`
- `!forceOrder@arr`

Binance only publishes the latest liquidation snapshot per symbol in a 1000ms window, so the runtime must locally build liquidation clusters over time.

## Trigger Concept

The first template targets a short setup driven by buy-side forced liquidation bursts above price.

Cluster build concept:

- collect `BUY forceOrder` events
- compute `notional = price * qty`
- merge nearby prices by `merge_bps`
- maintain a rolling cluster window

High-level trigger sequence:

1. Valid upper liquidation cluster exists
2. Price trades into or sweeps the cluster
3. Price fails to hold above the sweep area within a short timeout
4. Downside continuation confirms
5. Entry is submitted with a slippage cap

## Entry and Protection Model

Entry model:

- operator mental model: market entry with slippage cap
- runtime implementation: aggressive limit order anchored to best bid/ask

Short entry:

- reference: `best bid`
- limit price: `best_bid * (1 - max_entry_slippage_pct)`

Protection model:

- stop loss and take profit are exchange-native orders
- stop and take profit are computed from actual fill, not planned entry
- strategy run is considered complete only after protection is live on the exchange

## State Model

User-visible states:

- `armed`
- `triggered`
- `completed`
- `failed`
- `stopped`

## Architecture

The strategy layer sits above the current execution service.

Responsibilities:

- strategy/watch layer
  - watch registration
  - template metadata
  - trigger evaluation
  - trade plan generation
- execution layer
  - translate decided action into exchange orders
  - submit entry and protection orders
- exchange
  - maintain protection orders after handoff

The portfolio store remains exchange-truth state only. Strategy memory lives separately.

## MVP Implementation Plan

Phase 1:

- add `strategy` CLI command surface
- add in-memory watch store
- add template metadata and steps
- support `templates`, `start`, `list`, `show`, `stop`, `history`
- record watch lifecycle events

Deferred follow-ups:

- Binance liquidation stream ingestion
- liquidation cluster builder
- trigger evaluation loop
- entry submission bundle
- exchange-native SL/TP handoff

## Open Quantitative Questions

These are intentionally left unresolved for MVP tuning:

- `min_cluster_notional`
- `merge_bps`
- `min_event_count`
- `failed_hold_timeout_ms`
- `breakdown_confirm_bps`
- stop placement policy
- partial fill handling thresholds

The runtime should start with heuristics, then tighten them with live observation and data analysis.
