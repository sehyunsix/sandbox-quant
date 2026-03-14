# GUI Button Verification Scenarios

This document defines concrete verification scenarios for the GUI control surface.
The goal is to avoid claiming “it works” after implementing a single feature path.

## Scope

Buttons / actions covered:

- `Refresh`
- `Load Chart`
- `Run Backtest`
- `Latest Run`
- `Reset Zoom`
- `Today`
- `Last 2D`
- `Last 7D`
- `Chart timeframe` selector

## Verification Principles

1. Prefer deterministic app/unit tests for button state transitions.
2. Use headless chart export to validate rendering without launching the GUI.
3. Use CLI backtest/export commands where the GUI action is a thin wrapper over the same service path.
4. Record both the scenario and the command / test used as evidence.

## Scenarios

### 1. Date preset buttons

Expected:

- `Today` sets `from == to == today`
- `Last 2D` sets `from = today - 1 day`, `to = today`
- `Last 7D` sets `from = today - 6 days`, `to = today`

Evidence:

- `src/gui/app.rs::tests::date_presets_update_input_ranges`

### 2. Reset Zoom

Expected:

- existing chart viewport ranges are cleared
- both market + equity viewport state reset to default

Evidence:

- `src/gui/app.rs::tests::reset_viewports_clears_chart_ranges`

### 3. Load Chart with empty requested day

Expected:

- if the requested date range has no `book_ticker` / `derived_kline_1s` / `raw_kline` rows
- GUI falls back to the latest available UTC day derived from recorder metrics
- snapshot loads instead of staying empty when data exists on an older day

Evidence:

- `src/gui/app.rs::tests::refresh_dashboard_with_fallback_loads_latest_available_day_from_raw_klines`

### 4. Raw historical kline fallback

Expected:

- GUI should render chart data even when only `raw_klines` exist and `derived_kline_1s` is absent

Evidence:

- `src/visualization/service.rs::tests::load_dashboard_falls_back_to_raw_klines_when_derived_klines_are_absent`

### 5. Run Backtest

Expected:

- GUI action produces a selected report
- selected tab switches to `PnL`

Evidence:

- `src/gui/app.rs::tests::run_backtest_button_path_produces_selected_report_and_pnl_tab`

### 6. Latest Run

Expected:

- GUI loads the latest available run when no explicit `selected_run_id` is provided
- headless export uses the latest available selected report

Evidence:

- headless export command:

```bash
cargo run --features gui --bin sandbox-quant-gui -- \
  --base-dir var \
  --mode demo \
  --symbol BTCUSDT \
  --from 2026-03-13 \
  --to 2026-03-13 \
  --headless-debug-export-dir target/headless-debug-latest
```

### 7. Timeframe selector

Expected:

- GUI market chart supports `1s`, `1m`, `3m`, `5m`, `15m`, `30m`, `1h`, `4h`, `1w`, `1d`, `1mo`
- aggregation behaves deterministically
- headless export honors the same `--chart-timeframe`

Evidence:

- `tests/gui_charting_tests.rs::market_scene_can_aggregate_to_minute_timeframe`
- `tests/gui_charting_tests.rs::market_scene_can_aggregate_to_five_minute_timeframe`
- `tests/gui_charting_tests.rs::market_scene_can_aggregate_to_week_timeframe`
- headless export command:

```bash
cargo run --features gui --bin sandbox-quant-gui -- \
  --base-dir var \
  --mode demo \
  --symbol BTCUSDT \
  --from 2026-03-13 \
  --to 2026-03-13 \
  --chart-timeframe 4h \
  --headless-debug-export-dir target/headless-debug-4h
```

### 8. Hover / tooltip / chart interaction quality

Expected:

- hover snaps to nearest visible point
- tooltip is kept inside chart bounds as much as current heuristic allows
- double-click resets viewport

Evidence:

- `src/charting/inspect.rs::tests::nearest_visible_time_snaps_to_closest_point_in_view`
- visual/headless regression checks from `target/headless-debug-*`

## Command bundle

Primary verification command:

```bash
cargo check --features gui --bin sandbox-quant-gui
cargo test --features gui -q
```

Supplemental headless exports:

```bash
cargo run --features gui --bin sandbox-quant-gui -- \
  --base-dir var \
  --mode demo \
  --symbol BTCUSDT \
  --from 2026-03-13 \
  --to 2026-03-13 \
  --chart-timeframe 1m \
  --headless-debug-export-dir target/headless-debug-1m

cargo run --features gui --bin sandbox-quant-gui -- \
  --base-dir var \
  --mode demo \
  --symbol BTCUSDT \
  --from 2026-03-13 \
  --to 2026-03-13 \
  --chart-timeframe 4h \
  --headless-debug-export-dir target/headless-debug-4h
```
