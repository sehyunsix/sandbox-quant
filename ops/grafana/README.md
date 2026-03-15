# sandbox-quant Grafana Starter

This folder provides a minimal PostgreSQL + Grafana starter for operational dashboards.

## What this gives you

- local PostgreSQL via Docker
- local Grafana via Docker
- provisioned PostgreSQL datasource
- starter dashboards:
  - `sandbox-quant overview`
  - `sandbox-quant backtest pnl`

## Start

Create a local `ops/grafana/.env.example` first. Keep it untracked.

Example contents:

```bash
POSTGRES_DB=your-postgres-db
POSTGRES_USER=your-postgres-user
POSTGRES_PASSWORD=your-postgres-password

GF_SECURITY_ADMIN_USER=your-grafana-admin-user
GF_SECURITY_ADMIN_PASSWORD=your-grafana-admin-password
```

```bash
cd ops/grafana
docker compose up -d
```

When you want shell commands to reuse the same password values, load `.env.example` into your shell first:

```bash
cd ops/grafana
set -a
source .env.example
set +a
```

If you already created the PostgreSQL volume before the init SQL existed and Grafana shows:

- `Error updating options: error when executing the sql query`

then either:

1. bootstrap the schema manually:

```bash
export SANDBOX_QUANT_POSTGRES_URL="postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@localhost:5432/${POSTGRES_DB}"
cargo run --bin sandbox-quant-collector -- summary --storage postgres
```

or

2. recreate the PostgreSQL volume so init scripts run again:

```bash
cd ops/grafana
docker compose down -v
docker compose up -d
```

If the datasource itself is healthy but panels still fail to load after provisioning, verify the dashboard JSON uses `rawSql` for PostgreSQL panel targets. Grafana's PostgreSQL datasource does not execute provisioned panel SQL correctly when the target stores the query under `rawCode`.

Default endpoints:

- Grafana: `http://localhost:3000`
- PostgreSQL: `localhost:5432`

Credentials:

- Both services load `ops/grafana/.env.example` through `env_file`
- Keep `ops/grafana/.env.example` local only; it is gitignored
- Set every required PostgreSQL and Grafana variable explicitly in that file

## Initialize market-data schema in PostgreSQL

The app code initializes the PostgreSQL schema when PostgreSQL-backed collector/recorder flows run.
The Docker starter also ships an init SQL file so fresh PostgreSQL volumes start with the required tables.
An easy way to bootstrap schema + verify connectivity is:

```bash
export SANDBOX_QUANT_POSTGRES_URL="postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@localhost:5432/${POSTGRES_DB}"
cargo run --bin sandbox-quant-collector -- summary --storage postgres
```

## Load data into PostgreSQL

Example historical import:

```bash
export SANDBOX_QUANT_POSTGRES_URL="postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@localhost:5432/${POSTGRES_DB}"
cargo run --bin sandbox-quant-collector -- \
  binance-public import \
  --products um \
  --symbols BTCUSDT,ETHUSDT \
  --from 2026-03-12 \
  --to 2026-03-13 \
  --kline-interval 1m \
  --storage postgres
```

## Provisioned dashboard

The starter dashboard includes:

- Price (`raw_klines.close`)
- Volume (`raw_klines.volume`)
- Liquidation event counts (`raw_liquidation_events`)
- Kline coverage table
- Data Health table
- Recent Event Times table

Dashboard variables:

- `mode`
- `symbol` with multi-select support
- `interval` as `1m`, `15m`, `30m`, `1h`

Current sample expectation:

- mode: `demo`
- symbol: `BTCUSDT` or multiple symbols together
- interval: `15m`
- default dashboard time range: `now-3y`

The starter dashboard now uses `raw_klines` rows stored at `interval_name='1m'` as the source of truth for price and volume charts.
Higher intervals such as `15m`, `30m`, and `1h` are aggregated at query time in Grafana, so separate PostgreSQL backfills for those higher intervals are not required for charting.
When multiple symbols are selected together, the top chart shows relative return in percent from the first visible point so one-year performance can be compared on the same scale.
The wider default range also keeps older series such as `USD/KRW` visible without requiring a manual time-range change first.

## Backtest PnL dashboard

The `sandbox-quant backtest pnl` dashboard reads exported backtest runs from PostgreSQL tables:

- `backtest_runs`
- `backtest_trades`
- `backtest_equity_points`

Export a persisted DuckDB backtest run into PostgreSQL with:

```bash
export SANDBOX_QUANT_POSTGRES_URL="postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@localhost:5432/${POSTGRES_DB}"
cargo run --bin sandbox-quant-backtest -- \
  export postgres latest \
  --mode demo \
  --base-dir var
```

After export, the dashboard lets you filter by:

- `mode`
- `instrument`
- `template`
- `run`

## Copy/paste queries for Grafana panel editor

### 1. Data health

```sql
SELECT 'raw_klines' AS table_name, count(*) AS row_count FROM raw_klines
UNION ALL
SELECT 'raw_liquidation_events' AS table_name, count(*) AS row_count FROM raw_liquidation_events
UNION ALL
SELECT 'raw_book_ticker' AS table_name, count(*) AS row_count FROM raw_book_ticker
UNION ALL
SELECT 'raw_agg_trades' AS table_name, count(*) AS row_count FROM raw_agg_trades
ORDER BY table_name
```

### 2. Latest timestamps

```sql
SELECT 'raw_klines' AS table_name, max(close_time) AS latest_time FROM raw_klines
UNION ALL
SELECT 'raw_liquidation_events' AS table_name, max(event_time) AS latest_time FROM raw_liquidation_events
UNION ALL
SELECT 'raw_book_ticker' AS table_name, max(event_time) AS latest_time FROM raw_book_ticker
UNION ALL
SELECT 'raw_agg_trades' AS table_name, max(event_time) AS latest_time FROM raw_agg_trades
ORDER BY table_name
```

### 3. Symbol coverage

```sql
SELECT
  product,
  symbol,
  interval_name,
  count(*) AS row_count,
  min(open_time) AS first_open_time,
  max(close_time) AS last_close_time
FROM raw_klines
GROUP BY 1, 2, 3
ORDER BY row_count DESC
LIMIT 20
```

### 4. Relative return series

```sql
WITH bucketed AS (
  SELECT
    date_bin(
      CASE ${interval:sqlstring}
        WHEN '1m' THEN INTERVAL '1 minute'
        WHEN '15m' THEN INTERVAL '15 minutes'
        WHEN '30m' THEN INTERVAL '30 minutes'
        WHEN '1h' THEN INTERVAL '1 hour'
        ELSE INTERVAL '1 minute'
      END,
      open_time,
      TIMESTAMPTZ '1970-01-01 00:00:00+00'
    ) AS time,
    symbol,
    close,
    close_time
  FROM raw_klines
  WHERE mode = ${mode:sqlstring}
    AND symbol IN (${symbol:sqlstring})
    AND interval_name = '1m'
    AND $__timeFilter(open_time)
), ranked AS (
  SELECT
    time,
    symbol,
    close AS close_price,
    row_number() OVER (PARTITION BY symbol, time ORDER BY close_time DESC) AS row_num
  FROM bucketed
), sampled AS (
  SELECT
    time,
    symbol,
    close_price
  FROM ranked
  WHERE row_num = 1
)
SELECT
  time,
  symbol,
  ((close_price / nullif(first_value(close_price) OVER (PARTITION BY symbol ORDER BY time), 0)) - 1) * 100 AS relative_return_pct
FROM sampled
ORDER BY 1, 2
```

## Notes

- This is an **operational starter**, not a full production deployment.
- Keep `ops/grafana/.env.example` local only; it is gitignored.
- Grafana reads dashboard JSON from `ops/grafana/dashboards/`.
- Datasource provisioning lives in `ops/grafana/provisioning/datasources/`.
