use std::path::Path;

use duckdb::{params, AccessMode, Config, Connection};

use crate::app::bootstrap::BinanceMode;
use crate::backtest_app::runner::{BacktestExitReason, BacktestReport, BacktestTrade};
use crate::dataset::types::{
    BacktestDatasetSummary, BacktestRunSummaryRow, BookTickerRow, DerivedKlineRow,
    LiquidationEventRow, RecorderMetrics,
};
use crate::error::storage_error::StorageError;
use crate::strategy::model::StrategyTemplate;

fn open_dataset_connection_read_only(db_path: &Path) -> Result<Connection, StorageError> {
    let config = Config::default()
        .access_mode(AccessMode::ReadOnly)
        .map_err(storage_err)?;
    Connection::open_with_flags(db_path, config).map_err(|error| StorageError::DatabaseInitFailed {
        path: db_path.display().to_string(),
        message: error.to_string(),
    })
}

fn open_dataset_connection_read_write(db_path: &Path) -> Result<Connection, StorageError> {
    let config = Config::default()
        .access_mode(AccessMode::ReadWrite)
        .map_err(storage_err)?;
    Connection::open_with_flags(db_path, config).map_err(|error| StorageError::DatabaseInitFailed {
        path: db_path.display().to_string(),
        message: error.to_string(),
    })
}

pub fn metrics_for_path(db_path: &Path) -> Result<RecorderMetrics, StorageError> {
    if !db_path.exists() {
        return Ok(RecorderMetrics::default());
    }
    let connection = open_dataset_connection_read_only(db_path)?;

    Ok(RecorderMetrics {
        liquidation_events: query_count(&connection, "raw_liquidation_events")?,
        book_ticker_events: query_count(&connection, "raw_book_ticker")?,
        agg_trade_events: query_count(&connection, "raw_agg_trades")?,
        derived_kline_1s_bars: query_count(&connection, "derived_kline_1s")?,
        schema_version: query_schema_version(&connection)?,
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
            symbol_found: false,
            from: from.to_string(),
            to: to.to_string(),
            liquidation_events: 0,
            book_ticker_events: 0,
            agg_trade_events: 0,
            derived_kline_1s_bars: 0,
        });
    }
    let connection = open_dataset_connection_read_only(db_path)?;
    let symbol_found = market_data_symbol_exists(&connection, symbol)?;
    let from_ts = format!("{from} 00:00:00");
    let to_ts = format!("{to} 23:59:59");
    Ok(BacktestDatasetSummary {
        mode,
        symbol: symbol.to_string(),
        symbol_found,
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

pub fn load_liquidation_events_for_path(
    db_path: &Path,
    symbol: &str,
    from: chrono::NaiveDate,
    to: chrono::NaiveDate,
) -> Result<Vec<LiquidationEventRow>, StorageError> {
    if !db_path.exists() {
        return Ok(Vec::new());
    }
    let connection = open_dataset_connection_read_only(db_path)?;
    let from_ts = format!("{from} 00:00:00");
    let to_ts = format!("{to} 23:59:59");
    let mut statement = connection
        .prepare(
            "SELECT epoch_ms(event_time), force_side, price, qty, notional
             FROM raw_liquidation_events
             WHERE symbol = ? AND event_time >= CAST(? AS TIMESTAMP) AND event_time <= CAST(? AS TIMESTAMP)
             ORDER BY event_time ASC",
        )
        .map_err(|error| StorageError::WriteFailedWithContext {
            message: error.to_string(),
        })?;
    let mut rows = statement
        .query(params![symbol, from_ts, to_ts])
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
        result.push(LiquidationEventRow {
            event_time_ms: row
                .get(0)
                .map_err(|error| StorageError::WriteFailedWithContext {
                    message: error.to_string(),
                })?,
            force_side: row
                .get(1)
                .map_err(|error| StorageError::WriteFailedWithContext {
                    message: error.to_string(),
                })?,
            price: row
                .get(2)
                .map_err(|error| StorageError::WriteFailedWithContext {
                    message: error.to_string(),
                })?,
            qty: row
                .get(3)
                .map_err(|error| StorageError::WriteFailedWithContext {
                    message: error.to_string(),
                })?,
            notional: row
                .get(4)
                .map_err(|error| StorageError::WriteFailedWithContext {
                    message: error.to_string(),
                })?,
        });
    }
    Ok(result)
}

pub fn load_book_ticker_rows_for_path(
    db_path: &Path,
    symbol: &str,
    from: chrono::NaiveDate,
    to: chrono::NaiveDate,
) -> Result<Vec<BookTickerRow>, StorageError> {
    if !db_path.exists() {
        return Ok(Vec::new());
    }
    let connection = open_dataset_connection_read_only(db_path)?;
    let from_ts = format!("{from} 00:00:00");
    let to_ts = format!("{to} 23:59:59");
    let mut statement = connection
        .prepare(
            "SELECT epoch_ms(event_time), bid, ask
             FROM raw_book_ticker
             WHERE symbol = ? AND event_time >= CAST(? AS TIMESTAMP) AND event_time <= CAST(? AS TIMESTAMP)
             ORDER BY event_time ASC",
        )
        .map_err(|error| StorageError::WriteFailedWithContext {
            message: error.to_string(),
        })?;
    let mut rows = statement
        .query(params![symbol, from_ts, to_ts])
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
        result.push(BookTickerRow {
            event_time_ms: row
                .get(0)
                .map_err(|error| StorageError::WriteFailedWithContext {
                    message: error.to_string(),
                })?,
            bid: row
                .get(1)
                .map_err(|error| StorageError::WriteFailedWithContext {
                    message: error.to_string(),
                })?,
            ask: row
                .get(2)
                .map_err(|error| StorageError::WriteFailedWithContext {
                    message: error.to_string(),
                })?,
        });
    }
    Ok(result)
}

pub fn load_derived_kline_rows_for_path(
    db_path: &Path,
    symbol: &str,
    from: chrono::NaiveDate,
    to: chrono::NaiveDate,
) -> Result<Vec<DerivedKlineRow>, StorageError> {
    if !db_path.exists() {
        return Ok(Vec::new());
    }
    let connection = open_dataset_connection_read_only(db_path)?;
    let from_ts = format!("{from} 00:00:00");
    let to_ts = format!("{to} 23:59:59");
    let mut statement = connection
        .prepare(
            "SELECT epoch_ms(open_time), epoch_ms(close_time), open, high, low, close, volume, quote_volume, trade_count
             FROM derived_kline_1s
             WHERE symbol = ? AND open_time >= CAST(? AS TIMESTAMP) AND open_time <= CAST(? AS TIMESTAMP)
             ORDER BY open_time ASC",
        )
        .map_err(|error| StorageError::WriteFailedWithContext {
            message: error.to_string(),
        })?;
    let mut rows = statement
        .query(params![symbol, from_ts, to_ts])
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
        result.push(DerivedKlineRow {
            open_time_ms: row.get(0).map_err(storage_err)?,
            close_time_ms: row.get(1).map_err(storage_err)?,
            open: row.get(2).map_err(storage_err)?,
            high: row.get(3).map_err(storage_err)?,
            low: row.get(4).map_err(storage_err)?,
            close: row.get(5).map_err(storage_err)?,
            volume: row.get(6).map_err(storage_err)?,
            quote_volume: row.get(7).map_err(storage_err)?,
            trade_count: positive_i64_to_u64(row.get::<_, i64>(8).map_err(storage_err)?),
        });
    }
    Ok(result)
}

pub fn load_raw_kline_rows_for_path(
    db_path: &Path,
    symbol: &str,
    from: chrono::NaiveDate,
    to: chrono::NaiveDate,
) -> Result<Option<(String, Vec<DerivedKlineRow>)>, StorageError> {
    if !db_path.exists() {
        return Ok(None);
    }
    let connection = open_dataset_connection_read_only(db_path)?;
    let from_ts = format!("{from} 00:00:00");
    let to_ts = format!("{to} 23:59:59");
    let interval = preferred_raw_kline_interval(&connection, symbol, &from_ts, &to_ts)?;
    let Some(interval) = interval else {
        return Ok(None);
    };
    let mut statement = connection
        .prepare(
            "SELECT epoch_ms(open_time), epoch_ms(close_time), open, high, low, close, volume, quote_volume, trade_count
             FROM raw_klines
             WHERE symbol = ? AND interval = ? AND open_time >= CAST(? AS TIMESTAMP) AND open_time <= CAST(? AS TIMESTAMP)
             ORDER BY open_time ASC",
        )
        .map_err(storage_err)?;
    let mut rows = statement
        .query(params![symbol, interval.as_str(), from_ts, to_ts])
        .map_err(storage_err)?;
    let mut result = Vec::new();
    while let Some(row) = rows.next().map_err(storage_err)? {
        result.push(DerivedKlineRow {
            open_time_ms: row.get(0).map_err(storage_err)?,
            close_time_ms: row.get(1).map_err(storage_err)?,
            open: row.get(2).map_err(storage_err)?,
            high: row.get(3).map_err(storage_err)?,
            low: row.get(4).map_err(storage_err)?,
            close: row.get(5).map_err(storage_err)?,
            volume: row.get(6).map_err(storage_err)?,
            quote_volume: row.get(7).map_err(storage_err)?,
            trade_count: positive_i64_to_u64(row.get::<_, i64>(8).map_err(storage_err)?),
        });
    }
    Ok(Some((interval, result)))
}

fn preferred_raw_kline_interval(
    connection: &Connection,
    symbol: &str,
    from_ts: &str,
    to_ts: &str,
) -> Result<Option<String>, StorageError> {
    let mut statement = connection
        .prepare(
            "SELECT DISTINCT interval
             FROM raw_klines
             WHERE symbol = ? AND open_time >= CAST(? AS TIMESTAMP) AND open_time <= CAST(? AS TIMESTAMP)",
        )
        .map_err(storage_err)?;
    let mut rows = statement
        .query(params![symbol, from_ts, to_ts])
        .map_err(storage_err)?;
    let mut intervals = Vec::new();
    while let Some(row) = rows.next().map_err(storage_err)? {
        intervals.push(row.get::<_, String>(0).map_err(storage_err)?);
    }
    Ok(intervals
        .into_iter()
        .min_by_key(|interval| raw_kline_interval_rank(interval)))
}

fn raw_kline_interval_rank(interval: &str) -> usize {
    match interval {
        "1m" => 0,
        "3m" => 1,
        "5m" => 2,
        "15m" => 3,
        "30m" => 4,
        "1h" => 5,
        "4h" => 6,
        "1d" => 7,
        "1w" => 8,
        "1mo" => 9,
        _ => usize::MAX,
    }
}

pub fn load_recorded_symbols_for_path(
    db_path: &Path,
    limit: usize,
) -> Result<Vec<String>, StorageError> {
    if !db_path.exists() {
        return Ok(Vec::new());
    }
    let connection = open_dataset_connection_read_only(db_path)?;
    let mut statement = connection
        .prepare(
            "SELECT symbol
             FROM (
                SELECT symbol FROM raw_liquidation_events
                UNION
                SELECT symbol FROM raw_book_ticker
                UNION
                SELECT symbol FROM raw_agg_trades
                UNION
                SELECT symbol FROM raw_klines
                UNION
                SELECT instrument AS symbol FROM backtest_runs
             )
             ORDER BY symbol ASC
             LIMIT ?",
        )
        .map_err(storage_err)?;
    let mut rows = statement
        .query(params![limit as i64])
        .map_err(storage_err)?;
    let mut result = Vec::new();
    while let Some(row) = rows.next().map_err(storage_err)? {
        result.push(row.get(0).map_err(storage_err)?);
    }
    Ok(result)
}

pub fn latest_market_data_day_for_path(
    db_path: &Path,
    symbol: &str,
) -> Result<Option<chrono::NaiveDate>, StorageError> {
    if !db_path.exists() {
        return Ok(None);
    }
    let connection = open_dataset_connection_read_only(db_path)?;
    let timestamps = [
        latest_symbol_timestamp(
            &connection,
            "raw_book_ticker",
            "event_time",
            "symbol",
            symbol,
        )?,
        latest_symbol_timestamp(
            &connection,
            "raw_agg_trades",
            "event_time",
            "symbol",
            symbol,
        )?,
        latest_symbol_timestamp(
            &connection,
            "raw_liquidation_events",
            "event_time",
            "symbol",
            symbol,
        )?,
        latest_symbol_timestamp(&connection, "raw_klines", "open_time", "symbol", symbol)?,
    ];
    Ok(timestamps
        .into_iter()
        .flatten()
        .max()
        .map(|value| value.date_naive()))
}

pub fn persist_backtest_report(
    db_path: &Path,
    report: &BacktestReport,
) -> Result<i64, StorageError> {
    let connection = open_dataset_connection_read_write(db_path)?;
    let run_id = next_backtest_run_id(&connection)?;
    let closed_trades = report
        .trades
        .iter()
        .filter(|trade| trade.net_pnl.is_some())
        .count() as i64;
    connection
        .execute(
            "INSERT INTO backtest_runs (
                run_id, created_at, mode, template, instrument, from_date, to_date, db_path,
                liquidation_events, book_ticker_events, agg_trade_events, derived_kline_1s_bars,
                trigger_count, closed_trades, open_trades, wins, losses, skipped_triggers,
                starting_equity, ending_equity, net_pnl, observed_win_rate, average_net_pnl,
                configured_expected_value, risk_pct, win_rate_assumption, r_multiple,
                max_entry_slippage_pct, stop_distance_pct
             ) VALUES (
                ?, CAST(? AS TIMESTAMP), ?, ?, ?, CAST(? AS DATE), CAST(? AS DATE), ?,
                ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?
             )",
            params![
                run_id,
                chrono::Utc::now().to_rfc3339(),
                report.mode.as_str(),
                report.template.slug(),
                report.instrument,
                report.from.to_string(),
                report.to.to_string(),
                report.db_path.display().to_string(),
                report.dataset.liquidation_events as i64,
                report.dataset.book_ticker_events as i64,
                report.dataset.agg_trade_events as i64,
                report.dataset.derived_kline_1s_bars as i64,
                report.trigger_count as i64,
                closed_trades,
                report.open_trades as i64,
                report.wins as i64,
                report.losses as i64,
                report.skipped_triggers as i64,
                report.starting_equity,
                report.ending_equity,
                report.net_pnl,
                report.observed_win_rate,
                report.average_net_pnl,
                report.configured_expected_value,
                report.config.risk_pct,
                report.config.win_rate_assumption,
                report.config.r_multiple,
                report.config.max_entry_slippage_pct,
                report.config.stop_distance_pct,
            ],
        )
        .map_err(storage_err)?;
    for trade in &report.trades {
        insert_backtest_trade(&connection, run_id, trade)?;
    }
    Ok(run_id)
}

pub fn load_backtest_run_summaries(
    db_path: &Path,
    limit: usize,
) -> Result<Vec<BacktestRunSummaryRow>, StorageError> {
    if !db_path.exists() {
        return Ok(Vec::new());
    }
    let connection = open_dataset_connection_read_only(db_path)?;
    let mut statement = connection
        .prepare(
            "SELECT run_id, CAST(created_at AS VARCHAR), mode, template, instrument,
                    CAST(from_date AS VARCHAR), CAST(to_date AS VARCHAR),
                    trigger_count, closed_trades, open_trades, wins, losses, net_pnl, ending_equity
             FROM backtest_runs
             ORDER BY run_id DESC
             LIMIT ?",
        )
        .map_err(storage_err)?;
    let mut rows = statement
        .query(params![limit as i64])
        .map_err(storage_err)?;
    let mut result = Vec::new();
    while let Some(row) = rows.next().map_err(storage_err)? {
        let mode_raw: String = row.get(2).map_err(storage_err)?;
        result.push(BacktestRunSummaryRow {
            run_id: row.get(0).map_err(storage_err)?,
            created_at: row.get(1).map_err(storage_err)?,
            mode: parse_mode(&mode_raw)?,
            template: row.get(3).map_err(storage_err)?,
            instrument: row.get(4).map_err(storage_err)?,
            from: row.get(5).map_err(storage_err)?,
            to: row.get(6).map_err(storage_err)?,
            trigger_count: positive_i64_to_u64(row.get::<_, i64>(7).map_err(storage_err)?),
            closed_trades: positive_i64_to_u64(row.get::<_, i64>(8).map_err(storage_err)?),
            open_trades: positive_i64_to_u64(row.get::<_, i64>(9).map_err(storage_err)?),
            wins: positive_i64_to_u64(row.get::<_, i64>(10).map_err(storage_err)?),
            losses: positive_i64_to_u64(row.get::<_, i64>(11).map_err(storage_err)?),
            net_pnl: row.get(12).map_err(storage_err)?,
            ending_equity: row.get(13).map_err(storage_err)?,
        });
    }
    Ok(result)
}

pub fn load_backtest_report(
    db_path: &Path,
    requested_run_id: Option<i64>,
) -> Result<Option<BacktestReport>, StorageError> {
    if !db_path.exists() {
        return Ok(None);
    }
    let connection = open_dataset_connection_read_only(db_path)?;
    let mut statement = match requested_run_id {
        Some(_) => connection.prepare(
            "SELECT run_id, mode, template, instrument, CAST(from_date AS VARCHAR), CAST(to_date AS VARCHAR),
                    db_path, liquidation_events, book_ticker_events, agg_trade_events, derived_kline_1s_bars,
                    trigger_count, open_trades, wins, losses, skipped_triggers, starting_equity,
                    ending_equity, net_pnl, observed_win_rate, average_net_pnl, configured_expected_value,
                    risk_pct, win_rate_assumption, r_multiple, max_entry_slippage_pct, stop_distance_pct
             FROM backtest_runs WHERE run_id = ?",
        ),
        None => connection.prepare(
            "SELECT run_id, mode, template, instrument, CAST(from_date AS VARCHAR), CAST(to_date AS VARCHAR),
                    db_path, liquidation_events, book_ticker_events, agg_trade_events, derived_kline_1s_bars,
                    trigger_count, open_trades, wins, losses, skipped_triggers, starting_equity,
                    ending_equity, net_pnl, observed_win_rate, average_net_pnl, configured_expected_value,
                    risk_pct, win_rate_assumption, r_multiple, max_entry_slippage_pct, stop_distance_pct
             FROM backtest_runs ORDER BY run_id DESC LIMIT 1",
        ),
    }
    .map_err(storage_err)?;
    let mut rows = match requested_run_id {
        Some(run_id) => statement.query(params![run_id]).map_err(storage_err)?,
        None => statement.query([]).map_err(storage_err)?,
    };
    let Some(row) = rows.next().map_err(storage_err)? else {
        return Ok(None);
    };
    let run_id: i64 = row.get(0).map_err(storage_err)?;
    let mode_raw: String = row.get(1).map_err(storage_err)?;
    let template_raw: String = row.get(2).map_err(storage_err)?;
    let from_raw: String = row.get(4).map_err(storage_err)?;
    let to_raw: String = row.get(5).map_err(storage_err)?;
    let trades = load_backtest_trades(&connection, run_id)?;
    Ok(Some(BacktestReport {
        run_id: Some(run_id),
        template: parse_template(&template_raw)?,
        instrument: row.get(3).map_err(storage_err)?,
        mode: parse_mode(&mode_raw)?,
        from: chrono::NaiveDate::parse_from_str(&from_raw, "%Y-%m-%d").map_err(|error| {
            StorageError::WriteFailedWithContext {
                message: error.to_string(),
            }
        })?,
        to: chrono::NaiveDate::parse_from_str(&to_raw, "%Y-%m-%d").map_err(|error| {
            StorageError::WriteFailedWithContext {
                message: error.to_string(),
            }
        })?,
        db_path: Path::new(&row.get::<_, String>(6).map_err(storage_err)?).to_path_buf(),
        dataset: BacktestDatasetSummary {
            mode: parse_mode(&mode_raw)?,
            symbol: row.get(3).map_err(storage_err)?,
            symbol_found: true,
            from: from_raw,
            to: to_raw,
            liquidation_events: positive_i64_to_u64(row.get::<_, i64>(7).map_err(storage_err)?),
            book_ticker_events: positive_i64_to_u64(row.get::<_, i64>(8).map_err(storage_err)?),
            agg_trade_events: positive_i64_to_u64(row.get::<_, i64>(9).map_err(storage_err)?),
            derived_kline_1s_bars: positive_i64_to_u64(row.get::<_, i64>(10).map_err(storage_err)?),
        },
        config: crate::backtest_app::runner::BacktestConfig {
            starting_equity: row.get(16).map_err(storage_err)?,
            risk_pct: row.get(22).map_err(storage_err)?,
            win_rate_assumption: row.get(23).map_err(storage_err)?,
            r_multiple: row.get(24).map_err(storage_err)?,
            max_entry_slippage_pct: row.get(25).map_err(storage_err)?,
            stop_distance_pct: row.get(26).map_err(storage_err)?,
            ..Default::default()
        },
        trigger_count: positive_i64_to_u64(row.get::<_, i64>(11).map_err(storage_err)?) as usize,
        trades,
        wins: positive_i64_to_u64(row.get::<_, i64>(13).map_err(storage_err)?) as usize,
        losses: positive_i64_to_u64(row.get::<_, i64>(14).map_err(storage_err)?) as usize,
        open_trades: positive_i64_to_u64(row.get::<_, i64>(12).map_err(storage_err)?) as usize,
        skipped_triggers: positive_i64_to_u64(row.get::<_, i64>(15).map_err(storage_err)?) as usize,
        starting_equity: row.get(16).map_err(storage_err)?,
        ending_equity: row.get(17).map_err(storage_err)?,
        net_pnl: row.get(18).map_err(storage_err)?,
        observed_win_rate: row.get(19).map_err(storage_err)?,
        average_net_pnl: row.get(20).map_err(storage_err)?,
        configured_expected_value: row.get(21).map_err(storage_err)?,
    }))
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

fn next_backtest_run_id(connection: &Connection) -> Result<i64, StorageError> {
    let mut statement = connection
        .prepare("SELECT COALESCE(MAX(run_id), 0) + 1 FROM backtest_runs")
        .map_err(storage_err)?;
    statement
        .query_row([], |row| row.get(0))
        .map_err(storage_err)
}

fn insert_backtest_trade(
    connection: &Connection,
    run_id: i64,
    trade: &BacktestTrade,
) -> Result<(), StorageError> {
    connection
        .execute(
            "INSERT INTO backtest_trades (
                run_id, trade_id, trigger_time, entry_time, entry_price, stop_price,
                take_profit_price, qty, exit_time, exit_price, exit_reason, gross_pnl, fees, net_pnl
             ) VALUES (
                ?, ?, CAST(? AS TIMESTAMP), CAST(? AS TIMESTAMP), ?, ?, ?, ?, CAST(? AS TIMESTAMP), ?, ?, ?, ?, ?
             )",
            params![
                run_id,
                trade.trade_id as i64,
                trade.trigger_time.to_rfc3339(),
                trade.entry_time.to_rfc3339(),
                trade.entry_price,
                trade.stop_price,
                trade.take_profit_price,
                trade.qty,
                trade.exit_time.map(|value| value.to_rfc3339()),
                trade.exit_price,
                trade.exit_reason.as_ref().map(|reason| reason.as_str()),
                trade.gross_pnl,
                trade.fees,
                trade.net_pnl,
            ],
        )
        .map_err(storage_err)?;
    Ok(())
}

fn load_backtest_trades(
    connection: &Connection,
    run_id: i64,
) -> Result<Vec<BacktestTrade>, StorageError> {
    let mut statement = connection
        .prepare(
            "SELECT trade_id, CAST(trigger_time AS VARCHAR), CAST(entry_time AS VARCHAR), entry_price, stop_price,
                    take_profit_price, qty, CAST(exit_time AS VARCHAR), exit_price, exit_reason, gross_pnl, fees, net_pnl
             FROM backtest_trades
             WHERE run_id = ?
             ORDER BY trade_id ASC",
        )
        .map_err(storage_err)?;
    let mut rows = statement.query(params![run_id]).map_err(storage_err)?;
    let mut result = Vec::new();
    while let Some(row) = rows.next().map_err(storage_err)? {
        let trigger_time_raw: String = row.get(1).map_err(storage_err)?;
        let entry_time_raw: String = row.get(2).map_err(storage_err)?;
        let exit_reason_raw: Option<String> = row.get(9).map_err(storage_err)?;
        result.push(BacktestTrade {
            trade_id: positive_i64_to_u64(row.get::<_, i64>(0).map_err(storage_err)?) as usize,
            trigger_time: parse_timestamp_string(&trigger_time_raw)?,
            entry_time: parse_timestamp_string(&entry_time_raw)?,
            entry_price: row.get(3).map_err(storage_err)?,
            stop_price: row.get(4).map_err(storage_err)?,
            take_profit_price: row.get(5).map_err(storage_err)?,
            qty: row.get(6).map_err(storage_err)?,
            exit_time: row
                .get::<_, Option<String>>(7)
                .map_err(storage_err)?
                .map(|value| parse_timestamp_string(&value))
                .transpose()?,
            exit_price: row.get(8).map_err(storage_err)?,
            exit_reason: exit_reason_raw
                .map(|value| parse_exit_reason(&value))
                .transpose()?,
            gross_pnl: row.get(10).map_err(storage_err)?,
            fees: row.get(11).map_err(storage_err)?,
            net_pnl: row.get(12).map_err(storage_err)?,
        });
    }
    Ok(result)
}

fn parse_mode(raw: &str) -> Result<BinanceMode, StorageError> {
    match raw {
        "demo" => Ok(BinanceMode::Demo),
        "real" => Ok(BinanceMode::Real),
        other => Err(StorageError::WriteFailedWithContext {
            message: format!("unsupported mode in backtest row: {other}"),
        }),
    }
}

fn parse_template(raw: &str) -> Result<StrategyTemplate, StorageError> {
    match raw {
        "liquidation-breakdown-short" => Ok(StrategyTemplate::LiquidationBreakdownShort),
        other => Err(StorageError::WriteFailedWithContext {
            message: format!("unsupported backtest template: {other}"),
        }),
    }
}

fn parse_exit_reason(raw: &str) -> Result<BacktestExitReason, StorageError> {
    match raw {
        "take_profit" => Ok(BacktestExitReason::TakeProfit),
        "stop_loss" => Ok(BacktestExitReason::StopLoss),
        "open_at_end" => Ok(BacktestExitReason::OpenAtEnd),
        other => Err(StorageError::WriteFailedWithContext {
            message: format!("unsupported backtest exit reason: {other}"),
        }),
    }
}

fn positive_i64_to_u64(value: i64) -> u64 {
    value.max(0) as u64
}

fn storage_err(error: duckdb::Error) -> StorageError {
    StorageError::WriteFailedWithContext {
        message: error.to_string(),
    }
}

fn parse_timestamp_string(value: &str) -> Result<chrono::DateTime<chrono::Utc>, StorageError> {
    if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(value) {
        return Ok(parsed.with_timezone(&chrono::Utc));
    }
    let naive =
        chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%.f").map_err(|error| {
            StorageError::WriteFailedWithContext {
                message: error.to_string(),
            }
        })?;
    Ok(chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(
        naive,
        chrono::Utc,
    ))
}

fn query_schema_version(connection: &Connection) -> Result<Option<String>, StorageError> {
    let mut statement = connection
        .prepare("SELECT value FROM schema_metadata WHERE key = 'market_data_schema_version'")
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

fn latest_symbol_timestamp(
    connection: &Connection,
    table: &str,
    time_column: &str,
    symbol_column: &str,
    symbol: &str,
) -> Result<Option<chrono::DateTime<chrono::Utc>>, StorageError> {
    let sql = format!(
        "SELECT CAST(MAX({time_column}) AS VARCHAR) FROM {table} WHERE {symbol_column} = ?"
    );
    let mut statement = connection.prepare(&sql).map_err(storage_err)?;
    let value: Option<String> = statement
        .query_row(params![symbol], |row| row.get(0))
        .map_err(storage_err)?;
    value.map(|raw| parse_timestamp_string(&raw)).transpose()
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

fn market_data_symbol_exists(connection: &Connection, symbol: &str) -> Result<bool, StorageError> {
    let mut statement = connection
        .prepare(
            "SELECT EXISTS(
                SELECT 1 FROM (
                    SELECT symbol FROM raw_liquidation_events WHERE symbol = ?
                    UNION
                    SELECT symbol FROM raw_book_ticker WHERE symbol = ?
                    UNION
                    SELECT symbol FROM raw_agg_trades WHERE symbol = ?
                    UNION
                    SELECT symbol FROM raw_klines WHERE symbol = ?
                    UNION
                    SELECT symbol FROM derived_kline_1s WHERE symbol = ?
                )
            )",
        )
        .map_err(storage_err)?;
    let exists = statement
        .query_row(params![symbol, symbol, symbol, symbol, symbol], |row| {
            row.get::<_, bool>(0)
        })
        .map_err(storage_err)?;
    Ok(exists)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dataset::schema::init_schema_for_path;

    #[test]
    fn read_only_backtest_queries_work_while_write_connection_is_held() {
        let db_path = std::env::temp_dir().join(format!(
            "sandbox-quant-query-{}.duckdb",
            uuid::Uuid::new_v4()
        ));
        init_schema_for_path(&db_path).expect("init schema");

        let write_connection =
            open_dataset_connection_read_write(&db_path).expect("write connection");

        let runs =
            load_backtest_run_summaries(&db_path, 20).expect("read summaries with writer held");
        let report =
            load_backtest_report(&db_path, None).expect("read latest report with writer held");

        assert!(runs.is_empty());
        assert!(report.is_none());

        drop(write_connection);
        let _ = std::fs::remove_file(&db_path);
    }
}
