CREATE TABLE IF NOT EXISTS schema_metadata (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS raw_liquidation_events (
  event_id BIGSERIAL PRIMARY KEY,
  mode TEXT NOT NULL,
  product TEXT NOT NULL,
  symbol TEXT NOT NULL,
  event_time TIMESTAMPTZ NOT NULL,
  receive_time TIMESTAMPTZ NOT NULL,
  force_side TEXT NOT NULL,
  price DOUBLE PRECISION NOT NULL,
  qty DOUBLE PRECISION NOT NULL,
  notional DOUBLE PRECISION NOT NULL,
  raw_payload TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS raw_liquidation_events_natural_idx
ON raw_liquidation_events (mode, product, symbol, event_time, force_side, price, qty);

CREATE TABLE IF NOT EXISTS raw_book_ticker (
  tick_id BIGSERIAL PRIMARY KEY,
  mode TEXT NOT NULL,
  symbol TEXT NOT NULL,
  event_time TIMESTAMPTZ NOT NULL,
  receive_time TIMESTAMPTZ NOT NULL,
  bid DOUBLE PRECISION NOT NULL,
  bid_qty DOUBLE PRECISION NOT NULL,
  ask DOUBLE PRECISION NOT NULL,
  ask_qty DOUBLE PRECISION NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS raw_book_ticker_natural_idx
ON raw_book_ticker (mode, symbol, event_time, bid, ask, bid_qty, ask_qty);

CREATE TABLE IF NOT EXISTS raw_agg_trades (
  trade_id BIGSERIAL PRIMARY KEY,
  mode TEXT NOT NULL,
  symbol TEXT NOT NULL,
  event_time TIMESTAMPTZ NOT NULL,
  receive_time TIMESTAMPTZ NOT NULL,
  price DOUBLE PRECISION NOT NULL,
  qty DOUBLE PRECISION NOT NULL,
  is_buyer_maker BOOLEAN NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS raw_agg_trades_natural_idx
ON raw_agg_trades (mode, symbol, event_time, price, qty, is_buyer_maker);

CREATE TABLE IF NOT EXISTS raw_klines (
  kline_id BIGSERIAL PRIMARY KEY,
  mode TEXT NOT NULL,
  product TEXT NOT NULL,
  symbol TEXT NOT NULL,
  interval_name TEXT NOT NULL,
  open_time TIMESTAMPTZ NOT NULL,
  close_time TIMESTAMPTZ NOT NULL,
  open DOUBLE PRECISION NOT NULL,
  high DOUBLE PRECISION NOT NULL,
  low DOUBLE PRECISION NOT NULL,
  close DOUBLE PRECISION NOT NULL,
  volume DOUBLE PRECISION NOT NULL,
  quote_volume DOUBLE PRECISION NOT NULL,
  trade_count BIGINT NOT NULL,
  taker_buy_base_volume DOUBLE PRECISION,
  taker_buy_quote_volume DOUBLE PRECISION,
  raw_payload TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS raw_klines_natural_idx
ON raw_klines (mode, product, symbol, interval_name, open_time);

INSERT INTO schema_metadata (key, value, updated_at)
VALUES ('market_data_schema_version', '1', CURRENT_TIMESTAMP)
ON CONFLICT (key) DO UPDATE
SET value = EXCLUDED.value, updated_at = EXCLUDED.updated_at;
