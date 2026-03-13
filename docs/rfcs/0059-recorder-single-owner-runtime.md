# RFC 0059: Recorder Single-Owner Runtime And Coordination

## Status
Draft

## Summary
The recorder path must have one meaning.

`sandbox-quant-recorder` is the recorder runtime. It should not coexist with a second process-manager/status-file ownership model for the same lifecycle.

This RFC removes the old split-brain design and defines three separate concerns:

1. `RecorderTerminal`
   - interactive foreground recorder
   - owns one in-process `MarketDataRecorder`
2. `MarketDataRecorder`
   - websocket ingestion + DuckDB writes + live worker health
3. `RecorderCoordination`
   - lightweight shared file-based coordination for `strategy_symbols`
   - no process spawning
   - no pid or status-file ownership

## Problem

The previous implementation mixed two incompatible models:

- foreground in-process recorder terminal
- background process manager with config/status files and spawn logic

That created repeated failure modes:

- stale status files looked like live state
- `/status` and `/start` had different sources of truth
- DB lock/read behavior made live metrics appear as zero
- process ownership was ambiguous

## Decision

The recorder flow is reduced to a single ownership path.

### Interactive flow

```text
sandbox-quant-recorder
  -> RecorderTerminal
     -> MarketDataRecorder::start/update/status/stop
        -> worker thread
           -> Binance streams
           -> DuckDB writes
           -> in-memory heartbeat + metrics snapshot
```

### Shared coordination flow

```text
OperatorTerminal strategy watch changes
  -> RecorderCoordination::sync_strategy_symbols(mode, symbols)

RecorderTerminal /start or /status
  -> RecorderCoordination::strategy_symbols(mode)
  -> MarketDataRecorder::update_strategy_symbols(...)
```

### Explicit non-goals

- no recorder process spawn from operator shell
- no recorder process spawn from recorder interactive shell
- no status-file-based truth for interactive recorder health

## Module ownership

### Keep

- [`src/recorder_app/runtime.rs`](/Users/yuksehyun/project/sandbox-quant/src/recorder_app/runtime.rs)
  - recorder runtime + worker lifecycle
- [`src/recorder_app/terminal.rs`](/Users/yuksehyun/project/sandbox-quant/src/recorder_app/terminal.rs)
  - recorder terminal adapter
- [`src/record/coordination.rs`](/Users/yuksehyun/project/sandbox-quant/src/record/coordination.rs)
  - shared strategy symbol coordination

### Remove

- old process-manager/status-file lifecycle logic
- spawn-based recorder ownership

## Live status rules

`/status` must reflect in-process worker truth.

The status payload should be built from:

- in-memory heartbeat timestamp
- in-memory last error
- in-memory counters updated by the worker
- current manual symbols
- current coordinated strategy symbols

It must not require opening a second DuckDB connection just to render live status.

## Consequences

Positive:

- one clear source of truth
- fewer lock conflicts
- live status matches actual worker health
- operator and recorder stay decoupled

Trade-offs:

- detached daemon management is not part of this interactive path
- long-running background supervision, if ever reintroduced, must be a separate design

## Validation

Completion requires:

1. recorder starts from `sandbox-quant-recorder`
2. `/status` reflects a live heartbeat
3. counters keep increasing while data is flowing
4. this is observed continuously for at least 3 minutes
