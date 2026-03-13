# sandbox-quant

[![docs.rs](docs/assets/docsrs-badge.svg)](https://docs.rs/sandbox-quant)
[![crates.io](docs/assets/cratesio-badge.svg)](https://crates.io/crates/sandbox-quant)

Exchange-truth trading core for Binance Spot and Futures, with separate operator, recorder, and backtest terminals.

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
- separate `sandbox-quant-backtest` terminal for dataset inspection/backtest runs
- Binance signed REST transport
- runtime event logging
- CLI summaries for refresh and execution results

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
- `src/command`
- `src/dataset`
- `src/domain`
- `src/error`
- `src/exchange`
- `src/execution`
- `src/market_data`
- `src/portfolio`
- `src/record`
- `src/recorder_app`
- `src/storage`
- `src/terminal`
- `src/ui`

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
```

The runtime reads demo and real credentials separately based on `BINANCE_MODE` and when using `/mode real|demo`. The legacy shared key names are still accepted as a fallback. The default runtime mode is `demo`. Optional base URLs are useful for explicit testnet or custom routing.

## Binaries

- `sandbox-quant`
  - operator terminal
  - manual execution and strategy watch management
- `sandbox-quant-recorder`
  - foreground market-data recorder terminal
  - `/start`, `/status`, `/stop`, `/mode`
- `sandbox-quant-backtest`
  - dataset consumer terminal
  - `/run`, `/mode`

All three binaries share the same terminal UX style, but lifecycle ownership is separate.

## Running

Operator shell:

```bash
cargo run --bin sandbox-quant
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
cargo run --bin sandbox-quant-recorder -- --mode demo
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

Or one-shot dataset run:

```bash
target/debug/sandbox-quant-backtest run liquidation-breakdown-short BTCUSDT --from 2026-03-13 --to 2026-03-13 --mode demo --base-dir var
```

Recorder data is stored by default under:

```text
var/market-v2-demo.duckdb
var/market-v2-real.duckdb
```

`demo` and `real` here refer to account mode metadata. Public market-data streams currently use Binance public futures streams for both modes.

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
