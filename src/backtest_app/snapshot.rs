use chrono::NaiveDate;

use crate::app::bootstrap::BinanceMode;
use crate::error::storage_error::StorageError;
use crate::storage::postgres_market_data::{
    export_snapshot_to_duckdb, postgres_url_from_env, PostgresToDuckDbSnapshotConfig,
};

pub fn maybe_prepare_snapshot_from_postgres(
    mode: BinanceMode,
    base_dir: &str,
    instrument: &str,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<Option<String>, StorageError> {
    if !auto_snapshot_enabled() {
        return Ok(None);
    }
    let postgres_url = postgres_url_from_env()?;
    let interval_name = std::env::var("SANDBOX_QUANT_BACKTEST_SNAPSHOT_INTERVAL").ok();
    let product = std::env::var("SANDBOX_QUANT_BACKTEST_SNAPSHOT_PRODUCT").ok();
    let include_klines = std::env::var("SANDBOX_QUANT_BACKTEST_SNAPSHOT_SKIP_KLINES")
        .ok()
        .map(|value| !parse_boolish(&value))
        .unwrap_or(true);
    let include_liquidations = std::env::var("SANDBOX_QUANT_BACKTEST_SNAPSHOT_SKIP_LIQUIDATIONS")
        .ok()
        .map(|value| !parse_boolish(&value))
        .unwrap_or(false);
    let include_book_tickers = std::env::var("SANDBOX_QUANT_BACKTEST_SNAPSHOT_SKIP_BOOK_TICKERS")
        .ok()
        .map(|value| !parse_boolish(&value))
        .unwrap_or(false);
    let include_agg_trades = std::env::var("SANDBOX_QUANT_BACKTEST_SNAPSHOT_SKIP_AGG_TRADES")
        .ok()
        .map(|value| !parse_boolish(&value))
        .unwrap_or(false);
    let clear_duckdb_range = std::env::var("SANDBOX_QUANT_BACKTEST_SNAPSHOT_NO_CLEAR")
        .ok()
        .map(|value| !parse_boolish(&value))
        .unwrap_or(true);

    let report = export_snapshot_to_duckdb(&PostgresToDuckDbSnapshotConfig {
        postgres_url,
        mode,
        base_dir: base_dir.to_string(),
        symbols: vec![instrument.to_string()],
        from,
        to,
        product,
        interval_name,
        include_klines,
        include_liquidations,
        include_book_tickers,
        include_agg_trades,
        clear_duckdb_range,
    })?;

    Ok(Some(format!(
        "auto snapshot prepared: export_id={} kline_rows={} liquidation_rows={} book_ticker_rows={} agg_trade_rows={}",
        report.snapshot_export_id,
        report.kline_rows,
        report.liquidation_rows,
        report.book_ticker_rows,
        report.agg_trade_rows
    )))
}

fn auto_snapshot_enabled() -> bool {
    std::env::var("SANDBOX_QUANT_BACKTEST_AUTO_SNAPSHOT")
        .ok()
        .map(|value| {
            let normalized = value.trim().to_ascii_lowercase();
            normalized == "postgres" || parse_boolish(&normalized)
        })
        .unwrap_or(false)
}

fn parse_boolish(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_boolish_accepts_common_truthy_values() {
        assert!(parse_boolish("1"));
        assert!(parse_boolish("true"));
        assert!(parse_boolish("YES"));
        assert!(!parse_boolish("0"));
        assert!(!parse_boolish("false"));
    }
}
