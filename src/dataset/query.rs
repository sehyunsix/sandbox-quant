use std::path::Path;

use duckdb::{params, Connection};

use crate::app::bootstrap::BinanceMode;
use crate::dataset::types::{BacktestDatasetSummary, RecorderMetrics};
use crate::error::storage_error::StorageError;

pub fn metrics_for_path(db_path: &Path) -> Result<RecorderMetrics, StorageError> {
    if !db_path.exists() {
        return Ok(RecorderMetrics::default());
    }
    let connection =
        Connection::open(db_path).map_err(|error| StorageError::DatabaseInitFailed {
            path: db_path.display().to_string(),
            message: error.to_string(),
        })?;

    Ok(RecorderMetrics {
        liquidation_events: query_count(&connection, "raw_liquidation_events")?,
        book_ticker_events: query_count(&connection, "raw_book_ticker")?,
        agg_trade_events: query_count(&connection, "raw_agg_trades")?,
        derived_kline_1s_bars: query_count(&connection, "derived_kline_1s")?,
        last_liquidation_event_time: query_latest_timestamp(
            &connection,
            "raw_liquidation_events",
            "event_time",
        )?,
        last_book_ticker_event_time: query_latest_timestamp(
            &connection,
            "raw_book_ticker",
            "event_time",
        )?,
        last_agg_trade_event_time: query_latest_timestamp(
            &connection,
            "raw_agg_trades",
            "event_time",
        )?,
        top_liquidation_symbols: query_top_symbols(&connection, "raw_liquidation_events")?,
        top_book_ticker_symbols: query_top_symbols(&connection, "raw_book_ticker")?,
        top_agg_trade_symbols: query_top_symbols(&connection, "raw_agg_trades")?,
    })
}

pub fn backtest_summary_for_path(
    db_path: &Path,
    mode: BinanceMode,
    symbol: &str,
    from: chrono::NaiveDate,
    to: chrono::NaiveDate,
) -> Result<BacktestDatasetSummary, StorageError> {
    if !db_path.exists() {
        return Ok(BacktestDatasetSummary {
            mode,
            symbol: symbol.to_string(),
            from: from.to_string(),
            to: to.to_string(),
            liquidation_events: 0,
            book_ticker_events: 0,
            agg_trade_events: 0,
            derived_kline_1s_bars: 0,
        });
    }
    let connection =
        Connection::open(db_path).map_err(|error| StorageError::DatabaseInitFailed {
            path: db_path.display().to_string(),
            message: error.to_string(),
        })?;
    let from_ts = format!("{from} 00:00:00");
    let to_ts = format!("{to} 23:59:59");
    Ok(BacktestDatasetSummary {
        mode,
        symbol: symbol.to_string(),
        from: from.to_string(),
        to: to.to_string(),
        liquidation_events: query_count_in_range(
            &connection,
            "raw_liquidation_events",
            "event_time",
            symbol,
            &from_ts,
            &to_ts,
        )?,
        book_ticker_events: query_count_in_range(
            &connection,
            "raw_book_ticker",
            "event_time",
            symbol,
            &from_ts,
            &to_ts,
        )?,
        agg_trade_events: query_count_in_range(
            &connection,
            "raw_agg_trades",
            "event_time",
            symbol,
            &from_ts,
            &to_ts,
        )?,
        derived_kline_1s_bars: query_count_in_range(
            &connection,
            "derived_kline_1s",
            "open_time",
            symbol,
            &from_ts,
            &to_ts,
        )?,
    })
}

fn query_count(connection: &Connection, table: &str) -> Result<u64, StorageError> {
    let sql = format!("SELECT COUNT(*) FROM {table}");
    let mut statement =
        connection
            .prepare(&sql)
            .map_err(|error| StorageError::WriteFailedWithContext {
                message: error.to_string(),
            })?;
    let count: i64 = statement.query_row([], |row| row.get(0)).map_err(|error| {
        StorageError::WriteFailedWithContext {
            message: error.to_string(),
        }
    })?;
    Ok(count.max(0) as u64)
}

fn query_latest_timestamp(
    connection: &Connection,
    table: &str,
    column: &str,
) -> Result<Option<String>, StorageError> {
    let sql = format!("SELECT CAST(MAX({column}) AS VARCHAR) FROM {table}");
    let mut statement =
        connection
            .prepare(&sql)
            .map_err(|error| StorageError::WriteFailedWithContext {
                message: error.to_string(),
            })?;
    let value: Option<String> = statement.query_row([], |row| row.get(0)).map_err(|error| {
        StorageError::WriteFailedWithContext {
            message: error.to_string(),
        }
    })?;
    Ok(value)
}

fn query_top_symbols(connection: &Connection, table: &str) -> Result<Vec<String>, StorageError> {
    let sql = format!(
        "SELECT symbol, COUNT(*) AS row_count FROM {table} GROUP BY symbol ORDER BY row_count DESC, symbol ASC LIMIT 5"
    );
    let mut statement =
        connection
            .prepare(&sql)
            .map_err(|error| StorageError::WriteFailedWithContext {
                message: error.to_string(),
            })?;
    let mut rows = statement
        .query([])
        .map_err(|error| StorageError::WriteFailedWithContext {
            message: error.to_string(),
        })?;
    let mut result = Vec::new();
    while let Some(row) = rows
        .next()
        .map_err(|error| StorageError::WriteFailedWithContext {
            message: error.to_string(),
        })?
    {
        let symbol: String = row
            .get(0)
            .map_err(|error| StorageError::WriteFailedWithContext {
                message: error.to_string(),
            })?;
        let row_count: i64 = row
            .get(1)
            .map_err(|error| StorageError::WriteFailedWithContext {
                message: error.to_string(),
            })?;
        result.push(format!("{symbol}:{row_count}"));
    }
    Ok(result)
}

fn query_count_in_range(
    connection: &Connection,
    table: &str,
    time_column: &str,
    symbol: &str,
    from_ts: &str,
    to_ts: &str,
) -> Result<u64, StorageError> {
    let sql = format!(
        "SELECT COUNT(*) FROM {table} WHERE symbol = ? AND {time_column} >= CAST(? AS TIMESTAMP) AND {time_column} <= CAST(? AS TIMESTAMP)"
    );
    let mut statement =
        connection
            .prepare(&sql)
            .map_err(|error| StorageError::WriteFailedWithContext {
                message: error.to_string(),
            })?;
    let count: i64 = statement
        .query_row(params![symbol, from_ts, to_ts], |row| row.get(0))
        .map_err(|error| StorageError::WriteFailedWithContext {
            message: error.to_string(),
        })?;
    Ok(count.max(0) as u64)
}
