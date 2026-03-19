CREATE TABLE IF NOT EXISTS schema_metadata (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS raw_liquidation_events (
  event_id BIGSERIAL PRIMARY KEY,
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
ON raw_liquidation_events (product, symbol, event_time, force_side, price, qty);

CREATE TABLE IF NOT EXISTS raw_book_ticker (
  tick_id BIGSERIAL PRIMARY KEY,
  symbol TEXT NOT NULL,
  event_time TIMESTAMPTZ NOT NULL,
  receive_time TIMESTAMPTZ NOT NULL,
  bid DOUBLE PRECISION NOT NULL,
  bid_qty DOUBLE PRECISION NOT NULL,
  ask DOUBLE PRECISION NOT NULL,
  ask_qty DOUBLE PRECISION NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS raw_book_ticker_natural_idx
ON raw_book_ticker (symbol, event_time, bid, ask, bid_qty, ask_qty);

CREATE TABLE IF NOT EXISTS raw_agg_trades (
  trade_id BIGSERIAL PRIMARY KEY,
  symbol TEXT NOT NULL,
  event_time TIMESTAMPTZ NOT NULL,
  receive_time TIMESTAMPTZ NOT NULL,
  price DOUBLE PRECISION NOT NULL,
  qty DOUBLE PRECISION NOT NULL,
  is_buyer_maker BOOLEAN NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS raw_agg_trades_natural_idx
ON raw_agg_trades (symbol, event_time, price, qty, is_buyer_maker);

CREATE TABLE IF NOT EXISTS raw_klines (
  kline_id BIGSERIAL PRIMARY KEY,
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
ON raw_klines (product, symbol, interval_name, open_time);

CREATE TABLE IF NOT EXISTS backtest_runs (
  export_run_id BIGSERIAL PRIMARY KEY,
  source_db_path TEXT NOT NULL,
  source_run_id BIGINT NOT NULL,
  exported_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
  mode TEXT NOT NULL,
  template TEXT NOT NULL,
  instrument TEXT NOT NULL,
  from_date DATE NOT NULL,
  to_date DATE NOT NULL,
  liquidation_events BIGINT NOT NULL,
  book_ticker_events BIGINT NOT NULL,
  agg_trade_events BIGINT NOT NULL,
  derived_kline_1s_bars BIGINT NOT NULL,
  trigger_count BIGINT NOT NULL,
  closed_trades BIGINT NOT NULL,
  open_trades BIGINT NOT NULL,
  wins BIGINT NOT NULL,
  losses BIGINT NOT NULL,
  skipped_triggers BIGINT NOT NULL,
  starting_equity DOUBLE PRECISION NOT NULL,
  ending_equity DOUBLE PRECISION NOT NULL,
  net_pnl DOUBLE PRECISION NOT NULL,
  observed_win_rate DOUBLE PRECISION NOT NULL,
  average_net_pnl DOUBLE PRECISION NOT NULL,
  configured_expected_value DOUBLE PRECISION NOT NULL,
  risk_pct DOUBLE PRECISION NOT NULL,
  win_rate_assumption DOUBLE PRECISION NOT NULL,
  r_multiple DOUBLE PRECISION NOT NULL,
  max_entry_slippage_pct DOUBLE PRECISION NOT NULL,
  stop_distance_pct DOUBLE PRECISION NOT NULL,
  UNIQUE (source_db_path, source_run_id)
);

CREATE INDEX IF NOT EXISTS backtest_runs_mode_lookup_idx
ON backtest_runs (mode, instrument, template, export_run_id DESC);

CREATE TABLE IF NOT EXISTS backtest_trades (
  export_run_id BIGINT NOT NULL REFERENCES backtest_runs (export_run_id) ON DELETE CASCADE,
  trade_id BIGINT NOT NULL,
  trigger_time TIMESTAMPTZ NOT NULL,
  entry_time TIMESTAMPTZ NOT NULL,
  entry_price DOUBLE PRECISION NOT NULL,
  stop_price DOUBLE PRECISION NOT NULL,
  take_profit_price DOUBLE PRECISION NOT NULL,
  qty DOUBLE PRECISION NOT NULL,
  exit_time TIMESTAMPTZ,
  exit_price DOUBLE PRECISION,
  exit_reason TEXT,
  gross_pnl DOUBLE PRECISION,
  fees DOUBLE PRECISION,
  net_pnl DOUBLE PRECISION,
  PRIMARY KEY (export_run_id, trade_id)
);

CREATE INDEX IF NOT EXISTS backtest_trades_exit_time_lookup_idx
ON backtest_trades (export_run_id, exit_time);

CREATE TABLE IF NOT EXISTS backtest_equity_points (
  export_run_id BIGINT NOT NULL REFERENCES backtest_runs (export_run_id) ON DELETE CASCADE,
  point_id BIGINT NOT NULL,
  event_time TIMESTAMPTZ NOT NULL,
  equity DOUBLE PRECISION NOT NULL,
  cumulative_net_pnl DOUBLE PRECISION NOT NULL,
  PRIMARY KEY (export_run_id, point_id)
);

CREATE INDEX IF NOT EXISTS backtest_equity_points_time_lookup_idx
ON backtest_equity_points (export_run_id, event_time);

CREATE TABLE IF NOT EXISTS backtest_runs (
  export_run_id BIGSERIAL PRIMARY KEY,
  source_db_path TEXT NOT NULL,
  source_run_id BIGINT NOT NULL,
  exported_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
  mode TEXT NOT NULL,
  template TEXT NOT NULL,
  instrument TEXT NOT NULL,
  from_date DATE NOT NULL,
  to_date DATE NOT NULL,
  liquidation_events BIGINT NOT NULL,
  book_ticker_events BIGINT NOT NULL,
  agg_trade_events BIGINT NOT NULL,
  derived_kline_1s_bars BIGINT NOT NULL,
  trigger_count BIGINT NOT NULL,
  closed_trades BIGINT NOT NULL,
  open_trades BIGINT NOT NULL,
  wins BIGINT NOT NULL,
  losses BIGINT NOT NULL,
  skipped_triggers BIGINT NOT NULL,
  starting_equity DOUBLE PRECISION NOT NULL,
  ending_equity DOUBLE PRECISION NOT NULL,
  net_pnl DOUBLE PRECISION NOT NULL,
  observed_win_rate DOUBLE PRECISION NOT NULL,
  average_net_pnl DOUBLE PRECISION NOT NULL,
  configured_expected_value DOUBLE PRECISION NOT NULL,
  risk_pct DOUBLE PRECISION NOT NULL,
  win_rate_assumption DOUBLE PRECISION NOT NULL,
  r_multiple DOUBLE PRECISION NOT NULL,
  max_entry_slippage_pct DOUBLE PRECISION NOT NULL,
  stop_distance_pct DOUBLE PRECISION NOT NULL,
  UNIQUE (source_db_path, source_run_id)
);

CREATE INDEX IF NOT EXISTS backtest_runs_mode_lookup_idx
ON backtest_runs (mode, instrument, template, export_run_id DESC);

CREATE TABLE IF NOT EXISTS backtest_trades (
  export_run_id BIGINT NOT NULL REFERENCES backtest_runs (export_run_id) ON DELETE CASCADE,
  trade_id BIGINT NOT NULL,
  trigger_time TIMESTAMPTZ NOT NULL,
  entry_time TIMESTAMPTZ NOT NULL,
  entry_price DOUBLE PRECISION NOT NULL,
  stop_price DOUBLE PRECISION NOT NULL,
  take_profit_price DOUBLE PRECISION NOT NULL,
  qty DOUBLE PRECISION NOT NULL,
  exit_time TIMESTAMPTZ,
  exit_price DOUBLE PRECISION,
  exit_reason TEXT,
  gross_pnl DOUBLE PRECISION,
  fees DOUBLE PRECISION,
  net_pnl DOUBLE PRECISION,
  PRIMARY KEY (export_run_id, trade_id)
);

CREATE INDEX IF NOT EXISTS backtest_trades_exit_time_lookup_idx
ON backtest_trades (export_run_id, exit_time);

CREATE TABLE IF NOT EXISTS backtest_equity_points (
  export_run_id BIGINT NOT NULL REFERENCES backtest_runs (export_run_id) ON DELETE CASCADE,
  point_id BIGINT NOT NULL,
  event_time TIMESTAMPTZ NOT NULL,
  equity DOUBLE PRECISION NOT NULL,
  cumulative_net_pnl DOUBLE PRECISION NOT NULL,
  PRIMARY KEY (export_run_id, point_id)
);

CREATE INDEX IF NOT EXISTS backtest_equity_points_time_lookup_idx
ON backtest_equity_points (export_run_id, event_time);

INSERT INTO schema_metadata (key, value, updated_at)
VALUES ('market_data_schema_version', '1', CURRENT_TIMESTAMP)
ON CONFLICT (key) DO UPDATE
SET value = EXCLUDED.value, updated_at = EXCLUDED.updated_at;
