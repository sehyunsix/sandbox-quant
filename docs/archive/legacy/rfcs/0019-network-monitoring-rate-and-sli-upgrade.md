# RFC 0019: Network Monitoring Rate/SLI Upgrade (From Counters to Rates)

- Status: Draft
- Author: sandbox-quant
- Date: 2026-02-18
- Related:
  - `src/ui/mod.rs`
  - `docs/rfcs/0013-grid-network-tab-and-latency-observability.md`

## 1. Problem Statement

Current Network tab relies heavily on cumulative counters like `tick_drop=18957`.
This is useful for history, but weak for real-time diagnosis:

- high cumulative values do not indicate current severity
- operators cannot distinguish "past issue" vs "ongoing issue"
- hard to trigger immediate mitigation decisions from cumulative-only metrics

## 2. Goals

- add real-time rate metrics (1s/10s/60s windows)
- expose actionable SLI-style indicators for current network quality
- keep cumulative counters as secondary context

## 3. Non-Goals

- external monitoring stack integration (Prometheus/Grafana cloud, etc.)
- cross-process distributed tracing in this RFC
- replacing existing latency metrics; this RFC extends them

## 4. Proposal Summary

Upgrade Network monitoring to show both:

1. Rate metrics (primary for live operations)
2. Cumulative counters (secondary for trend/history)

## 5. New Metrics

### 5.1 Tick stream health

- `tick_in_rate_1s`
- `tick_drop_rate_1s`
- `tick_drop_rate_10s`
- `tick_drop_rate_60s`
- `tick_drop_ratio_10s = dropped / (in + dropped)`
- `tick_drop_ratio_60s`

### 5.2 Connectivity stability

- `ws_reconnect_rate_60s`
- `ws_disconnect_rate_60s`
- `heartbeat_gap_ms` (time since last tick)
- `last_tick_age_ms`

### 5.3 Order-path latency quality

- `tick_latency_p50/p95/p99`
- `fill_latency_p50/p95/p99`
- `order_sync_latency_p50/p95/p99`
- `last_order_update_age_ms`

### 5.4 Cumulative context (keep)

- `tick_drop_total`
- `ws_reconnect_total`

## 6. SLI Health Classification

Introduce health label based on thresholds:

- `OK`: low drop ratio + stable reconnect + acceptable p95 latency
- `WARN`: moderate degradation
- `CRIT`: severe ongoing degradation

Example threshold baseline:

- `tick_drop_ratio_10s >= 1%` -> `WARN`
- `tick_drop_ratio_10s >= 5%` -> `CRIT`
- `ws_reconnect_rate_60s >= 2/min` -> `WARN`
- `ws_reconnect_rate_60s >= 5/min` -> `CRIT`
- `tick_latency_p95 >= 1500ms` -> `WARN`
- `tick_latency_p95 >= 4000ms` -> `CRIT`
- `heartbeat_gap_ms >= 3000ms` -> `WARN`

## 7. UI Changes (Network Tab)

Top summary row:

- `Health`, `tick_in_rate`, `tick_drop_rate_10s`, `drop_ratio_10s`, `reconnect_rate_60s`

Middle table:

- latency p50/p95/p99 for tick/fill/order-sync

Bottom row:

- cumulative counters (`tick_drop_total`, `reconnect_total`)
- freshness (`last_tick_age_ms`, `last_order_update_age_ms`)

## 8. Implementation Plan

1. Add sliding-window bins (1s granularity, 60 bins)
2. Update event ingestion:
   - increment tick-in/drop/reconnect/disconnect buckets
3. Compute rate/ratio at render-time
4. Add percentile p99 support for existing latency samples
5. Update Network tab renderer with new layout and SLI health logic
6. Add regression tests for:
   - rate math
   - threshold classification
   - rendering fallback with sparse samples

## 9. Acceptance Criteria

- operators can see current network degradation within 1-10 seconds
- cumulative counters remain available but no longer primary diagnostic signal
- health label reflects real-time conditions, not historical accumulation only
- existing tests remain green, and new metric tests are added

## 10. Risks and Mitigations

- Risk: noisy short-window rates
  - Mitigation: show 1s + 10s + 60s together, prioritize 10s in health scoring

- Risk: increased UI complexity
  - Mitigation: concise top summary + compact tables with fixed column semantics

- Risk: false alarms due to strict thresholds
  - Mitigation: configurable thresholds and staged tuning period
