# RFC 0044: Headless Diagnostics CLI and Verifiable Runtime Workflow

- Status: Proposed
- Author: Codex (GPT-5) + project maintainer
- Date: 2026-03-03
- Related: 0017, 0019, 0023, 0043

## 1. Background

Current troubleshooting flow is heavily TUI-centric.
When issues occur (auth failures, missing positions, incorrect PnL, stale history), diagnosis depends on:

- manual tab switching in TUI,
- visual inspection by a human,
- repeated back-and-forth to confirm state.

This creates operational friction, especially when:

- the runtime cannot open a TUI,
- we want deterministic checks in CI,
- an agent needs machine-readable diagnostics.

## 2. Problem Statement

The system lacks a first-class, read-only CLI diagnostics surface that can:

- validate exchange auth and environment alignment,
- fetch and normalize live positions,
- explain PnL computation inputs/outputs,
- summarize order-history sync health,
- run once in headless mode for reproducible debugging.

Without this, issue triage is slower and less reliable.

## 3. Goals

1. Provide a `doctor` CLI namespace for operational diagnostics.
2. Make critical runtime checks available without TUI.
3. Support both human-friendly and machine-readable (`--json`) output.
4. Reuse core runtime logic where possible to avoid divergent behavior.
5. Keep diagnostics read-only by default.

## 4. Non-goals

1. Replacing the TUI as the primary trading UX.
2. Adding order-execution flows to diagnostics.
3. Building a remote daemon/service in this RFC.

## 5. Proposal

Add a `sandbox-quant doctor ...` command family.

### 5.1 Command Set (Phase 1)

- `sandbox-quant doctor auth`
  - Validate signed endpoint auth.
  - Print endpoint hosts, credential length hints, and failure class (`-1022`, `-1021`, permission, network).

- `sandbox-quant doctor positions --market futures [--symbol BTCUSDT] [--json]`
  - Fetch live futures positions from exchange (`positionRisk` or equivalent).
  - Show normalized symbol, side, qty, entry, mark, unrealized.

- `sandbox-quant doctor pnl --market futures [--symbol BTCUSDT] [--json]`
  - Show PnL derivation per position:
    - API unrealized value,
    - fallback recomputation,
    - final selected value,
    - field-level source and confidence.

### 5.2 Command Set (Phase 2)

- `sandbox-quant doctor history --symbol BTCUSDT [--market spot|futures] [--json]`
  - Show allOrders/myTrades fetch status, counts, latency, latest timestamps.

- `sandbox-quant doctor sync --once [--json]`
  - Execute one headless sync cycle equivalent to the periodic loop.
  - Emit aggregate status for balances, history, positions, and asset snapshot composition.

## 6. CLI Output Contract

### 6.1 Human Output

- concise table/summary,
- explicit status line (`OK`, `WARN`, `FAIL`),
- next-action hint on failures.

### 6.2 JSON Output

`--json` must return stable keys for automation.

Example top-level envelope:

```json
{
  "status": "ok|warn|fail",
  "timestamp_ms": 0,
  "command": "doctor positions",
  "data": {},
  "errors": []
}
```

## 7. Architecture and Reuse

1. Introduce a diagnostics service layer (read-only) in core runtime path.
2. Move normalization logic (symbol/market/PnL field selection) into reusable functions.
3. Keep TUI and CLI both consuming the same normalized data model.
4. Avoid duplicating exchange-request formatting/signing logic.

## 8. Security and Safety

1. Diagnostics commands are read-only by default.
2. Never print raw secrets; allow only length/fingerprint hints.
3. Keep credential-related messages actionable but non-sensitive.
4. If future write commands are added, require explicit opt-in flags.

## 9. Observability

1. Each doctor command logs a structured start/end event.
2. Include latency and endpoint outcome metadata.
3. Distinguish transport failures from exchange business errors.

## 10. Acceptance Criteria

1. A maintainer can diagnose `-1022` without opening TUI.
2. Live futures positions and non-zero unrealized PnL can be verified via CLI.
3. CI can run at least one doctor command in mock/stub mode with deterministic JSON.
4. Existing TUI behavior remains unchanged except for shared bug fixes from reused logic.

## 11. Rollout Plan

### Phase 1

- Implement `doctor auth`, `doctor positions`, `doctor pnl`.
- Add JSON schema tests for output envelope.
- Add docs in README operational section.

### Phase 2

- Implement `doctor history`, `doctor sync --once`.
- Add regression tests for edge cases (missing mark price, empty history, symbol mismatch).

### Phase 3

- Add CI workflow jobs for headless diagnostics smoke checks.

## 12. Risks and Mitigations

1. Risk: Logic drift between TUI and CLI.
   - Mitigation: shared service module and shared tests.

2. Risk: Rate-limit pressure from additional diagnostics calls.
   - Mitigation: explicit command boundaries and optional throttling.

3. Risk: Operator confusion from multiple sources of truth.
   - Mitigation: annotate each field with source (`exchange`, `recomputed`, `cached`).

## 13. Open Questions

1. Should diagnostics support a fully offline replay mode from persisted snapshots?
2. Should `doctor sync --once` include predictor metric health in Phase 2 or Phase 3?
3. Should we expose a small `--watch` mode for repeated non-TUI monitoring?

## 14. Decision

If approved, implementation should begin with Phase 1 in a separate feature branch,
with tests under `tests/` and JSON output fixtures committed for stability.
