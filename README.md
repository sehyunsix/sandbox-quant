# sandbox-quant

Rust-native Binance Spot Testnet trading prototype.

It provides real-time market streaming, strategy-driven order execution, cumulative trade history, and a terminal dashboard for monitoring positions and performance.

## Main Features

- Real-time market + strategy loop
  - Streams Binance Spot Testnet trades through WebSocket.
  - Runs MA crossover logic and executes orders via REST.
- Terminal trading dashboard (ratatui)
  - Live chart, position panel, order/signal panel, order history, and system log.
  - Fast keyboard control for trading and navigation.
- Historical trade persistence and recovery
  - Persists orders/trades to SQLite.
  - Automatically backfills missing history and continues with incremental sync.
- Stable cumulative performance stats
  - Keeps cumulative `Trades/W/L/PnL` from resetting on transient API issues.
  - Uses history-first stats for consistent dashboard values.
- Strategy and manual-performance breakdown
  - Strategy selector shows per-strategy `W/L/T/PnL`.
  - Includes `MANUAL(rest)` and `TOTAL` rows for attribution.
- Chart fill markers
  - Shows historical `B/S` markers mapped by symbol/timeframe.
  - Keeps marker display simple (`B/S` only).
- Operational robustness
  - Handles Binance time drift errors (`-1021`) with time sync + retry.
  - Reduces high request-weight behavior to avoid rate-limit pressure.

## Quick Start

1. Create environment file:

```bash
cp .env.example .env
```

2. Set your Binance Spot Testnet credentials in `.env`:

```bash
BINANCE_API_KEY=your_testnet_api_key_here
BINANCE_API_SECRET=your_testnet_api_secret_here
```

3. Run:

```bash
cargo run --bin sandbox-quant
```

## Runtime Configuration

Edit `config/default.toml` as needed.

## Usage

Main keys:

- `Q`: quit
- `P`: pause strategy
- `R`: resume strategy
- `B`: manual buy
- `S`: manual sell
- `T`: open symbol selector
- `Y`: open strategy selector
- `0/1/H/D/W/M`: switch timeframe (`1s/1m/1h/1d/1w/1M`)

Example flow:

1. Start with `cargo run --bin sandbox-quant`
2. Press `Y`, choose a strategy with arrows, then Enter
3. Press `B` for a manual buy
4. Verify `B/S` markers on chart
5. Open `Y` again to review strategy stats and `MANUAL(rest)`

## Run Capture

### Terminal Screenshot

The image below summarizes a real `cargo run --bin sandbox-quant` session:

![cargo run terminal snapshot](docs/assets/cargo-run-terminal-snapshot.png)

### Raw Output

Captured run output:

- `docs/assets/cargo-run-output.txt`

Note:
- This is a TUI app. Running in non-interactive output redirection contexts can fail terminal initialization.
- For normal usage, run directly in an interactive terminal.

## Project Layout

```text
sandbox-quant/
├── Cargo.toml
├── config/default.toml
├── docs/assets/
│   ├── cargo-run-terminal-snapshot.svg
│   └── cargo-run-output.txt
├── src/
│   ├── main.rs
│   ├── order_manager.rs
│   ├── order_store.rs
│   ├── ui/
│   └── ...
└── TESTING.md
```
