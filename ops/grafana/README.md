# sandbox-quant Grafana Starter

This folder provides a minimal PostgreSQL + Grafana starter for operational dashboards.

## What this gives you

- local PostgreSQL via Docker
- local Grafana via Docker
- provisioned PostgreSQL datasource
- a starter dashboard (`sandbox-quant overview`)

## Start

```bash
cd ops/grafana
docker compose up -d
```

If you already created the PostgreSQL volume before the init SQL existed and Grafana shows:

- `Error updating options: error when executing the sql query`

then either:

1. bootstrap the schema manually:

```bash
export SANDBOX_QUANT_POSTGRES_URL=postgres://sandbox_quant:sandbox_quant@localhost:5432/sandbox_quant
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

Default credentials:

- Grafana user: `admin`
- Grafana password: `admin`
- PostgreSQL db: `sandbox_quant`
- PostgreSQL user: `sandbox_quant`
- PostgreSQL password: `sandbox_quant`

## Initialize market-data schema in PostgreSQL

The app code initializes the PostgreSQL schema when PostgreSQL-backed collector/recorder flows run.
The Docker starter also ships an init SQL file so fresh PostgreSQL volumes start with the required tables.
An easy way to bootstrap schema + verify connectivity is:

```bash
export SANDBOX_QUANT_POSTGRES_URL=postgres://sandbox_quant:sandbox_quant@localhost:5432/sandbox_quant
cargo run --bin sandbox-quant-collector -- summary --storage postgres
```

## Load data into PostgreSQL

Example historical import:

```bash
export SANDBOX_QUANT_POSTGRES_URL=postgres://sandbox_quant:sandbox_quant@localhost:5432/sandbox_quant
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
- `symbol`
- `interval`

Current sample expectation:

- mode: `demo`
- symbol: `BTCUSDT`
- interval: `15m`
- default dashboard time range: `now-30d`

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

### 4. Price series

```sql
SELECT
  $__timeGroupAlias(open_time, $__interval),
  avg(close) AS close_price
FROM raw_klines
WHERE mode = ${mode:sqlstring}
  AND symbol = ${symbol:sqlstring}
  AND interval_name = ${interval:sqlstring}
  AND $__timeFilter(open_time)
GROUP BY 1
ORDER BY 1
```

## Notes

- This is an **operational starter**, not a full production deployment.
- For production, move credentials into a `.env` file or secret manager.
- Grafana reads dashboard JSON from `ops/grafana/dashboards/`.
- Datasource provisioning lives in `ops/grafana/provisioning/datasources/`.
