# sandbox-quant

[![docs.rs](docs/assets/docsrs-badge.svg)](https://docs.rs/sandbox-quant)
[![crates.io](docs/assets/cratesio-badge.svg)](https://crates.io/crates/sandbox-quant)

Exchange-truth trading core for Binance Spot and Futures.

The current codebase is a reset `v1` architecture focused on:

- authoritative account, position, and open-order sync
- typed execution commands
- safe close primitives
- Binance adapter with signed HTTP transport
- UI-independent core testing

The old strategy-heavy terminal dashboard described in earlier revisions is no longer the active implementation.

## Current Scope

Implemented today:

- `refresh` of authoritative exchange state
- `close-all`
- `close-symbol <instrument>`
- `set-target-exposure <instrument> <target>`
- Binance signed REST transport
- runtime event logging
- CLI summaries for refresh and execution results

Not implemented as first-class runtime features yet:

- strategy engine
- auto-trading loop
- persistent analytics pipeline
- websocket-driven live market loop

Legacy strategy/UI documents and historical datasets are archived under `docs/archive/legacy` and `archive/legacy/data`.

## Architecture

Top-level modules:

- `src/app`
- `src/domain`
- `src/error`
- `src/exchange`
- `src/execution`
- `src/market_data`
- `src/portfolio`
- `src/storage`

Core rules:

- exchange state is the source of truth
- local storage is not authoritative trading state
- canonical position representation is `signed_qty`
- execution is command-driven
- tests live in `tests/`

The design rationale is documented in [0056-v1-reset-exchange-truth-architecture.md](docs/rfcs/0056-v1-reset-exchange-truth-architecture.md).

## Environment

Required:

```bash
BINANCE_API_KEY=your_key
BINANCE_SECRET_KEY=your_secret
```

Optional:

```bash
BINANCE_SPOT_BASE_URL=https://api.binance.com
BINANCE_FUTURES_BASE_URL=https://fapi.binance.com
```

The optional base URLs are useful for testnet or custom routing.

## Running

Refresh authoritative state:

```bash
cargo run -- refresh
```

Close all currently open positions:

```bash
cargo run -- close-all
```

Close one symbol:

```bash
cargo run -- close-symbol BTCUSDT
```

Set target exposure:

```bash
cargo run -- set-target-exposure BTCUSDT 0.25
```

`target exposure` must be in `-1.0..=1.0`.

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

## Notes

- `set-target-exposure` currently relies on an existing authoritative position and fetched last price.
- execution and refresh flows are tested without any UI dependency.
- README examples reflect the current runtime surface, not the removed legacy system.
