# RFC 0022: Strategy Creation Lifecycle and Governance

- Status: Draft
- Author: sandbox-quant
- Date: 2026-02-19
- Related:
  - `docs/rfcs/0021-strategy-expansion-first-roadmap.md`
  - `docs/rfcs/0020-descope-and-core-stabilization.md`

## 1. Problem Statement

Strategy creation is currently possible, but the lifecycle is not yet fully standardized end-to-end.
Without a strict creation pipeline, strategy quality can vary, and production risk increases:

- inconsistent config quality
- weak reproducibility between backtest/replay/live
- unclear promotion criteria (idea -> production)
- difficult rollback when strategy behavior degrades

## 2. Goal

Define a **single, auditable strategy creation lifecycle** from ideation to production rollout.

## 3. Non-Goals

- building a fully automated ML strategy factory
- cross-exchange optimization in this RFC
- replacing existing order/risk engine

## 4. Strategy Creation Lifecycle (Stages)

### Stage 0: Idea Intake

Required artifacts:
- hypothesis (market inefficiency or behavioral edge)
- target market regime (trend/range/high-volatility/low-liquidity)
- expected risk profile (holding time, drawdown tolerance)

Output:
- strategy proposal ticket (`strategy-proposal/<id>`)

### Stage 1: Spec Definition

Create a strategy spec template with mandatory fields:
- strategy id / tag
- signal definition (entry/exit/invalidation)
- timeframe and universe
- risk constraints (cooldown, max active orders, exposure cap)
- fee/slippage assumptions
- failure modes and guardrails

Output:
- `docs/strategies/specs/<strategy_id>.md`

### Stage 2: Deterministic Replay Validation

Run deterministic replay using fixed dataset windows.

Required metrics:
- realized pnl, win rate, trade count
- max drawdown, pnl volatility
- latency-adjusted fill impact
- rejection reason distribution from risk module

Pass gates:
- minimum trade_count threshold
- drawdown under limit
- no critical rejection-pattern anomaly

Output:
- replay report (`reports/strategy/<id>/<version>/replay.json`)

### Stage 3: Simulation/Staging Run

Run strategy in staging mode with live market feed but protected execution policy.

Requirements:
- no uncontrolled order bursts
- stable risk checks
- log traceability: signal -> risk decision -> order outcome

Output:
- staging validation report

### Stage 4: Production Canary

Deploy with controlled scope:
- limited symbol set
- limited capital/exposure
- explicit rollback threshold

Success criteria:
- pnl drift within replay tolerance band
- operational stability (latency, reconnect, error rates)

### Stage 5: Full Promotion

After canary success:
- remove capital cap in steps
- keep continuous monitoring
- enable periodic re-validation

## 5. Versioning and Immutability Rules

### 5.1 Config immutability by run

- each running strategy instance uses immutable config snapshot
- config changes create a new version; old run remains auditable

### 5.2 Strategy identifier model

Use explicit identifiers:
- `strategy_id` (logical family)
- `strategy_version` (config/code version)
- `run_id` (runtime instance)

### 5.3 PnL attribution boundaries

All PnL and trade metrics must be queryable by:
- symbol
- strategy_tag
- strategy_version
- run_id

## 6. Data and Logging Contract

For every order intent, store traceable fields:
- `trace_id`
- `symbol`
- `strategy_tag`
- `strategy_version`
- `run_id`
- risk decision outcome + reason code
- final order status/fills

This allows exact post-mortem and strategy comparison.

## 7. UI/UX Requirements for Strategy Creation

1. Strategy Create Form (mandatory validation)
- symbol
- base strategy template
- fast/slow periods or equivalent parameters
- risk profile preset

2. Pre-flight checker
- invalid parameter combination detection
- duplicate/overlapping strategy warning
- projected order frequency warning

3. Activation flow
- `Create` -> `Validate` -> `Staging` -> `Canary` -> `Promote`
- each step with explicit status and blocker reason

## 8. Governance Model

### 8.1 Promotion committee logic (lightweight)

At least one reviewer for:
- strategy spec completeness
- replay metrics validity
- risk profile sanity

### 8.2 Kill-switch policy

Must support immediate disable by:
- strategy id
- symbol scope
- global emergency switch

### 8.3 Re-certification cycle

- periodic review every N days (configurable)
- forced replay on material code/risk model changes

## 9. Acceptance Criteria

1. New strategy cannot be activated without spec + replay report.
2. Strategy config changes always create a new versioned run.
3. PnL and fill history are queryable with strategy version/run granularity.
4. Canary rollback can be triggered by predefined thresholds.
5. `cargo test -q` remains green for lifecycle-related tests.

## 10. Implementation Plan (Incremental)

1. Introduce strategy metadata schema (`strategy_version`, `run_id`).
2. Add pre-flight validation module for creation inputs.
3. Add replay report storage and loader.
4. Add staging/canary state machine.
5. Add governance checks and promotion flags.

## 11. Risks and Mitigations

Risk: process overhead slows experimentation
- Mitigation: lightweight templates + automation in replay/report generation

Risk: too many strategy variants in production
- Mitigation: strict canary limits + version retirement policy

Risk: metric gaming
- Mitigation: fixed evaluation windows + holdout segments + reviewer sign-off

## 12. Open Questions

1. What is the default replay window set (e.g., 30/90/180 days)?
2. Should canary capital caps be static or volatility-adjusted?
3. Which metrics are hard blockers vs warning-only?
