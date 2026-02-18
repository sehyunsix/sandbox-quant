# RFC 0017: Logging Consistency and Traceability System

- Status: Draft
- Author: sandbox-quant
- Date: 2026-02-18
- Related:
  - `src/main.rs`
  - `src/binance/ws.rs`
  - `src/order_manager.rs`
  - `src/ui/mod.rs`
  - `docs/rfcs/0013-grid-network-tab-and-latency-observability.md`

## 1. Problem Statement

Current logs are useful but inconsistent:

- mixed message styles (`WebSocket Connected` vs `WebSocket connected`)
- mixed severity semantics (`[WARN]` embedded in free text)
- missing stable identifiers to trace one action across modules
- duplicate status lines from multiple producers
- difficult root-cause analysis for reconnect/order/PnL issues

This makes incident triage slow and error-prone.

## 2. Goals

- define one canonical log schema for runtime events
- provide end-to-end traceability for critical flows (signal -> risk -> submit -> fill)
- make UI/system-log rendering deterministic and deduplicated
- keep implementation incremental without breaking current UX

## 3. Non-Goals

- replacing all logging backend infrastructure in one PR
- introducing external observability stack (ELK/Datadog/etc.) in this RFC
- retroactively rewriting all historical log files

## 4. Proposal Summary

Introduce a structured logging contract with explicit event envelopes:

1. Canonical fields
- `ts_ms`: event timestamp (u64)
- `level`: `DEBUG | INFO | WARN | ERROR`
- `domain`: `ws | strategy | risk | order | portfolio | ui | system`
- `event`: stable event code (e.g. `ws.reconnect.scheduled`)
- `symbol`: normalized instrument label when applicable
- `strategy_tag`: source tag when applicable (`cfg`, `fst`, `c07`, `mnl`)
- `trace_id`: cross-module correlation id
- `msg`: short human-readable summary
- `kv`: optional structured key-values

2. Correlation policy
- create `trace_id` at signal creation
- preserve same `trace_id` through risk evaluation, submit, fill/reject, history sync
- map `trace_id` to `intent_id` and `client_order_id` in order path

3. Producer rules
- backend modules emit structured records only
- UI translates structured record to rendered line format
- prohibit ad-hoc inline prefixes like `\"[WARN] ...\"`

4. Rendering rules (System Log tab)
- one-line compact format:
  - `HH:MM:SS.mmm | LEVEL | domain.event | symbol | strategy_tag | msg`
- configurable dedup window (default 1s) for identical status spam
- keep last N records in memory with stable ordering by `ts_ms`

5. Standards alignment
- internal schema is OTel-compatible by design (`timestamp`, `severity`, `trace_id`, attributes)
- maintain an adapter layer for sink-specific formats (OTel/ECS/Cloud vendor)
- avoid backend lock-in by keeping domain/event taxonomy independent from transport

## 5. Event Taxonomy (Initial Set)

- WebSocket
  - `ws.connect.start`
  - `ws.connect.ok`
  - `ws.connect.fail`
  - `ws.reconnect.scheduled`
  - `ws.worker.retired`
  - `ws.tick.dropped`
- Strategy/Risk
  - `strategy.signal.emit`
  - `risk.eval.approved`
  - `risk.eval.rejected`
- Order
  - `order.submit.sent`
  - `order.submit.accepted`
  - `order.fill.received`
  - `order.reject.received`
  - `order.history.sync`
- Portfolio/UI
  - `portfolio.pnl.snapshot`
  - `ui.tab.change`
  - `ui.popup.open`
  - `ui.popup.close`

## 6. Data Model Changes

Add shared log record type:

```rust
struct LogRecord {
    ts_ms: u64,
    level: LogLevel,
    domain: LogDomain,
    event: &'static str,
    symbol: Option<String>,
    strategy_tag: Option<String>,
    trace_id: Option<String>,
    msg: String,
    kv: Vec<(String, String)>,
}
```

Service metadata (required):
- `service.name`
- `service.version`
- `deployment.environment`

Compatibility mapping:
- OTel fields: `trace_id`, `span_id`, `severity_text`, `attributes`
- ECS fields: `trace.id`, `span.id`, `event.action`, `log.level`

App event extension:
- add `AppEvent::LogRecord(LogRecord)`
- keep `AppEvent::LogMessage(String)` temporarily for backward compatibility

## 7. Migration Plan

Phase 1: Contract and adapters
1. add `LogRecord` types and formatting helpers
2. add compatibility adapter from legacy text logs to `LogRecord`
3. route new producers to `LogRecord` first in WS/order paths

Phase 2: Correlation rollout
1. generate and propagate `trace_id` in signal/order flow
2. add trace mapping to order lifecycle logs

Phase 3: UI dedup and filtering
1. render System Log from structured records
2. implement dedup window for status noise
3. add optional filters (`domain`, `level`, `symbol`)
4. apply label-cardinality policy:
   - high-cardinality ids (`trace_id`, `intent_id`, `client_order_id`) stay in payload, not index labels

Phase 4: Legacy cleanup
1. remove direct string-prefixed severity usage
2. deprecate `AppEvent::LogMessage(String)` once migration completes

Phase 5: Export adapters
1. add JSON emitter compatible with `tracing-subscriber`
2. add OTel log exporter adapter (optional runtime feature)
3. add ECS mapping adapter for external search backends

## 8. Acceptance Criteria

- all new WS/order/strategy logs follow canonical fields
- signal->fill/reject flow can be traced by `trace_id` in one search
- duplicate status spam reduced by dedup policy without losing key state transitions
- System Log tab remains readable under high event throughput
- no regression in existing runtime behavior and tests

## 9. Risks and Mitigations

- Risk: migration complexity during active feature work
  - Mitigation: adapter-based phased rollout, one domain at a time

- Risk: extra log volume
  - Mitigation: level controls + dedup window + bounded in-memory ring buffer

- Risk: incomplete correlation propagation
  - Mitigation: enforce trace_id-required checks in submit/fill code paths

## 10. Open Questions

1. Should `trace_id` be UUIDv4 or short sortable id (ULID)?
2. Should dedup happen only in UI, or also at producer side?
3. Do we persist structured logs to SQLite for post-mortem analysis?

## 11. References

- OpenTelemetry Logs Data Model
  - https://opentelemetry.io/docs/specs/otel/logs/data-model/
- W3C Trace Context
  - https://www.w3.org/TR/trace-context/
- Rust `tracing`
  - https://docs.rs/tracing/
- `tracing-subscriber` JSON formatting
  - https://docs.rs/tracing-subscriber/latest/tracing_subscriber/fmt/format/struct.Json.html
- Elastic Common Schema (ECS) tracing fields
  - https://www.elastic.co/guide/en/ecs/current/ecs-tracing.html
- Datadog log/trace correlation guidance
  - https://docs.datadoghq.com/tracing/other_telemetry/connect_logs_and_traces/
- Grafana Loki label cardinality guidance
  - https://grafana.com/docs/loki/latest/get-started/labels/cardinality/
