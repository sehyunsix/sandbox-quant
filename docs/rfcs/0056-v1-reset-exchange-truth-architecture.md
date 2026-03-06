# RFC 0056: Sandbox Quant v1.0.0 Reset with Exchange-Truth Architecture

## Status
Draft

## Summary
This RFC proposes a full reset for `sandbox-quant` toward `v1.0.0`.

The current codebase has accumulated too much ambiguity around position truth, local ledger ownership, close-all semantics, and runtime orchestration. Instead of incrementally patching the existing design, `v1.0.0` should restart from a simpler architecture where:

- the exchange is the only source of truth for account and position state,
- local storage is analytics- and UI-oriented only,
- execution is target-state based rather than signal-state based,
- emergency and close-all flows are exchange-truth driven,
- readability and module boundaries are treated as hard architecture constraints.

This RFC is intended as the design discussion baseline before implementation begins.

## Decision Summary
The following decisions are currently preferred for `v1.0.0`.

### Product Scope
- `v1.0.0` is a trading-core release, not a strategy release.
- Strategy engine, predictor, auto-trading, and backtest are out of scope for the first cut.
- The first cut focuses on account sync, position sync, order/trade sync, reconciliation, manual execution, `close-symbol`, and `close-all`.

### Truth and State Ownership
- The exchange is the only source of truth for executable current state.
- Local storage is not authoritative for current positions or balances.
- The authoritative current-state core is:
  - account,
  - positions,
  - open orders.

### Position Representation
- Canonical internal position representation is `signed quantity`.
- Positive = long, negative = short, zero = flat.
- `side + abs_qty` is a derived presentation form only.

### Strategy Output Model
- The standard strategy output model is `target exposure`.
- The preferred initial exposure range is `-1.0..=1.0`.
- The preferred initial interpretation baseline is total account equity.

### Close Behavior
- `close-symbol` is a close-submit primitive.
- `close-all` is a thin batch coordinator over repeated `close-symbol` submissions.
- Default close behavior is submit-oriented, not managed-to-flat.
- `managed-to-flat` is treated as a possible future supervisory feature.

### Reconciliation and Staleness
- Use stream-first updates with adaptive periodic REST snapshot reconciliation.
- Track market-data staleness separately from account/position staleness and reconciliation staleness.
- On authoritative conflict, exchange data overwrites local cache.
- In stale mode, opening should degrade or block, while closing should remain allowed.

### Error Handling
- Execution-critical paths use typed errors.
- `Result<T, String>` is disallowed in domain and application service boundaries.
- `anyhow` is allowed only at application edges, tests, and developer tooling.
- Major error types must expose classification metadata such as code, severity, and retryability.

### Storage
- Persist event log, audit/execution records, structured errors, and health events.
- UI preferences and read-only caches may be persisted as non-authoritative data.
- Current executable balances and positions must not be persisted as authoritative truth.

### External API Testing
- External API tests are layered into:
  - specification tests,
  - request construction tests,
  - error mapping tests,
  - opt-in testnet integration tests,
  - optional live smoke tests.
- Documentation-backed contract validation is required.
- Rate-limit behavior should be tested safely without intentional endpoint flooding.

### Testability
- Core execution and reconciliation logic must be testable without UI panels.
- UI is a consumer of view models, not the home of business logic.

### Readability and Structure
- Rust source files have a 500-line hard cap.
- Files above 350 lines are split candidates.
- `main.rs` and `lib.rs` should remain orchestration-focused.
- Comments should explain intent and constraints.
- Examples should clarify behavior.
- Tests should prove correctness.

## Background
The current design has several systemic failure modes:

1. Local reconstructed position state can diverge from real exchange state.
2. Close paths may use derived or stale quantities instead of actual exchange quantities.
3. The runtime loop mixes orchestration, reconciliation, execution, UI updates, and recovery logic.
4. Heavy sync tasks can block runtime responsiveness and make the UI appear frozen.
5. The codebase is difficult to reason about because ownership of truth and mutation rights is unclear.

These are not isolated bugs. They are architecture-level problems.

## Problem Statement
The current system does not provide a reliable answer to these basic questions:

- What is the authoritative current position?
- Which component is allowed to mutate it?
- Which data is for execution safety versus analytics convenience?
- What guarantees does `close-all` provide?
- How does the system recover after restart, lag, or user stream loss?

Without explicit answers, execution safety and maintainability both degrade.

## Goals
- Establish a clean `v1.0.0` architecture from first principles.
- Make the exchange the only source of truth for executable state.
- Redesign `close-all` and emergency exit around exchange-reported positions.
- Separate execution, reconciliation, strategy, storage, and UI responsibilities.
- Make code readability a hard design rule, not a style preference.
- Enforce file-size and module-boundary constraints in CI.

## Non-Goals
- Preserve backward compatibility with the current internal architecture.
- Reuse the existing local ledger as an execution source of truth.
- Carry all current features into `v1.0.0`.
- Optimize for feature breadth before execution correctness.

## Design Principles
### 1. Exchange Truth First
Executable state must come from exchange snapshots and exchange event streams.

Examples:

- Futures open positions come from exchange position endpoints and user data events.
- Spot executable balances come from account balances and order/trade events.
- Local derived state may assist the UI, but must never override exchange-truth state for execution.

### 2. Intent and Fact Must Be Separate
Commands express desired actions.
Events express facts that have already happened.

Examples:

- `CloseAllRequested` is a command/intention.
- `PositionSnapshotUpdated` is a fact.
- `OrderRejected` is a fact.

This separation makes behavior easier to trace and reason about.

### 3. Target-State Execution
Strategies should produce desired exposure or target position, not raw buy/sell impulses.

Execution should compare:

- current exchange-truth state
- desired target state

and derive the required orders from the difference.

### 4. Local Storage Is Not Truth
Local databases and in-memory ledgers should be used only for:

- event log,
- analytics,
- UI cache,
- historical reporting.

They must not be used as the final authority for close quantity, position direction, or executable balance.

### 5. Readability Is a Product Requirement
Code organization must make it obvious:

- where truth comes from,
- where state changes happen,
- which module owns each responsibility,
- how close and sync flows work.

## Proposed Architecture
The `v1.0.0` system should be split into the following subsystems.

### 1. Exchange Adapter
Responsibility:

- wrap Binance REST and WebSocket interfaces,
- expose normalized account, position, order, and trade APIs,
- expose symbol rules and normalization helpers,
- shield higher layers from exchange-specific payloads.

Examples of concerns:

- `load_open_positions`
- `load_account_balances`
- `submit_market_order`
- `submit_reduce_only_close`
- `load_symbol_order_rules`

### 2. Portfolio State Store
Responsibility:

- hold the latest authoritative runtime cache of exchange-truth state,
- apply snapshots and stream deltas,
- expose read-only current account and position views,
- centralize mutation rights.

Rules:

- all executable current-state reads flow through this store,
- no other subsystem may directly own authoritative position state.

### 3. Reconciliation Engine
Responsibility:

- consume user stream deltas,
- periodically refresh from REST snapshots,
- repair drift after reconnect or missed events,
- overwrite stale local cache with exchange truth.

Rules:

- reconciliation resolves ambiguity in favor of exchange data,
- local reconstructed history never wins against exchange snapshots.

### 4. Execution Engine
Responsibility:

- receive high-level commands such as `close-all`, `close-symbol`, and `set-target-position`,
- compare current position and target position,
- build orders,
- validate quantity against exchange rules,
- submit and track orders.

Rules:

- execution does not depend on local historical reconstruction,
- emergency close uses live exchange-truth quantities,
- close-all must be safe by construction.

### 5. Strategy Engine
Responsibility:

- process market data,
- produce target exposure or target position outputs,
- remain independent from exchange transport details.

Rules:

- strategies do not place orders directly,
- strategies do not own current authoritative position state.

### 6. Storage Layer
Responsibility:

- persist event logs,
- persist analytics data,
- persist optional UI-friendly history.

Rules:

- storage is for replay, analytics, reporting, and debugging,
- storage is not consulted as the final truth for immediate execution safety.

### 7. UI Layer
Responsibility:

- render current state and logs,
- dispatch user commands,
- avoid embedding execution logic.

Rules:

- UI reads from the store,
- UI sends commands to application/execution services,
- UI never mutates authoritative position state directly.

## Position and State Model
The system should explicitly distinguish these state categories:

### 1. Authoritative State
Directly sourced from exchange:

- balances,
- open positions,
- open orders,
- latest order/trade status.

### 2. Derived Runtime State
Computed from authoritative state:

- net exposure,
- unrealized PnL views,
- close-all progress,
- strategy-target diff.

### 3. Historical/Analytical State
Used for reporting and debugging:

- event logs,
- trade history summaries,
- analytics aggregates,
- UI history rows.

Only category 1 may drive executable close/open decisions.

### Canonical Position Representation
The preferred canonical internal representation for positions is `signed quantity`.

Recommended rule:

- positive quantity = long,
- negative quantity = short,
- zero quantity = flat.

`side + absolute quantity` may still be used as a derived presentation form, but it should not be the primary internal truth representation.

Reason:

- target-delta calculations are simpler,
- reconciliation math is simpler,
- close-direction derivation is simpler,
- flat-state invariants are less ambiguous,
- projected-state calculations are easier to express.

Recommended pattern:

- store `signed_qty` as the canonical position quantity,
- derive `side()`, `abs_qty()`, and `is_flat()` as helper accessors,
- use derived presentation forms in UI, logs, and exchange-facing adapters where useful.

## Close Semantics
Close behavior in `v1.0.0` should start from a simpler primitive.

The core primitive is:

- `close-symbol` submits a close order for one currently open instrument.

On top of that:

- `close-all` submits close orders for all currently open instruments in the selected authoritative snapshot.

`managed-to-flat` should not be the default close primitive in the first cut.
It should be treated as a possible future supervisory feature layered on top of close submission.

### Required Behavior
1. Load current open positions from authoritative exchange state.
2. For each non-flat target position, derive the exact signed quantity to close.
3. Submit the opposite-side close order using exchange rules.
4. Use `reduce-only` when supported and appropriate.
5. Return structured per-symbol submission results.
6. Let reconciliation update the post-submit state separately.

### Forbidden Behavior
- Do not use strategy order size as close quantity.
- Do not rely on local reconstructed history as the final close quantity.
- Do not infer close direction from stale local state when exchange truth is available.

### Futures Rule
Futures close logic should be built from exchange-reported position quantity and side.

### Spot Rule
Spot close logic should be built from authoritative free base-asset balance and any required execution safety checks.

### `close-symbol` Contract
`close-symbol` should be a close-submit primitive.

Its contract is:

- find the authoritative current position for the instrument,
- derive the close order,
- submit the close order,
- return the structured submit result.

It should not, by default, guarantee that the position is flat after submission.

### `close-all` Contract
`close-all` should be a thin batch coordinator over repeated `close-symbol` submissions.

Its contract is:

- take an authoritative starting snapshot,
- determine the currently open instrument set,
- submit close orders for each instrument,
- return per-symbol submit outcomes and a batch summary.

It should not, by default, behave as a full workflow engine.

### Result Model
The preferred initial result model is simple and submission-oriented.

Per symbol, the result should classify outcomes such as:

- `submitted`,
- `rejected`,
- `skipped_no_position`.

This keeps the first cut operationally clear without forcing a heavy close-state machine into the initial architecture.

### Future Extension
`managed-to-flat` remains a valid future feature, but should be implemented as an additional supervisory layer above close submission rather than as the default meaning of every close command.

## Execution Model
The preferred execution model for `v1.0.0` is:

- strategies emit target exposure or target position,
- execution computes the delta from current authoritative position,
- order planning produces one or more safe exchange orders,
- reconciliation confirms the resulting actual state.

This is preferred over a pure `buy/sell signal` model because it is more compatible with:

- restart recovery,
- close-all semantics,
- portfolio-aware control,
- partial fills,
- external/manual intervention.

## Sync Model
The preferred sync model is:

- user stream first,
- REST reconciliation fallback,
- periodic snapshot repair,
- overwrite-on-authoritative-conflict.

Expected behavior:

1. Fast updates come from stream events.
2. Missing or stale streams are repaired through REST.
3. Long-lived drift is not tolerated.
4. UI freshness should be based on store update timestamps, not best-effort local reconstructions.

## Reconciliation and Staleness Policy
`v1.0.0` should use a stream-first model with periodic snapshot reconciliation.

This is the recommended middle path between:

- overly optimistic stream-only state management,
- overly expensive snapshot-driven execution on every step.

### Recommended Model
The preferred combination is:

1. user stream first for low-latency updates,
2. periodic REST snapshot reconciliation in the background,
3. stricter reconciliation behavior during high-risk commands,
4. exchange overwrite on authoritative conflict,
5. degraded execution mode when state freshness cannot be trusted.

### Staleness Categories
Staleness should be tracked separately for at least these domains:

#### 1. Market Data Staleness
Used to determine whether price-driven logic is trustworthy.

#### 2. Account and Position Stream Staleness
Used to determine whether balances, positions, and order state are trustworthy for execution.

#### 3. Reconciliation Staleness
Used to determine whether a recent authoritative snapshot comparison has happened.

These categories must not be collapsed into one generic stale flag.

### Recommended Initial Thresholds
Initial thresholds for `v1.0.0` should be conservative but simple:

- market data stale threshold: `3s`,
- account/position stream stale threshold: `5s`,
- healthy snapshot reconciliation cadence: `10s`,
- stale-suspected snapshot cadence: `2s`,
- `close-all` in-progress reconciliation cadence: `1s`.

These values may later become configurable, but should begin as fixed defaults.

### Snapshot Policy
The system should not wait for stream failure before reconciling.

Recommended behavior:

1. When streams are healthy, reconcile periodically at a relaxed cadence.
2. When stream freshness is suspect, tighten snapshot cadence.
3. After reconnect, force a full authoritative snapshot refresh.
4. After high-risk commands, require explicit reconciliation before declaring completion.

This prevents silent drift from persisting for long periods.

### Stale-State Behavior
When state freshness cannot be trusted, the system should enter a degraded mode.

Recommended degraded behavior:

- allow close and reduce actions,
- restrict or block new position opening,
- surface the stale condition clearly in logs and UI,
- continue active reconciliation attempts until trust is restored.

This follows the principle:

- opening is optional,
- closing is safety-critical.

### Authoritative Conflict Rule
When local cache and exchange snapshot disagree, exchange data must win.

Required behavior:

1. overwrite stale local authoritative cache with exchange snapshot,
2. emit a structured reconciliation-conflict event,
3. preserve logs for debugging and analytics,
4. do not continue using local reconstructed state for execution decisions.

This rule is required to keep the architecture consistent with exchange-truth ownership.

### High-Risk Command Policy
High-risk commands must use stricter reconciliation visibility than normal runtime behavior.

Examples:

- `close-all`,
- emergency close,
- panic flatten,
- manual force-close.

Recommended rule:

- submit,
- reconcile promptly,
- surface the resulting exchange state clearly,
- allow higher-level supervisory logic to decide whether retry or escalation is needed.

Order submission and post-submit reconciliation must both be visible, but the initial close command contract may remain submission-oriented.

### `close-all` and Reconciliation
Because `close-all` is high-risk, reconciliation after submission is still important even if the command contract is submit-oriented.

Required behavior:

1. take an authoritative starting snapshot,
2. define the batch target set,
3. submit close attempts,
4. reconcile against exchange state,
5. surface the updated resulting state to operators and logs.

### Execution Policy Under Staleness
Recommended default policy:

- stale market data may pause strategy evaluation,
- stale account/position state should degrade or block new open-position execution,
- stale account/position state should still allow safety-oriented close flows,
- execution success should not be declared without authoritative post-action verification for high-risk commands.

### Recommendation
Adopt the following default `v1.0.0` policy:

- stream-first updates,
- adaptive periodic snapshot reconciliation,
- fixed stale thresholds with stricter high-risk overrides,
- degraded mode when state trust is low,
- exchange overwrite on conflict,
- prompt post-submit reconciliation for `close-all`.

## Error Handling Architecture
Error handling in `v1.0.0` must be redesigned as a first-class architecture concern.

The current `anyhow + String` style is too weak for an execution-sensitive system because it obscures:

- which layer failed,
- whether the error is retryable,
- whether the error is user-actionable,
- whether the error is fatal,
- whether the error should trigger reconciliation, backoff, or stop.

### Core Rules
1. Execution-critical paths must use typed errors.
2. `Result<T, String>` is disallowed in domain and application services.
3. `anyhow` is allowed only at application edges, test utilities, and developer tooling.
4. User-facing messages must be derived from structured errors, not used as the primary error representation.
5. All major errors must expose stable classification metadata.

### Error Layers
The system should define explicit error families by responsibility.

#### 1. Exchange Errors
Examples:

- network timeout,
- authentication failure,
- rate limit,
- invalid exchange payload,
- remote order reject,
- symbol rule lookup failure.

These should map to an `ExchangeError`.

#### 2. Execution Errors
Examples:

- close quantity too small,
- position not found,
- reduce-only requirement mismatch,
- invalid execution plan,
- state mismatch between target and current snapshot,
- order submission failure.

These should map to an `ExecutionError`.

#### 3. Sync Errors
Examples:

- user stream disconnected,
- snapshot fetch failed,
- stale state detected,
- reconciliation conflict,
- replay gap or drift beyond tolerance.

These should map to a `SyncError`.

#### 4. Storage Errors
Examples:

- sqlite failure,
- file I/O failure,
- serialization failure,
- event-log append failure.

These should map to a `StorageError`.

#### 5. Strategy Errors
Examples:

- invalid target output,
- unsupported strategy configuration,
- insufficient input state for evaluation.

These should map to a `StrategyError`.

#### 6. UI Errors
Examples:

- invalid user command,
- render degradation,
- projection formatting failure.

These should map to a `UiError`.

#### 7. Application Errors
The top-level application may aggregate subsystem errors into an `AppError`, but only after lower layers have already classified them precisely.

### Required Metadata
Each major error type should expose metadata that supports operations and policy decisions.

Required fields or equivalent trait methods:

- stable `error_code`,
- `severity`,
- `is_retryable`,
- `is_user_actionable`,
- `is_fatal`.

This metadata is more important than the display string because it drives:

- retry policy,
- operator visibility,
- UI presentation,
- alerting,
- shutdown or recovery behavior.

### `anyhow` Usage Boundary
`anyhow` should be treated as an edge-only convenience, not as the internal error model.

#### Allowed
- CLI entrypoints,
- top-level application bootstrap,
- one-shot tooling,
- test helpers,
- developer scripts.

#### Not Allowed
- exchange adapter public APIs,
- portfolio state store,
- reconciliation engine,
- execution engine,
- `close-all` and emergency close paths.

### `String` Error Ban
`Result<T, String>` must be considered a design failure in `v1.0.0` service boundaries.

Reason:

- strings do not encode classification,
- strings do not support reliable branching behavior,
- strings are poor inputs to retry logic and UI policy,
- strings encourage hidden coupling to message text.

### Error Reporting Model
Internal services should return typed errors.
Only at the application/UI/logging boundary should errors be converted into structured reports.

An error report should contain at least:

- error code,
- severity,
- concise user-facing message,
- optional diagnostic detail,
- retryability,
- user-actionability.

This keeps internal logic precise while still allowing clean logs and UI output.

### High-Risk Flow Classification
Some workflows require extra-specific errors because generic failures are not sufficient for safe operation.

`close-all` in particular should define dedicated error cases such as:

- snapshot unavailable,
- no open positions,
- close quantity too small,
- close submit rejected,
- partial close failure,
- reconciliation failed after close.

These should not be collapsed into generic text errors.

### Recommendation
Use `thiserror`-based typed enums for subsystem errors, plus a shared metadata trait or equivalent classification layer.

The architecture should explicitly prefer:

- typed error families per subsystem,
- stable machine-readable error codes,
- edge-only `anyhow`,
- no `String` errors in service boundaries.

## Storage Policy
Storage in `v1.0.0` should exist to preserve history, auditability, and UI convenience.
It must not become a shadow source of truth for execution-critical current state.

### Core Rule
Persist history.
Do not persist authoritative current trading truth as an execution dependency.

The system should restart by reconciling current state from the exchange, not by trusting a local ledger as final truth.

### Must Persist
The following categories should be persisted because they are important for operations, debugging, and auditability:

#### 1. Event Log
Examples:

- command received,
- close-all job started,
- order submitted,
- order rejected,
- fill observed,
- stale mode entered,
- reconciliation conflict detected,
- reconnect completed.

#### 2. Execution and Audit Records
Examples:

- who initiated a command,
- when a command was initiated,
- what execution plan was derived,
- what exchange requests were attempted,
- what terminal execution outcome was observed.

#### 3. Structured Error Records
Persist structured error information, including:

- error code,
- severity,
- retryability classification,
- relevant context payloads.

#### 4. System Health Events
Examples:

- stream disconnect,
- snapshot sync failure,
- degraded mode entered,
- degraded mode cleared,
- reconciliation backlog warnings.

### May Persist
The following may be persisted for convenience, performance, or user experience, but they must remain non-authoritative.

#### 1. UI Cache and Preferences
Examples:

- selected instrument,
- selected tab,
- sort order,
- scroll state,
- last viewed panels.

#### 2. Derived Analytics
Examples:

- session summaries,
- aggregated PnL views,
- derived performance counters,
- non-authoritative operational summaries.

#### 3. Read-Only History Cache
Examples:

- recently displayed order history,
- recently displayed trade history,
- compact summaries used to accelerate UI rendering.

#### 4. Completed Job Summaries
Examples:

- close-all result summaries,
- manual close job outcomes,
- reconciliation repair summaries.

### Must Not Persist as Authoritative Truth
The following categories must not be treated as the final source of truth for execution-critical decisions:

#### 1. Current Executable Position State
Examples:

- current signed position quantity,
- current long/short direction,
- current open-position list.

These must come from exchange-truth reconciliation.

#### 2. Current Executable Balance State
Examples:

- available quote balance,
- available base balance for spot close,
- current margin or available collateral used for execution safety.

These must come from exchange-truth reconciliation.

#### 3. Close Quantity Truth
The quantity used to close a live position must not come from a historical local ledger as final authority.

#### 4. Execution-Critical Drift Shortcuts
The system must not rely on assumptions such as:

- previous local position implies current position,
- previous local balance implies current executable balance,
- local reconstructed fills can replace current exchange state for safety-critical actions.

### Restart Rule
On startup or recovery:

1. load persisted event/audit/history data if useful,
2. reconstruct or refresh the current authoritative account state from the exchange,
3. mark local persisted runtime caches as non-authoritative,
4. allow execution only after minimum required reconciliation is complete.

### Recommendation
The initial `v1.0.0` storage footprint should remain minimal.

Recommended first-cut storage:

- append-only event log,
- execution/audit records,
- structured error records,
- close-all job history,
- minimal UI preferences.

This is sufficient for observability without reintroducing local-ledger truth problems.

## External API Test Strategy
External API testing must be treated as a dedicated architecture concern.

This system depends on third-party exchange behavior, which means correctness is not only about internal logic.
It is also about:

- request shape matching exchange documentation,
- parameter contracts being enforced correctly,
- error payloads being mapped correctly,
- rate limits being handled safely,
- test coverage avoiding accidental abusive behavior.

### Core Principles
1. External API testing must be layered.
2. Official API documentation should be translated into internal executable specifications.
3. Error mapping must validate HTTP status, exchange error code, and relevant headers together.
4. Rate-limit testing must not rely on intentionally exhausting production quotas.
5. Live network tests must be opt-in, never the default CI path.

### Test Layers
The recommended strategy is a four-layer model plus an optional live smoke layer.

#### 1. Specification Tests
These tests validate that internal exchange rules match the documented API contract.

Examples:

- required and optional parameters,
- valid parameter combinations,
- invalid parameter combinations,
- `reduceOnly` constraints,
- `positionSide` rules,
- symbol order-rule interpretation,
- spot versus futures endpoint capability differences.

These tests should be network-free and deterministic.

#### 2. Request Construction and Serialization Tests
These tests validate that the adapter builds requests exactly as intended.

Examples:

- route selection,
- query serialization,
- body serialization if used,
- signed payload generation,
- timestamp and `recvWindow` inclusion,
- omission of unsupported optional parameters,
- spot/futures request divergence where required.

These tests should also be network-free and deterministic.

#### 3. Error Mapping Tests
These tests validate that exchange failures are converted into the correct typed internal errors.

They must verify combinations of:

- HTTP status,
- exchange-specific error code,
- response body,
- relevant rate-limit or retry headers.

Examples:

- throttling,
- timestamp drift,
- invalid quantity,
- reduce-only rejection,
- insufficient position,
- invalid symbol rules.

These tests should use fixtures, not real network failures.

#### 4. Testnet Integration Tests
These tests validate that the adapter behaves correctly against official test environments.

Examples:

- authenticated connectivity,
- test order endpoints,
- symbol rule fetches,
- signature acceptance,
- timestamp offset correction,
- documented reject payload behavior,
- rate-limit header parsing.

These tests should:

- require explicit environment configuration,
- use dedicated test credentials,
- be opt-in,
- not run by default in normal local or CI flows.

#### 5. Optional Live Smoke Tests
Live smoke tests may exist, but must remain tightly scoped and manually or explicitly triggered.

They should be limited to:

- read-only endpoints,
- minimal operational checks,
- carefully bounded low-risk validation.

They must not be part of standard CI.

### Documentation-Driven Validation
The exchange adapter should not treat documentation as informal reference only.

Required practice:

1. convert important documented parameter rules into internal test cases,
2. convert important documented error codes into typed mapping tests,
3. keep endpoint-specific fixtures aligned with official docs,
4. review tests whenever official endpoint contracts change.

This turns documentation drift into a visible engineering task instead of a production surprise.

### Parameter Contract Validation
The system should explicitly validate parameter-level correctness before requests leave the process whenever possible.

Examples:

- mutually exclusive arguments,
- required field combinations,
- market-specific parameter support,
- invalid use of futures-only or spot-only options,
- client-order-id formatting expectations,
- quantity normalization preconditions.

This reduces avoidable remote rejects and makes internal failures easier to classify.

### Error and Log Consistency
External API failures should produce structured internal records, not ad hoc log strings.

Recommended captured fields include:

- endpoint identifier,
- market kind,
- HTTP status,
- exchange error code,
- retry-related headers,
- client order id if present,
- internal typed error code,
- severity.

Tests should validate structured mapping behavior first.
Human-readable log strings are secondary.

### Rate-Limit Test Policy
Rate limiting is critical and must be tested safely.

#### Required Validation
- response-header parsing,
- internal budget accounting,
- retry-after handling,
- typed error mapping for throttling,
- degraded-mode behavior under throttling,
- execution prioritization for close-oriented commands.

#### Explicit Anti-Pattern
Do not test rate limiting by intentionally flooding production or testnet endpoints until rejection.

Instead, use:

- fixture-based header tests,
- throttling simulation,
- internal budget-state tests,
- targeted opt-in integration checks.

### Test Execution Policy
Recommended defaults:

#### Always Run
- specification tests,
- request construction tests,
- error mapping tests,
- architecture and line-budget tests.

#### Opt-In
- testnet integration tests.

#### Manual or Scheduled Only
- live smoke tests.

### Recommendation
Adopt an external API testing strategy built around:

- documentation-backed executable specs,
- deterministic local adapter tests,
- structured error-mapping tests,
- opt-in testnet validation,
- safe rate-limit-aware operational checks.

## UI-Independent Testability
Core runtime behavior in `v1.0.0` must be testable without UI panels.

This is a hard architecture requirement, not just a testing preference.

If core execution or reconciliation logic can only be validated through UI rendering, that is evidence that:

- business logic is leaking into the UI layer,
- view state is being treated as execution state,
- subsystem boundaries are not strong enough.

### Core Rule
Execution-critical and reconciliation-critical behavior must be testable without launching or rendering UI panels.

### Required Testable Areas
At minimum, the following must be testable without the UI layer:

#### 1. Domain Logic
Examples:

- exposure translation,
- signed position math,
- quantity normalization,
- dust classification.

#### 2. Portfolio State and Reconciliation
Examples:

- snapshot application,
- stream delta application,
- stale-state transitions,
- exchange-overwrite conflict handling,
- open-order state updates.

#### 3. Execution Flows
Examples:

- target exposure to execution plan conversion,
- close-symbol planning,
- close-all managed-to-flat state transitions,
- symbol-level retry behavior,
- stale-mode open/close policy.

#### 4. Exchange Adapter Behavior
Examples:

- request building,
- signature behavior,
- parameter validation,
- error mapping,
- rate-limit parsing.

#### 5. Storage Behavior
Examples:

- event append,
- audit record persistence,
- restart-time reload behavior for non-authoritative caches.

### UI Testing Role
UI tests are still useful, but they must remain downstream consumers of already-validated core behavior.

Preferred UI test scope:

- view-model projection,
- command dispatch mapping,
- status rendering,
- progress and health display.

UI rendering tests should not be the primary place where close-all, reconciliation, or execution correctness is verified.

### Recommended Test Pyramid
The preferred priority order is:

1. pure domain tests,
2. state-store and state-machine tests,
3. service tests with fake exchange dependencies,
4. adapter contract tests,
5. view-model projection tests,
6. minimal UI rendering tests.

### Architectural Implication
To satisfy this requirement:

- UI must consume view models rather than own business logic,
- execution must be exposed through commands and services,
- reconciliation must be modeled independently of rendering,
- panel state must not be the place where execution truth is stored.

### Recommendation
Adopt the following principle explicitly:

`If a core behavior cannot be tested without the UI, the design should be reconsidered before implementation continues.`

## Commenting Policy
Commenting style in `v1.0.0` should be intentional and constrained.

The goal is not to maximize comment volume.
The goal is to make important intent, invariants, constraints, and examples obvious without burying the code in narration.

### Core Rules
1. Comments should explain `why`, not restate obvious `what`.
2. Comments should be used for intent, invariants, external constraints, and non-obvious decisions.
3. Comments should not compensate for poor naming or oversized functions.
4. Safety-critical behavior and exchange-specific constraints should be commented explicitly.
5. Short examples are encouraged when they prevent ambiguity.

### Comments That Are Required
The following areas should normally have comments or doc comments.

#### 1. External API Constraints
Examples:

- why a Binance parameter combination is required or forbidden,
- why `reduceOnly` is applied,
- why timestamp correction is needed,
- why a certain rate-limit header matters.

#### 2. Safety-Critical Logic
Examples:

- why stale mode blocks opens but allows closes,
- why exchange snapshots overwrite local cache,
- why close quantity comes from exchange-truth state,
- why a given error is considered retryable or fatal.

#### 3. Core Invariants
Examples:

- canonical signed quantity representation,
- flat-state rules,
- authoritative ownership of portfolio state,
- command versus event distinction.

#### 4. Public Contracts
Public execution-, reconciliation-, and exchange-facing APIs should document:

- what they guarantee,
- what they do not guarantee,
- what inputs they assume,
- what outputs mean.

This is especially important for APIs such as:

- `close_symbol`,
- `close_all`,
- reconciliation entrypoints,
- snapshot application functions,
- execution planners.

### Comments That Should Usually Be Avoided
Avoid comments that only restate the code mechanically.

Examples of low-value comments:

- "increment retry count",
- "return false if qty is zero",
- "submit the order".

If a line is obvious from the code, a comment usually adds noise instead of clarity.

### Example Usage Guidance
Short examples are encouraged when they prevent domain ambiguity.

Examples are especially useful for:

- signed quantity interpretation,
- target exposure semantics,
- close direction derivation,
- dust classification,
- submit-only close behavior,
- exchange-specific parameter rules.

Examples should be:

- short,
- concrete,
- behavior-oriented,
- local to the relevant type or function.

Preferred style:

- a few short bullet examples in doc comments,
- or a short inline comment before a non-obvious transformation.

### Recommended Example Topics
Examples are most valuable for items such as:

- `+0.25 = long`, `-0.25 = short`, `0.0 = flat`,
- `close_symbol` on `signed_qty = -0.3` produces a buy close of `0.3`,
- `close_all` submits closes but does not itself guarantee flatness,
- exchange overwrite behavior on reconciliation conflict.

### Module-Level Comment Guidance
Top-level modules should briefly state:

- what the module owns,
- what it does not own,
- what kind of data it treats as authoritative or derived.

This is especially valuable for:

- `exchange/`,
- `portfolio/`,
- `execution/`,
- `storage/`,
- `ui/`.

### Function-Level Comment Guidance
Function comments should remain focused.

Preferred content:

- contract,
- invariants,
- non-guarantees,
- important side effects,
- important assumptions.

If a function requires long narrative comments to be understandable, the function should usually be split instead.

### Tests and Examples
Examples and tests have different roles:

- comments and examples explain intended behavior,
- tests prove behavior.

Long examples should usually become tests instead of extended doc comments.

### Recommendation
Adopt the following principle:

`Comments explain intent and constraints. Examples clarify behavior. Tests prove correctness.`

## Initial Module and Directory Layout
`v1.0.0` should begin with a deliberately small, explicit module structure.

The goal is not to predict every future feature.
The goal is to establish clear ownership boundaries so the codebase does not collapse back into mixed-responsibility files.

### Top-Level Layout
Recommended initial structure:

```text
src/
  app/
    mod.rs
    bootstrap.rs
    runtime.rs
    commands.rs

  domain/
    mod.rs
    instrument.rs
    market.rs
    position.rs
    balance.rs
    order.rs
    exposure.rs
    identifiers.rs

  error/
    mod.rs
    app_error.rs
    exchange_error.rs
    execution_error.rs
    sync_error.rs
    storage_error.rs
    ui_error.rs
    severity.rs
    error_code.rs

  exchange/
    mod.rs
    facade.rs
    types.rs
    symbol_rules.rs
    binance/
      mod.rs
      client.rs
      auth.rs
      market_data.rs
      account.rs
      orders.rs
      user_stream.rs
      mapper.rs

  portfolio/
    mod.rs
    store.rs
    snapshot.rs
    reconcile.rs
    staleness.rs

  execution/
    mod.rs
    service.rs
    command.rs
    planner.rs
    close_all.rs
    close_symbol.rs
    target_translation.rs
    spot/
      mod.rs
      planner.rs
      executor.rs
    futures/
      mod.rs
      planner.rs
      executor.rs

  storage/
    mod.rs
    event_log.rs
    audit_log.rs
    ui_prefs.rs
    models.rs

  ui/
    mod.rs
    app.rs
    view_model.rs
    commands.rs
    panels/
      mod.rs
      positions.rs
      logs.rs
      health.rs

tests/
  ...
```

### Module Responsibilities
Each top-level module should have a narrow purpose.

#### `app/`
Application wiring and orchestration only.

Allowed:

- startup sequencing,
- task wiring,
- top-level command dispatch,
- runtime bootstrap.

Not allowed:

- exchange-specific request building,
- business rule calculation,
- direct UI projection logic,
- direct order planning logic.

#### `domain/`
Core domain types and small domain rules.

Examples:

- instruments,
- markets,
- positions,
- balances,
- exposures,
- identifiers,
- normalized order-side concepts.

This layer should be mostly dependency-light.

#### `error/`
Typed error families and shared error metadata.

This module owns:

- error enums,
- error code taxonomy,
- severity classification,
- retryability and fatality metadata.

#### `exchange/`
External exchange integration boundary.

This module owns:

- HTTP/WebSocket integration,
- signing,
- request construction,
- response decoding,
- exchange-specific type mapping,
- symbol-rule loading.

This module should not own application policy such as close-all orchestration or target exposure decisions.

#### `portfolio/`
Authoritative runtime state store and reconciliation rules.

This module owns:

- authoritative state cache,
- snapshot application,
- delta application,
- staleness tracking,
- reconciliation conflict handling.

This is the central state-truth layer of the runtime.

#### `execution/`
Command-to-order planning and execution policy.

This module owns:

- execution command handling,
- target exposure translation,
- market-specific order planning,
- close-all managed-to-flat flow,
- per-market execution behavior.

This module should depend on `portfolio/` reads and `exchange/` actions, but should not become a state-truth store.

#### `storage/`
Persistence for event logs, audit records, and small non-authoritative caches.

This module must not own current trading truth.

#### `ui/`
Rendering, interaction, and view-model projection only.

This module should read application-facing state and emit user commands.
It should not plan orders or reconcile positions directly.

### Dependency Direction
Recommended high-level dependency direction:

```text
ui -> app -> execution -> exchange
                |
                -> portfolio
                -> storage

portfolio -> domain
execution -> domain
exchange -> domain
storage -> domain
error -> shared across layers
```

Practical rule:

- outer layers may depend on inner/domain layers,
- inner layers should not depend on UI or top-level app orchestration,
- `exchange` and `execution` should not depend on `ui`,
- `portfolio` should not depend on `ui`,
- `domain` should not depend on runtime orchestration modules.

### Entrypoint Constraints
`main.rs` should stay thin.

Recommended role:

- initialize config,
- initialize runtime services,
- launch app bootstrap,
- map top-level fatal errors to process exit behavior.

It should not become a place where:

- reconciliation logic lives,
- close-all state machines live,
- symbol rule logic lives,
- market-specific execution branching grows.

### File Budget Guidance
The 500-line hard limit applies to this structure from the start.

Recommended practice:

- if a file grows beyond ~350 lines, split it before it becomes crowded,
- use submodules instead of large "utils" files,
- prefer explicit names like `close_all.rs` over generic buckets like `helpers.rs`.

### Suggested First Implementation Order
Recommended implementation order for the first architecture cut:

1. `domain/`
2. `error/`
3. `exchange/`
4. `portfolio/`
5. `execution/`
6. `storage/`
7. `ui/`
8. `app/`

This order forces the runtime to sit on top of stable primitives rather than inventing them ad hoc.

## Readability and Code Organization Constraints
Readability is part of the architecture and must be enforced.

### File Size Limits
- No Rust source file may exceed `500` lines.
- Files above `350` lines are split candidates and should be reviewed immediately.
- Exceptions must be explicit and documented.

### Module Rules
- `main.rs` and `lib.rs` should remain orchestration-only.
- `mod.rs` files should primarily declare modules and lightweight exports.
- Business logic should live in domain-specific modules, not entrypoint files.

### Mutation Rules
- Authoritative portfolio state may only be mutated by the portfolio state store and reconciliation paths.
- Execution may request changes through commands and exchange submissions, but not mutate truth ad hoc.

### Naming Rules
- Prefer exact domain names over vague helpers.
- Use domain types where possible instead of raw `String` and `f64` everywhere.
- Keep one abstraction level per function.

### Testing Rules
- New tests must live in `tests/`.
- Tests should be named by behavior, not implementation detail.
- Execution-safety flows such as `close-all` must have dedicated scenario tests.

## Enforcement Strategy
The line-budget and structure rules should be enforced through build/test gates.

### Required Enforcement
1. Add a line-budget test that fails when a Rust source file exceeds `500` lines.
2. Add CI checks that require this test to pass before merge.
3. Maintain an explicit exception list if any temporary exemptions are allowed.

### Preferred Additional Enforcement
- a lightweight architecture check for forbidden dependency directions,
- a rule that prevents large business logic growth in `main.rs`,
- a documented module ownership map.

### Explicit Non-Decision
Using `build.rs` as a hard failure mechanism is possible but not preferred at this stage.
`cargo test` plus CI is the preferred enforcement path.

## Initial `v1.0.0` Scope
The first `v1.0.0` milestone should optimize for correctness and operational clarity.

`v1.0.0` should be treated as a trading-core release, not as a strategy release.

### In Scope
- exchange-truth portfolio store,
- account and balance synchronization,
- position synchronization,
- order and trade synchronization,
- reconciliation engine,
- execution engine,
- safe `close-all`,
- safe `close-symbol`,
- manual order execution,
- single-symbol manual execution,
- basic UI for positions, logs, and health,
- event logging,
- line-budget and structure enforcement.

### Out of Scope for First Cut
- strategy engine,
- complex predictor stack,
- heavy backtest surface,
- broad strategy catalog restoration,
- automated strategy trading,
- signal generation,
- advanced analytics features,
- nonessential UI expansion.

### Scope Framing
The initial release should answer these questions well:

- Is the exchange-truth account state accurate?
- Is the exchange-truth position state accurate?
- Are order and trade updates reconciled correctly?
- Does `close-all` safely flatten real positions?
- Is manual execution safe and understandable?

The initial release does not need to answer:

- Which strategy should trade?
- How should signals be generated?
- How should predictor quality be modeled?
- How should backtesting be restored?

## Migration Strategy
The preferred migration path is not incremental retrofitting inside the old design.

Recommended approach:

1. write and agree on the `v1.0.0` RFC,
2. create a new architecture skeleton,
3. implement exchange store, reconciliation, and execution first,
4. add `close-all` and manual close early,
5. restore only the minimum viable UI,
6. reintroduce additional features selectively after execution safety is proven.

The current codebase may remain temporarily available as reference, but should not dictate the `v1.0.0` shape.

## Acceptance Criteria
`v1.0.0` architecture work should not be considered complete until all of the following are true:

1. The system has a single explicit source of truth for executable state.
2. `close-all` closes real exchange positions without relying on strategy order size.
3. Runtime sync and UI update paths remain responsive during reconciliation.
4. Local storage is not used as authoritative execution state.
5. File-size and code-organization constraints are enforced in CI.
6. The responsibility boundaries between exchange, store, execution, strategy, storage, and UI are clear in code structure.

## Open Questions
These questions should be resolved before implementation starts in earnest:

1. Should strategy output be target exposure, target notional, or target quantity?
2. How much of spot and futures execution should share interfaces versus separate implementations?
3. What is the exact retry and timeout behavior for `close-all`?
4. What is the minimum viable UI for `v1.0.0`?
5. Should event sourcing be a first-class internal pattern, or only a storage concern?
6. How aggressively should REST reconciliation run when stream health degrades?

## Recommendation
Proceed with `v1.0.0` as a reset, not as an incremental cleanup.

The first implementation milestone should prioritize:

- exchange-truth state ownership,
- reconciliation clarity,
- safe close semantics,
- strong readability constraints,
- minimal but reliable operational scope.
