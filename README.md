# sandbox-quant

Minimal Rust-native trading prototype targeting **Binance Spot Testnet**.

Implements a Moving Average Crossover strategy with real-time market data,
order placement, and a terminal-based dashboard.

> **WARNING: This is for TESTNET ONLY. Do not use with real API keys or mainnet.**

## Architecture

```
WS Task ──tick──> Strategy Task ──signal──> Order Manager
    │                                           │
    │         (all send AppEvent)               │
    └──MarketTick──> app_event_rx <──OrderUpdate─┘
                         │
                    TUI Main Loop (ratatui)
```

- Fully async (tokio + tokio-tungstenite + reqwest)
- Channel-based coordination between tasks
- Graceful shutdown via Ctrl+C

### NautilusTrader Note

NautilusTrader's Binance Adapter is Python-only and cannot be used directly
from Rust. This project implements a Rust-native alternative that directly
integrates with Binance REST and WebSocket APIs, following the same trading
concepts (strategy, order management, event handling).

## Prerequisites

- Rust 1.75+ (`rustup update stable`)
- Binance Spot Testnet API keys ([testnet.binance.vision](https://testnet.binance.vision/))
- macOS or Linux

## Setup

1. Clone and enter the project:
   ```bash
   cd sandbox-quant
   ```

2. Copy the environment template and add your testnet keys:
   ```bash
   cp .env.example .env
   ```

3. Edit `.env` with your Binance **Testnet** credentials:
   ```
   BINANCE_API_KEY=your_testnet_api_key_here
   BINANCE_API_SECRET=your_testnet_api_secret_here
   ```

4. Build:
   ```bash
   cargo build --release
   ```

5. Run:
   ```bash
   cargo run --release
   ```

## Environment Variables

| Variable | Required | Description |
|----------|----------|-------------|
| `BINANCE_API_KEY` | Yes | Binance Spot Testnet API key |
| `BINANCE_API_SECRET` | Yes | Binance Spot Testnet API secret |
| `RUST_LOG` | No | Override log level (e.g., `debug`, `sandbox_quant=trace`) |

## Configuration

Edit `config/default.toml` to change:

```toml
[binance]
rest_base_url = "https://testnet.binance.vision"  # Testnet REST endpoint
ws_base_url = "wss://testnet.binance.vision/ws"   # Testnet WebSocket endpoint
symbol = "BTCUSDT"                                  # Trading pair
recv_window = 5000                                  # Request timestamp tolerance (ms)

[strategy]
fast_period = 10       # Fast SMA period (in ticks)
slow_period = 30       # Slow SMA period (in ticks)
order_qty = 0.001      # Order quantity in BTC
min_ticks_between_signals = 50  # Cooldown between signals

[ui]
refresh_rate_ms = 100  # TUI refresh interval
price_history_len = 120  # Price points shown in chart
```

### Symbol Configuration

Change `symbol` in `config/default.toml`. The WebSocket stream is derived
automatically from the symbol (lowercased + `@trade`).

Common testnet pairs: `BTCUSDT`, `ETHUSDT`, `BNBUSDT`.

## Terminal Dashboard

```
 sandbox-quant | BTCUSDT | CONNECTED | RUNNING | ticks: 1234
┌─ Price (BTCUSDT) ──────────────────┐┌─ Position ──┐
│  ●                                 ││ Side: LONG   │
│    ●●                            F ││ Qty:  0.001  │
│      ●●●                       S  ││ Entry: 42000 │
│          ●●●●●                     ││ UnrPL: 0.05  │
│                ●●●●                ││ RlzPL: 0.10  │
│                    ●●●             ││ Trades: 3    │
└────────────────────────────────────┘└──────────────┘
┌─ Orders & Signals ─────────────────────────────────┐
│ Signal: BUY 0.00100   Order: FILLED sq-abc @ 42000 │
│ Fast SMA: 42155.30  Slow SMA: 42120.80             │
└────────────────────────────────────────────────────┘
 [Q]uit  [P]ause  [R]esume
```

**Keybinds:**
- `Q` - Quit (graceful shutdown)
- `P` - Pause strategy (stops signal generation, data keeps flowing)
- `R` - Resume strategy

## Logging

Logs are written to `sandbox-quant.log` in structured JSON format to avoid
interfering with the terminal UI. View logs:

```bash
tail -f sandbox-quant.log | jq .
```

## Project Structure

```
sandbox-quant/
├── Cargo.toml
├── .env.example
├── config/default.toml
├── src/
│   ├── main.rs              # Entry point, task orchestration, TUI loop
│   ├── config.rs            # Config loading (.env + TOML)
│   ├── error.rs             # Error types
│   ├── event.rs             # AppEvent enum
│   ├── order_manager.rs     # Order lifecycle state machine
│   ├── model/               # Data models (tick, order, position, signal)
│   ├── binance/             # REST client, WebSocket client, API types
│   ├── indicator/           # SMA indicator (ring buffer)
│   ├── strategy/            # MA crossover strategy
│   └── ui/                  # ratatui chart, dashboard, widgets
└── TESTING.md               # Testing plan
```

## Testing

Run unit tests:
```bash
cargo test
```

Run integration tests (requires .env with testnet keys and network):
```bash
cargo test -- --ignored
```

See [TESTING.md](TESTING.md) for the full testing plan.

## Automation (Hourly)

This repository includes a scheduled GitHub Actions workflow:

- `.github/workflows/periodic-maintenance.yml`

What it does every hour:

- Runs `cargo fmt --all` and opens a PR if formatting changes are found
- Bumps crate patch version in `Cargo.toml` (e.g. `0.1.0` -> `0.1.1`)
- Runs `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- Runs `cargo test --workspace --all-targets --all-features`
- Opens (or updates) a health-check issue when clippy/tests fail

You can also run it manually with `workflow_dispatch` from the Actions tab.

## Hourly Exchange/Product PR Automation

This repository also includes an hourly scheduler for expanding demo venue coverage:

- `.github/workflows/hourly-market-catalog-pr.yml`

What it does every hour:

- Runs `scripts/validate_market_catalog.sh` to fail early on malformed catalog data
- Runs `scripts/hourly_market_update.sh`
- Adds the next not-yet-registered exchange/product candidate from `data/demo_market_backlog.csv`
- Updates `data/demo_market_registry.csv` and `docs/hourly-market-catalog.md`
- Opens a PR from a fresh branch `chore/hourly-market-catalog/<run_id>`

## Multi-Broker Demo Probe (Stocks/Options)

To validate non-crypto paper/sandbox venues before full adapter work, run:

```bash
cargo run --bin demo_broker_probe
```

Environment variables:

- `ALPACA_PAPER_API_KEY`, `ALPACA_PAPER_API_SECRET`
- `TRADIER_SANDBOX_TOKEN`

Optional endpoint overrides:

- `ALPACA_PAPER_BASE_URL` (default: `https://paper-api.alpaca.markets`)
- `TRADIER_SANDBOX_BASE_URL` (default: `https://sandbox.tradier.com/v1`)

See `docs/multi-broker-demo-integration.md` for scope and roadmap.
