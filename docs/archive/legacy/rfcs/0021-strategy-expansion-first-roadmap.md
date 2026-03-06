# RFC 0021: Strategy Expansion First Roadmap

- Status: Draft
- Author: sandbox-quant
- Date: 2026-02-19
- Related:
  - `docs/rfcs/0019-network-monitoring-rate-and-sli-upgrade.md`
  - `docs/rfcs/0020-descope-and-core-stabilization.md`

## 1. Decision Summary

Next direction should prioritize **strategy expansion** before exchange expansion.

Rationale:
- lower execution risk and smaller blast radius
- faster validation loop for PnL, risk, and signal quality
- cleaner baseline before adding exchange-specific complexity

## 2. Problem Statement

Current system has meaningful progress in UI, observability, and strategy lifecycle, but still needs stronger strategy-level correctness and comparability. If exchange expansion starts first, the team will multiply integration complexity before strategy quality is stabilized.

## 3. Goals

1. Scale strategy capabilities in a controlled and measurable way.
2. Standardize strategy interfaces (signal, config, stats, risk hooks).
3. Build reliable evaluation pipeline for strategy quality.
4. Prepare architecture so exchange expansion can be added with minimal strategy changes.

## 4. Non-Goals

- adding multiple exchanges in this phase
- introducing cross-exchange smart order routing
- redesigning existing UI theme/layout from scratch

## 5. Why Strategy-First (Compared to Exchange-First)

### 5.1 Strategy-first advantages

- validates core alpha/risk engine in one environment
- keeps debugging domain narrow (model/runtime), not transport/API variance
- improves confidence in PnL attribution before adding venue complexity

### 5.2 Exchange-first risks now

- doubles/triples error surface (rate limits, precision rules, fills semantics)
- harder root-cause analysis when performance differs by venue
- delayed product value if strategy quality is not already strong

## 6. Proposed Scope (Phase 1)

### 6.1 Strategy plugin contract

Define a stable strategy contract:
- inputs: tick/candle/order/balance context
- outputs: signal intents + metadata
- metadata: strategy tag, confidence, cooldown reason, trace id

### 6.2 Strategy config lifecycle

- immutable run snapshot per strategy session
- explicit versioning and migration path for config schema
- deterministic serialization for reproducibility

### 6.3 Strategy evaluation pipeline

- unified replay mode (historical feed -> runtime)
- per-strategy metrics: win rate, realized pnl, max drawdown, fill latency impact
- comparable scorecard for strategy ranking

### 6.4 Risk profile by strategy

- per-strategy limits: cooldown, max active orders, exposure cap
- reason-coded rejection taxonomy
- audit logs linking strategy intent -> risk decision -> order result

### 6.5 UI support

- strategy table shows clear per-symbol, per-strategy realized pnl
- add strategy ranking panel (top/bottom over selected window)
- keep network/system tabs focused on operations, not strategy ranking logic

## 7. Architecture Readiness for Phase 2 (Exchange Expansion)

During strategy phase, enforce boundaries needed later:
- strategy runtime must not depend on exchange-specific models directly
- order submission path should use normalized execution interface
- symbol precision/filters should stay in execution adapter layer

## 8. Milestones

1. M1: Strategy contract stabilization
- strategy trait/interface finalized
- existing strategies migrated

2. M2: Config/versioning hardening
- schema versioning and migration tests
- session snapshot reproducibility tests

3. M3: Evaluation/replay framework
- deterministic replay runner
- baseline report output

4. M4: Strategy portfolio controls
- per-strategy risk profile integration
- pnl attribution checks vs asset-level realized pnl

5. M5: Exchange-ready adapter boundary check
- adapter contract test suite ready for next RFC

## 9. Acceptance Criteria

- per-symbol strategy realized pnl is consistent with asset realized pnl model
- strategy performance comparison can be generated from replay deterministically
- risk rejection reasons are visible and traceable per strategy
- no regression in `cargo test -q`

## 10. Risks and Mitigations

Risk: overfitting during strategy proliferation
- Mitigation: enforce out-of-sample replay segments and holdout checks

Risk: metric inflation by inconsistent fee/slippage handling
- Mitigation: centralized fee/slippage model in evaluation pipeline

Risk: architecture drift before exchange phase
- Mitigation: adapter boundary tests as an explicit gate in M5

## 11. Exit Criteria to Start Exchange Expansion RFC

Start exchange expansion only when all are true:
1. strategy contract and config schema are stable for at least one release cycle
2. replay scorecard is used in decision-making for strategy enablement
3. pnl attribution mismatch issues are resolved and covered by tests
4. execution adapter boundary tests pass for current venue
