# RFC 0060: Binance Public Data Collector

## Status
Draft

## Summary

Add a separate `sandbox-quant-collector` binary for backfilling historical market data from Binance Data Collection.

The collector is not a live recorder. It is a one-shot importer for downloadable public datasets such as:

- `futures/*/daily/liquidationSnapshot/...`
- `futures/*/daily/klines/...`

The first implementation imports:

- liquidation snapshots
- OHLCV + volume from klines

into the same DuckDB dataset used by recorder and backtest.

## Why

The live recorder is necessary for forward collection, but it does not solve historical backfill.

We need a separate path for:

- historical liquidation backfill
- historical OHLCV/volume backfill
- seeding datasets before forward collection starts

## Ownership

- `sandbox-quant-recorder`
  - live forward collection
- `sandbox-quant-collector`
  - historical backfill
- `sandbox-quant-backtest`
  - read-only consumer of the combined dataset

## Scope

First version supports:

- `um` and `cm`
- one date at a time
- one symbol at a time
- optional liquidation import
- optional kline import

## Dataset integration

New historical kline data is stored in a dedicated `raw_klines` table.

Liquidation snapshots are normalized into `raw_liquidation_events`.

## Command shape

```text
sandbox-quant-collector binance-public import \
  --product um \
  --symbol BTCUSDT \
  --date 2026-03-13 \
  --kline-interval 1m \
  --base-dir var
```

## Consequences

Positive:

- historical backfill path is separated from live recorder
- OHLCV and liquidation data can be seeded before live collection
- backtest can operate on a wider historical dataset

Trade-offs:

- public data format drift must be handled carefully
- historical liquidation snapshots may have gaps depending on Binance dataset availability
