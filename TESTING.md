# Testing Plan

## 1. Unit Tests

### Strategy Signal Logic
| Test | File | Verifies |
|------|------|----------|
| `insufficient_data_returns_hold` | `strategy/ma_crossover.rs` | First N ticks (< slow_period) yield Hold |
| `buy_signal_on_bullish_crossover` | `strategy/ma_crossover.rs` | Fast SMA crosses above slow → Buy |
| `sell_signal_on_bearish_crossover` | `strategy/ma_crossover.rs` | Fast SMA crosses below slow → Sell |
| `no_double_buy` | `strategy/ma_crossover.rs` | Already Long, bullish crossover → Hold |
| `cooldown_prevents_rapid_signals` | `strategy/ma_crossover.rs` | Signal within cooldown window → Hold |

### Indicator Calculations
| Test | File | Verifies |
|------|------|----------|
| `basic_sma` | `indicator/sma.rs` | SMA of [1,2,3,4,5] with period=3 |
| `single_period` | `indicator/sma.rs` | Period=1 returns each value immediately |
| `ring_buffer_wraps_correctly` | `indicator/sma.rs` | Buffer wraps after > period values |
| `no_drift_after_many_pushes` | `indicator/sma.rs` | No float drift after 10K pushes vs naive |
| `zero_period_panics` | `indicator/sma.rs` | Period=0 panics at construction |
| `value_without_push` | `indicator/sma.rs` | `value()` returns None before ready |

### Config Loading
| Test | File | Verifies |
|------|------|----------|
| `parse_default_toml` | `config.rs` | TOML parsing produces correct Config struct |

### Order State Transitions
| Test | File | Verifies |
|------|------|----------|
| `valid_state_transitions` | `order_manager.rs` | PendingSubmit→Submitted→Filled valid |
| `from_binance_str_mapping` | `order_manager.rs` | Binance status strings map correctly |

### Binance Types
| Test | File | Verifies |
|------|------|----------|
| `deserialize_trade_event` | `binance/types.rs` | Parse Binance trade JSON with string→f64 |
| `deserialize_order_response` | `binance/types.rs` | Parse full order response with fills |
| `hmac_signing_produces_hex_signature` | `binance/rest.rs` | Signed query has correct format |
| `hmac_known_vector` | `binance/rest.rs` | HMAC matches Binance docs example |

### Position PnL
| Test | File | Verifies |
|------|------|----------|
| `open_and_close_long` | `model/position.rs` | Buy then sell calculates realized PnL |
| `unrealized_pnl_updates` | `model/position.rs` | Mark-to-market PnL updates correctly |

Run: `cargo test`

## 2. Integration Tests (Binance Testnet)

These tests require network access and valid testnet API keys in `.env`.
They are marked `#[ignore]` and run explicitly.

| Test | What it verifies |
|------|-----------------|
| WebSocket connect | Connect to `wss://testnet.binance.vision/ws/btcusdt@trade`, receive ≥1 message |
| REST ping | `GET /api/v3/ping` returns `{}` |
| REST server time | `GET /api/v3/time` returns timestamp within 5s of local time |
| REST account info | `GET /api/v3/account` returns balances with testnet keys |
| Place and cancel | Place LIMIT BUY far below market → query → cancel → verify CANCELED |
| Market order fill | Place MARKET BUY 0.001 BTCUSDT → verify FILLED response |

Run: `cargo test -- --ignored`

## 3. Failure Scenario Tests

| Scenario | How to test |
|----------|------------|
| WebSocket disconnect | Mock WS server closes after N messages; verify reconnect with backoff |
| REST 429 (rate limit) | Mock returns 429 with `Retry-After`; verify client backs off |
| REST -1021 (timestamp) | Mock returns `{"code":-1021,"msg":"Timestamp..."}`; verify error handling |
| REST -2010 (insufficient balance) | Mock returns error; verify order → Rejected |
| Malformed WS JSON | Send garbage on WS; verify no panic, error logged |
| Channel full | Fill mpsc channel (256 capacity); verify `try_send` drops gracefully |
| Invalid API key | Set invalid key; verify helpful error message, no crash |
| Missing .env | Remove .env; verify error message mentions BINANCE_API_KEY |

## 4. Determinism Checks

| Test | File | Verifies |
|------|------|----------|
| `deterministic_output` | `strategy/ma_crossover.rs` | Same price sequence → identical signal sequence across 2 runs |
| `no_drift_after_many_pushes` | `indicator/sma.rs` | Ring-buffer SMA matches naive implementation after 10K ticks |

Given a fixed stream of 200 price points (sinusoidal pattern), the strategy
produces the exact same sequence of Buy/Sell/Hold signals on every run.

## 5. Observability Checks

| What | How to verify |
|------|--------------|
| Order IDs in logs | `grep "order_id" sandbox-quant.log` returns entries for every order |
| Structured JSON format | `jq . sandbox-quant.log` parses successfully (each line is valid JSON) |
| Error classification | Errors include `error` field with descriptive message and context chain |
| Strategy events | Logs contain `Placing market order` with symbol, side, quantity, client_order_id |
| WS connection events | Logs contain `WebSocket connected` and `WebSocket disconnected` |

## 6. Manual QA Checklist

- [ ] Start app → TUI renders, status bar shows "CONNECTED"
- [ ] Price chart populates within seconds, dots scroll left as new ticks arrive
- [ ] Wait for crossover signal → "Signal: BUY" appears in order panel
- [ ] Order fills → "Order: FILLED" appears, position panel updates
- [ ] Press `P` → status changes to "PAUSED", no new signals generated
- [ ] Press `R` → status changes to "RUNNING", signals resume
- [ ] Kill network (e.g., disable Wi-Fi) → status changes to "DISCONNECTED"
- [ ] Restore network → status returns to "CONNECTED", chart resumes
- [ ] Press `Q` → app exits cleanly, prints "Goodbye!" message
- [ ] Run with invalid API key → error message displayed, no crash
- [ ] Run without `.env` → error mentions BINANCE_API_KEY, exits gracefully
- [ ] Resize terminal → layout adjusts, chart and panels remain readable
- [ ] Check `sandbox-quant.log` → entries are valid JSON with timestamps
- [ ] Position PnL updates on each tick while position is open
- [ ] Realized PnL accumulates across multiple trades
