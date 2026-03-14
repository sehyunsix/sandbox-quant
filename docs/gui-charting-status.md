# GUI Charting Status

## Summary

The project now has a reusable internal charting layer for `egui + plotters`.
The existing CLI flow remains unchanged, while the GUI path renders market/backtest charts through a shared charting module.

Current structure:

- `core -> shell`
- `core -> gui`
- `charting` is no longer embedded inside the GUI widget code

## Implemented

### 1. GUI entrypoint

- Added `sandbox-quant-gui`
- Enabled with Cargo feature `gui`

Relevant files:

- [`Cargo.toml`](/Users/yuksehyun/project/sandbox-quant/Cargo.toml)
- [`src/bin/sandbox-quant-gui.rs`](/Users/yuksehyun/project/sandbox-quant/src/bin/sandbox-quant-gui.rs)

### 2. Visualization/query layer

- Added chart-facing dataset loaders
- Added reusable visualization service for dashboard/backtest data preparation

Relevant files:

- [`src/dataset/query.rs`](/Users/yuksehyun/project/sandbox-quant/src/dataset/query.rs)
- [`src/dataset/types.rs`](/Users/yuksehyun/project/sandbox-quant/src/dataset/types.rs)
- [`src/visualization/service.rs`](/Users/yuksehyun/project/sandbox-quant/src/visualization/service.rs)
- [`src/visualization/types.rs`](/Users/yuksehyun/project/sandbox-quant/src/visualization/types.rs)

### 3. Internal charting library

- Extracted a reusable charting core
- Added renderer trait and plotters backend
- Added egui texture adapter
- Added sandbox-specific scene adapters

Relevant files:

- [`src/charting/mod.rs`](/Users/yuksehyun/project/sandbox-quant/src/charting/mod.rs)
- [`src/charting/scene.rs`](/Users/yuksehyun/project/sandbox-quant/src/charting/scene.rs)
- [`src/charting/render.rs`](/Users/yuksehyun/project/sandbox-quant/src/charting/render.rs)
- [`src/charting/plotters.rs`](/Users/yuksehyun/project/sandbox-quant/src/charting/plotters.rs)
- [`src/charting/egui.rs`](/Users/yuksehyun/project/sandbox-quant/src/charting/egui.rs)
- [`src/charting/inspect.rs`](/Users/yuksehyun/project/sandbox-quant/src/charting/inspect.rs)
- [`src/charting/adapters/sandbox.rs`](/Users/yuksehyun/project/sandbox-quant/src/charting/adapters/sandbox.rs)

### 4. Chart scene model

- Introduced `EpochMs` newtype
- Introduced `ChartScene`, `Pane`, `Series`
- Added `Candles`, `Bars`, `Line`, `Markers`
- Added pane weights
- Added pane-specific y-axis formatting
- Added viewport state for x-range zoom/pan
- Added hover model, crosshair model, tooltip model

### 5. Market chart features

- Candlestick rendering from `derived_kline_1s`
- Fallback line rendering when no candles exist
- Liquidation overlays
- Entry/exit signal overlays
- Volume pane
- Hovered candle/bar highlight
- Crosshair labels on axes

### 6. Equity/backtest features

- Equity curve rendering
- Backtest run selection
- Trade table
- PnL summary

### 7. Interaction

- Symbol/date/template selection in GUI
- Quick date presets
- Load chart / run selected strategy actions
- Hover tooltip
- Crosshair
- Mouse wheel zoom
- Drag pan

Relevant GUI file:

- [`src/gui/app.rs`](/Users/yuksehyun/project/sandbox-quant/src/gui/app.rs)

## Verification

Validated repeatedly during implementation with:

- `cargo check`
- `cargo check --features gui --bin sandbox-quant-gui`
- `cargo test --lib charting::inspect::tests::zoom_scene_handles_subsecond_full_span`

## Known constraints

- Tooltip positioning is better than before, but still heuristic relative to the widget rect
- Viewport state currently lives in the GUI shell, not in a generalized session/state store
- Hover geometry is approximated from current plot layout constants
- The plotters backend is still single-backend focused even though the public API is now much cleaner
- `ChartScene` is reusable, but not yet extracted into a standalone crate

## TODO

### High priority

- Add double-click viewport reset
- Add keyboard zoom/pan shortcuts
- Add explicit viewport min/max controls in GUI
- Persist viewport per chart tab instead of resetting on refresh/run
- Improve tooltip placement to avoid clipping near window edges

### Charting quality

- Snap hover/crosshair to the nearest visible candle center rather than interpolated x only
- Improve candle width and label density based on current zoom level
- Add volume color legend and pane title styling
- Add configurable crosshair theme
- Add current-price marker on the right axis
- Add backtest fill markers aligned to candle bodies/wicks

### UX

- Add reset zoom button
- Add visible indicator for current viewport range
- Add optional compact mode for tooltip cards
- Add legend toggle per series
- Add show/hide controls for liquidations, signals, and volume

### Architecture

- Move viewport mutation helpers behind a chart controller API
- Separate plotters backend feature from egui adapter feature
- Add backend-neutral tests for scene -> hover/tooltip behavior
- Add chart snapshots/golden tests for renderer regression detection
- Prepare `src/charting` for extraction into its own crate

### Data features

- Add higher timeframe aggregation alongside `derived_kline_1s`
- Add explicit OHLCV loader APIs for chart consumers
- Add indicator panes such as VWAP, EMA, and liquidation intensity
- Add trade PnL markers and position lifecycle overlays

## Run

```bash
cargo run --features gui --bin sandbox-quant-gui -- \
  --base-dir var \
  --mode demo \
  --symbol BTCUSDT \
  --from 2026-03-12 \
  --to 2026-03-13
```

Notes:

- The binary is feature-gated behind `--features gui`.
- Launch arguments are parsed directly in `src/bin/sandbox-quant-gui.rs`.
- There is currently no dedicated `--help` output; unsupported args return an error immediately.


## Current data-source limitation

The GUI market chart currently loads market data from:

- `raw_book_ticker`
- `raw_liquidation_events`
- `derived_kline_1s` (derived from `raw_agg_trades`)

It does **not** yet render historical backfill directly from `raw_klines` (for example 15m / 1h collector imports).

Operational consequence:

- `sandbox-quant-collector` may successfully import historical kline data into `raw_klines`
- `sandbox-quant-collector summary` will show those rows
- but the GUI can still appear empty if `raw_book_ticker` / `raw_agg_trades` / `derived_kline_1s` are absent for the selected symbol/date range

For majors-only historical dataset management, treat `collector summary` as the source of truth until GUI support for `raw_klines` is added.
