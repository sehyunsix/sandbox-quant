# Release Plan: `0.1.2 -> 1.0.0`

- Current Version: `0.1.2`
- Target Major Version: `1.0.0`
- Last Updated: 2026-02-17
- Related RFCs:
  - `docs/rfcs/0001-multi-strategy-one-risk-module.md`
  - `docs/rfcs/0002-risk-module-visualization.md`
  - `docs/rfcs/0003-ui-transition-to-multi-asset-multi-strategy.md`

## Version Roadmap

1. `0.1.2 -> 0.2.0` (Foundation: One RiskModule)
2. `0.2.0 -> 0.3.0` (Runtime: Multi-Strategy + Multi-Asset)
3. `0.3.0 -> 0.4.0` (UI Transition: Portfolio Grid + Focus View)
4. `0.4.0 -> 1.0.0` (Major: Default Architecture Switch)

---

## `0.2.0` Checklist (Foundation)

### Scope
- Introduce single `RiskModule` entrypoint for all order intents.
- Standardize risk decision / rejection reason codes.
- Add global rate-governor skeleton and budget snapshots.

### Build Checklist
- [ ] Add `OrderIntent` and `RiskDecision` domain structs.
- [ ] Route all order submissions through `RiskModule::evaluate_and_dispatch`.
- [ ] Remove direct order submit path bypassing risk checks.
- [ ] Add `reason_code` taxonomy (`risk.*`, `rate.*`, `broker.*`).
- [ ] Emit structured logs for each decision (approved/rejected/normalized).
- [ ] Add config section for global risk/rate policies.

### Validation Checklist
- [ ] Unit tests for policy evaluation pass/fail branches.
- [ ] Integration test confirms direct submit bypass is impossible.
- [ ] Rejection reason codes appear consistently in logs/UI.
- [ ] `cargo test -q` passes.

### Release Exit Criteria
- [ ] Every order event includes `intent_id` + decision source.
- [ ] Known rejection scenarios map to stable `reason_code`.

---

## `0.3.0` Checklist (Multi Runtime)

### Scope
- Run multiple strategies concurrently in one process.
- Trade multiple symbols concurrently with shared risk/rate governance.
- Apply strategy-level and symbol-level limits.

### Build Checklist
- [x] Introduce strategy worker registry (`strategy_id` keyed).
- [x] Add per-symbol execution channels and shared risk queue.
- [x] Implement per-strategy cooldown / max-active-orders.
- [x] Implement per-symbol exposure limits (USDT notionals).
- [x] Add global API budget accounting by endpoint group.
- [x] Persist strategy+symbol scoped stats for restart recovery.

### Validation Checklist
- [ ] Simulated concurrent intents do not violate global limits.
- [ ] 10 symbols x 3 strategies run without deadlock/starvation.
- [x] Rate budget throttling prevents limit burst failures.
- [x] `cargo test -q` passes.

### Release Exit Criteria
- [ ] Multi-strategy scheduling is deterministic under fixed seed/input.
- [ ] No order accepted when global or symbol risk limit is exceeded.

---

## `0.4.0` Checklist (UI Transition)

### Scope
- Move from single-focus UI to `Portfolio Grid + Focus View`.
- Add risk/rate/rejection visual surfaces.
- Add dynamic pane resizing with terminal compatibility fallbacks.

### Build Checklist
- [ ] Create `AppStateV2` (`portfolio`, `assets`, `strategies`, `matrix`, `focus`).
- [ ] Implement `Asset Table`, `Strategy Table`, `Risk/Rate Heatmap`, `Rejection Stream`.
- [ ] Implement Focus drill-down reusing chart/position/history widgets.
- [ ] Add resize controls (`Ctrl+D`, fallback `[`, `]`, resize mode).
- [ ] Add keymap override config (`ui.keymap.*`).
- [ ] Add panel state persistence across redraws/symbol switches.

### Validation Checklist
- [ ] Grid refresh remains stable with high event throughput.
- [ ] Focus enter/exit works without state loss.
- [ ] Resize controls work across supported terminals (fallback verified).
- [ ] `cargo test -q` passes.

### Release Exit Criteria
- [ ] Operators can identify rejection source within 1 second from UI.
- [ ] No regression in manual buy/sell flow from legacy UI.

---

## `1.0.0` Checklist (Major)

### Scope
- Make multi-strategy + one-risk-module architecture the default path.
- Remove legacy single-path assumptions.
- Finalize operational SLO, docs, and migration notes.

### Build Checklist
- [ ] Legacy direct-flow code paths removed or fully gated off.
- [ ] RiskModule and UI contracts frozen (`v1` semantics).
- [ ] Backward-compatible config migration tooling added.
- [ ] Add release notes template and operator runbook updates.
- [ ] Document known limits and terminal compatibility matrix.

### Validation Checklist
- [ ] End-to-end soak test (>=24h) with multi-strategy/multi-asset profile.
- [ ] API rate-limit violation rate below agreed threshold.
- [ ] UI render latency and event lag within SLO.
- [ ] Failure-injection scenarios (network drop, stale balance, 429) pass.
- [ ] `cargo test -q` passes.

### Release Exit Criteria
- [ ] `1.0.0` migration guide published.
- [ ] On-call/ops checklist signed off.
- [ ] Tag + changelog + release artifact verified.
