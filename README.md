# sandbox-quant

[![docs.rs](docs/assets/docsrs-badge.svg)](https://docs.rs/sandbox-quant)
[![crates.io](docs/assets/cratesio-badge.svg)](https://crates.io/crates/sandbox-quant)

Exchange-truth trading core for Binance Spot and Futures, with separate operator, recorder, collector, backtest, and optional GUI entrypoints.

![sandbox-quant shell startup](docs/assets/shell-startup.png)

The current codebase is a reset `v1` architecture focused on:

- authoritative account, position, and open-order sync
- typed execution commands
- safe close primitives
- Binance adapter with signed HTTP transport
- foreground market-data recorder
- dataset-backed backtest terminal
- UI-independent core testing

The old strategy-heavy terminal dashboard described in earlier revisions is no longer the active implementation.

## Current Scope

Implemented today:

- `refresh` of authoritative exchange state
- `close-all`
- `close-symbol <instrument>`
- `set-target-exposure <instrument> <target>`
- strategy watch start/list/show/stop in the operator terminal
- separate `sandbox-quant-recorder` terminal for market data collection
- separate `sandbox-quant-collector` binary for historical Binance public-data imports
- separate `sandbox-quant-backtest` terminal for dataset inspection/backtest runs
- optional `sandbox-quant-gui` desktop app for charting and backtest exploration
- Binance signed REST transport
- runtime event logging
- CLI summaries for refresh and execution results
- automatic dataset schema bootstrap/version surfacing for recorder/collector flows

Not implemented as first-class runtime features yet:

- automated strategy execution engine
- liquidation trigger evaluator for live trading
- full historical replay engine beyond dataset summary
- detached recorder supervision model

Legacy strategy/UI documents are archived under `docs/archive/legacy`.

## Architecture

Top-level modules:

- `src/app`
- `src/backtest_app`
- `src/charting`
- `src/command`
- `src/dataset`
- `src/domain`
- `src/error`
- `src/exchange`
- `src/execution`
- `src/gui`
- `src/market_data`
- `src/portfolio`
- `src/record`
- `src/recorder_app`
- `src/storage`
- `src/terminal`
- `src/ui`
- `src/visualization`

Core rules:

- exchange state is the source of truth
- local storage is not authoritative trading state
- canonical position representation is `signed_qty`
- execution is command-driven
- recorder terminal owns live market-data ingestion in-process
- strategy logic may be shared between operator and backtest, but it must not depend directly on DuckDB
- tests live in `tests/`

The design rationale is documented in [0056-v1-reset-exchange-truth-architecture.md](docs/rfcs/0056-v1-reset-exchange-truth-architecture.md).
Recorder ownership is documented in [0058-recorder-foreground-terminal-semantics.md](docs/rfcs/0058-recorder-foreground-terminal-semantics.md) and [0059-recorder-single-owner-runtime.md](docs/rfcs/0059-recorder-single-owner-runtime.md).

GUI/charting implementation notes and current limitations are tracked in [`docs/gui-charting-status.md`](docs/gui-charting-status.md).

## Storage / Dependency Notes

This project currently uses both embedded and external storage dependencies:

- `duckdb` / `rusqlite` for local dataset and recorder-facing storage workflows
- `postgres` for collector storage targets and PostgreSQL-backed market-data import / summary flows

In other words, PostgreSQL is not just an optional environment detail; it is a real crate dependency in `Cargo.toml` and is used by the collector/storage path when `--storage postgres` is selected.

## Recent Hardening Notes

Recent follow-up work focused on making the current GUI/backtest path safer and clearer to operate:

- GUI market charts now avoid the high-resolution timestamp panic that previously appeared on some BTCUSDT ranges.
- GUI chart time labels are rendered through an overflow-safe footer-label path, with adaptive width-based label density.
- GUI hover/tooltip behavior was polished with safer placement near chart edges, plus better hover snapping to visible points.
- GUI controls now expose clearer empty/error states, plus reset-zoom affordances.
- Backtest CLI now rejects reversed date ranges before any DB initialization work begins.
- Backtest output now distinguishes `state=ok`, `state=no_trades`, `state=empty_dataset`, and `state=missing`.
- Collector/recorder summary surfaces now expose `schema_version` metadata so schema bootstrap state is visible to operators.

Known current caveats:

- GUI footer time labels are adaptive and panic-safe, but they are still not full plotters-native mesh ticks.
- Tooltip sizing/placement is still heuristic.
- Backtest UX is being refined further around `symbol_not_found` vs generic `empty_dataset` messaging.
- Full strict `clippy` across the repo still reports pre-existing `too_many_arguments` warnings outside the focused GUI/backtest hardening scope.

## Environment

Required:

```bash
BINANCE_DEMO_API_KEY=your_demo_key
BINANCE_DEMO_SECRET_KEY=your_demo_secret
BINANCE_REAL_API_KEY=your_real_key
BINANCE_REAL_SECRET_KEY=your_real_secret
```

Optional:

```bash
BINANCE_API_KEY=legacy_shared_key
BINANCE_SECRET_KEY=legacy_shared_secret
BINANCE_SPOT_BASE_URL=https://api.binance.com
BINANCE_FUTURES_BASE_URL=https://fapi.binance.com
BINANCE_OPTIONS_BASE_URL=https://eapi.binance.com
SANDBOX_QUANT_RECORDER_STORAGE=duckdb
SANDBOX_QUANT_POSTGRES_URL=postgres://localhost/sandbox_quant
```

The runtime reads demo and real credentials separately based on `BINANCE_MODE` and when using `/mode real|demo`. The legacy shared key names are still accepted as a fallback. The default runtime mode is `demo`. Optional base URLs are useful for explicit testnet or custom routing.

Storage-specific env vars:

- `SANDBOX_QUANT_RECORDER_STORAGE=duckdb|postgres` selects the live recorder sink
- `SANDBOX_QUANT_POSTGRES_URL` (or `DATABASE_URL`) is used by PostgreSQL-backed recorder/collector flows
- `SANDBOX_QUANT_BACKTEST_SOURCE=postgres` makes `sandbox-quant-backtest run` read source market data directly from PostgreSQL
- `SANDBOX_QUANT_BACKTEST_AUTO_SNAPSHOT=postgres` makes backtest `run` pull the requested symbol/date range from PostgreSQL into DuckDB before executing
- `SANDBOX_QUANT_BACKTEST_EXPORT_POSTGRES=1` forces backtest runs to export summary, trades, and equity points into PostgreSQL for Grafana
- `SANDBOX_QUANT_BACKTEST_SNAPSHOT_PRODUCT` / `SANDBOX_QUANT_BACKTEST_SNAPSHOT_INTERVAL` can narrow the imported snapshot

## Binaries

- `sandbox-quant`
  - operator terminal
  - manual execution and strategy watch management
- `sandbox-quant-recorder`
  - foreground market-data recorder terminal
  - `/start`, `/status`, `/stop`, `/mode`
- `sandbox-quant-backtest`
  - dataset consumer terminal
  - interactive shell plus `run`, `list`, `report latest|show <run_id>`
- `sandbox-quant-collector`
  - one-shot historical Binance public data backfill
  - `binance-public import`, `summary`, `snapshot postgres-to-duckdb`
- `sandbox-quant-watchdog`
  - PostgreSQL freshness probe for recorder market data
  - non-interactive operational helper for recorder freshness checks
- `sandbox-quant-gui`
  - optional desktop GUI for charting + backtest exploration
  - requires Cargo feature `gui`

The terminal binaries share the same line-oriented UX style, but lifecycle ownership is separate. The GUI is a separate optional launch path built on the same dataset/charting core.

## Running

Operator shell:

```bash
cargo run --bin sandbox-quant
```

Trading-engine daemon mode:

```bash
cargo run --bin sandbox-quant -- serve --mode demo --base-dir var --listen 127.0.0.1:9782
```

Trading-engine daemon control:

```bash
cargo run --bin sandbox-quant -- status --listen 127.0.0.1:9782
cargo run --bin sandbox-quant -- health --listen 127.0.0.1:9782
cargo run --bin sandbox-quant -- stop --listen 127.0.0.1:9782
```

Trading-engine daemon command API examples:

```bash
curl -s -X POST http://127.0.0.1:9782/refresh
curl -s -X POST http://127.0.0.1:9782/close-all
curl -s -X POST http://127.0.0.1:9782/close-symbol \
  -H 'content-type: application/json' \
  -d '{"instrument":"BTCUSDT"}'
curl -s -X POST http://127.0.0.1:9782/set-target-exposure \
  -H 'content-type: application/json' \
  -d '{"instrument":"BTCUSDT","target":0.25,"order_type":"market"}'
```

Refresh authoritative state:

```bash
cargo run --bin sandbox-quant -- refresh
```

Close all currently open positions:

```bash
cargo run --bin sandbox-quant -- close-all
```

Close one symbol:

```bash
cargo run --bin sandbox-quant -- close-symbol BTCUSDT
```

Set target exposure:

```bash
cargo run --bin sandbox-quant -- set-target-exposure BTCUSDT 0.25
```

`target exposure` must be in `-1.0..=1.0`.

Submit an options limit order:

```bash
cargo run --bin sandbox-quant -- option-order BTC-260327-200000-C buy 0.01 5
```

Options orders are handled as a separate workflow. They appear in portfolio positions and open orders, but they are not integrated into `set-target-exposure`.

Recorder terminal:

```bash
cargo run --bin sandbox-quant-recorder
```

Recorder terminal with PostgreSQL sink:

```bash
export SANDBOX_QUANT_RECORDER_STORAGE=postgres
export SANDBOX_QUANT_POSTGRES_URL=postgres://localhost/sandbox_quant
cargo run --bin sandbox-quant-recorder
```

YAML-based runtime launcher:

```bash
bash ops/run-runtime.sh ops/grafana/.env recorder
bash ops/run-runtime.sh ops/grafana/.env trading_engine
bash ops/run-runtime.sh ops/grafana/.env backfill_binance
bash ops/run-runtime.sh ops/grafana/.env backfill_fx
bash ops/run-runtime.sh ops/grafana/.env backfill_kr
```

The launcher reads service defaults from [ops/runtime.yaml](/Users/yuksehyun/project/sandbox-quant/ops/runtime.yaml) and derives `SANDBOX_QUANT_POSTGRES_URL` from the env file, so operators do not need to type long `export` sequences manually.

Recorder with integrated Binance 1m backfill sidecar:

```bash
cargo run --bin sandbox-quant-recorder -- \
  serve BTCUSDT ETHUSDT SOLUSDT XRPUSDT \
  --backfill \
  --backfill-poll-seconds 30
```

This starts the recorder daemon and also spawns `postgres_kline_backfill` as a child process so current-day `raw_klines` stay fresh alongside websocket event ingest.

Long-running recorder processes now emit heartbeat JSON logs every 5 seconds with `kind=heartbeat`, including `ping_at`, `pong_at`, `heartbeat_age_sec`, `reader_alive`, `writer_alive`, `worker_alive`, and watched symbols. Those logs are written to `var/log/recorder.jsonl` by default and are intended for Loki/Grafana operational monitoring.

Recorder daemon control:

```bash
cargo run --bin sandbox-quant-recorder -- status --listen 127.0.0.1:9781
cargo run --bin sandbox-quant-recorder -- health --listen 127.0.0.1:9781
cargo run --bin sandbox-quant-recorder -- freshness --listen 127.0.0.1:9781
cargo run --bin sandbox-quant-recorder -- stop --listen 127.0.0.1:9781
```

Recorder watchdog probe against PostgreSQL + recorder heartbeat:

```bash
cargo run --bin sandbox-quant-watchdog -- probe
```

Then inside the recorder terminal:

```text
/start BTCUSDT
/status
/stop
```

Backtest terminal:

```bash
cargo run --bin sandbox-quant-backtest -- --mode demo
```

One-shot dataset run:

```bash
cargo run --bin sandbox-quant-backtest -- \
  run liquidation-breakdown-short BTCUSDT --from 2026-03-13 --to 2026-03-13 --mode demo --base-dir var
```

Direct PostgreSQL-backed backtest run without DuckDB source reads:

```bash
export SANDBOX_QUANT_BACKTEST_SOURCE=postgres
export SANDBOX_QUANT_BACKTEST_EXPORT_POSTGRES=1
export SANDBOX_QUANT_POSTGRES_URL=postgres://localhost/sandbox_quant
cargo run --bin sandbox-quant-backtest -- \
  run price-sma-cross-long BTCUSDT --from 2026-01-01 --to 2026-03-15 --mode demo
```

In direct mode, the CLI reads `raw_klines` from PostgreSQL and writes the result set back to PostgreSQL for Grafana. DuckDB is bypassed for the source dataset.

PostgreSQL sweep script for repeated strategy experiments with Grafana-visible exports:

```bash
ops/backtest-postgres-sweep.sh
```

Useful overrides:

```bash
MODE=demo \
INSTRUMENTS_CSV=BTCUSDT,ETHUSDT,SOLUSDT \
TEMPLATES_CSV=price-sma-cross-long,price-sma-cross-short,price-sma-cross-long-fast,price-sma-cross-short-fast \
DATE_WINDOWS_CSV=2026-03-01:2026-03-15,2026-03-16:2026-03-18 \
ops/backtest-postgres-sweep.sh
```

The sweep script uses the existing PostgreSQL export path, so each run is inserted into `backtest_runs`, `backtest_trades`, and `backtest_equity_points` for the Grafana `sandbox-quant backtest pnl` dashboard.

When the trading engine runs in shell or explicit `run` mode, it emits heartbeat JSON logs every 5 seconds with `kind=heartbeat`, `heartbeat_age_sec=0`, DuckDB path visibility, and latest local market-data freshness so the process can be monitored from Grafana/Loki.

If `--from` is after `--to`, the command now fails fast with an invalid date-range error before touching the dataset DB.

List recent runs:

```bash
cargo run --bin sandbox-quant-backtest -- list --mode demo --base-dir var
```

Show the latest persisted report:

```bash
cargo run --bin sandbox-quant-backtest -- report latest --mode demo --base-dir var
```

Export the latest persisted backtest run into PostgreSQL for Grafana:

```bash
export SANDBOX_QUANT_POSTGRES_URL="postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@localhost:5432/${POSTGRES_DB}"
cargo run --bin sandbox-quant-backtest -- \
  export postgres latest \
  --mode demo \
  --base-dir var
```

Backtest report/list output now uses explicit state markers so operators can tell the difference between:

- a normal run with trades: `state=ok`
- a valid dataset window with no executed trades: `state=no_trades`
- no available dataset rows for the requested symbol/date range: `state=empty_dataset`
- a missing persisted run id: `state=missing`

Historical public-data backfill:

```bash
cargo run --bin sandbox-quant-collector -- \
  binance-public import \
  --products um \
  --symbols BTCUSDT,ETHUSDT \
  --from 2026-03-12 \
  --to 2026-03-13 \
  --kline-interval 1m \
  --mode demo \
  --base-dir var
```

Dataset summary after import:

```bash
cargo run --bin sandbox-quant-collector -- summary --mode demo --base-dir var
```

PostgreSQL historical ingest (first migration slice):

```bash
export SANDBOX_QUANT_POSTGRES_URL=postgres://localhost/sandbox_quant
cargo run --bin sandbox-quant-collector -- \
  binance-public import \
  --products um \
  --symbols BTCUSDT,ETHUSDT \
  --from 2026-03-12 \
  --to 2026-03-13 \
  --kline-interval 15m \
  --storage postgres
```

PostgreSQL summary:

```bash
cargo run --bin sandbox-quant-collector -- summary --storage postgres
```

Unified PostgreSQL backfill wrapper:

```bash
bash ops/backfill-postgres.sh ops/grafana/.env binance
bash ops/backfill-postgres.sh ops/grafana/.env fx
bash ops/backfill-postgres.sh ops/grafana/.env kr
bash ops/backfill-postgres.sh ops/grafana/.env all
```

Use this wrapper as the operator-facing entrypoint for source-specific PostgreSQL kline backfills.

Backtest-ready DuckDB snapshot exported from PostgreSQL:

```bash
cargo run --bin sandbox-quant-collector -- \
  snapshot postgres-to-duckdb \
  --symbols BTCUSDT,ETHUSDT \
  --from 2026-03-12 \
  --to 2026-03-13 \
  --interval 15m
```

By default the PostgreSQL snapshot now exports any matching `raw_klines`, `raw_liquidation_events`, `raw_book_ticker`, and `raw_agg_trades` rows into DuckDB so existing backtest/GUI flows can keep reading DuckDB snapshots. Use `--skip-book-tickers` / `--skip-agg-trades` / `--skip-liquidations` to narrow the export.

Backtest Grafana flow:

- run or inspect a backtest against DuckDB
- export the persisted run into PostgreSQL with `sandbox-quant-backtest export postgres latest|show <run_id>`
- open the `sandbox-quant backtest pnl` Grafana dashboard to inspect equity curve, cumulative PnL, trade PnL, and recent exported runs

Collector/recorder summary output now includes schema metadata such as `schema_version` so DB bootstrap state is visible without manual inspection.

GUI launch (optional feature):

```bash
cargo run --features gui --bin sandbox-quant-gui -- \
  --base-dir var \
  --mode demo \
  --symbol BTCUSDT \
  --from 2026-03-12 \
  --to 2026-03-13
```

`src/bin/sandbox-quant-gui.rs` currently accepts launch args directly and does not expose a dedicated `--help` screen; unsupported args fail fast.

Current limitation: the GUI market chart currently reads `raw_book_ticker`, `raw_liquidation_events`, and `derived_kline_1s` (from `raw_agg_trades`). Historical collector backfills stored only in `raw_klines` may appear in `collector summary` but still not render in the GUI until `raw_klines` support is added there.

The GUI uses the same DuckDB-backed dataset/backtest pipeline as the terminal tools:

- load recorded symbols and summary metrics from `var/`
- render book-ticker / 1s kline market charts with liquidation + trade markers
- run a strategy backtest and inspect equity + trade tables in the same app session

Recent GUI usability improvements include:

- clearer empty/error/status guidance when no data is loaded
- reset zoom control and double-click viewport reset
- safer hover snapping and tooltip placement
- overflow-safe adaptive footer time labels on charts

Recorder data is stored by default under:

```text
var/market-v2-demo.duckdb
var/market-v2-real.duckdb
```

`demo` and `real` here refer to account mode metadata. Public market-data streams currently use Binance public futures streams for both modes.

When recorder / collector / backtest tooling opens a dataset DB, the shared market-data schema is applied automatically. Summary/status surfaces now expose `schema_version` so older recorder-created DBs can be bootstrapped forward without manual table creation.

Recommended storage split for concurrency-sensitive workflows:

- PostgreSQL for concurrent ingest / accumulation of raw market data
- DuckDB for read-heavy snapshots used by backtesting and research
- Collector currently supports PostgreSQL historical imports plus `snapshot postgres-to-duckdb`
- Recorder still writes directly to DuckDB today; migrating live recorder ingest to PostgreSQL is the next architectural step if lock contention remains a problem

## Output

`refresh` prints a summary like:

```text
refresh completed
staleness=Fresh
balances=1
positions=2
open_order_groups=1
last_event=app.portfolio.refreshed
```

Execution commands print a summary like:

```text
execution completed
command=close-all
batch_id=1
submitted=2
skipped=0
rejected=0
outcome=batch_completed
```

or:

```text
execution completed
command=set-target-exposure
instrument=BTCUSDT
target=0.25
outcome=submitted
```

## Testing

Library:

```bash
cargo test -q --lib
```

Current integration suite:

```bash
cargo test -q \
  --test core_types_tests \
  --test reconciliation_tests \
  --test binance_adapter_tests \
  --test app_runtime_tests \
  --test binance_http_transport_tests \
  --test bootstrap_tests \
  --test cli_command_tests \
  --test cli_output_tests
```

## Release

Release automation is driven by GitHub Actions on `main`.

- default bump: `patch`
- merge commit with `#minor`: `minor`
- merge commit with `#major` or `BREAKING CHANGE`: `major`

For the `1.0.0` release, the final merge into `main` should include `#major`.

Automation outputs:

- bump `Cargo.toml` and `Cargo.lock`
- create git tag `vX.Y.Z`
- create GitHub release
- publish to crates.io

## Notes

- `set-target-exposure` refreshes authoritative portfolio state before planning and can open from flat if the exchange symbol resolves.
- execution and refresh flows are tested without any UI dependency.
- README examples reflect the current runtime surface, not the removed legacy system.
- recorder terminal live status is in-process worker truth, not a stale external status file.
