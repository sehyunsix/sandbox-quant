use chrono::NaiveDate;
use duckdb::{params, Connection};
use reqwest::blocking::Client;
use std::io::{Cursor, Read};
use zip::read::ZipArchive;

use crate::app::bootstrap::BinanceMode;
use crate::dataset::schema::init_schema_for_path;
use crate::error::storage_error::StorageError;
use crate::record::coordination::RecorderCoordination;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinanceFuturesProduct {
    Um,
    Cm,
}

impl BinanceFuturesProduct {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Um => "um",
            Self::Cm => "cm",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BinancePublicImportConfig {
    pub product: BinanceFuturesProduct,
    pub symbol: String,
    pub date: NaiveDate,
    pub kline_interval: String,
    pub import_liquidation: bool,
    pub import_klines: bool,
    pub mode: BinanceMode,
    pub base_dir: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinancePublicImportReport {
    pub db_path: String,
    pub liquidation_rows: usize,
    pub kline_rows: usize,
}

pub fn import_binance_public_data(
    config: &BinancePublicImportConfig,
) -> Result<BinancePublicImportReport, StorageError> {
    let db_path = RecorderCoordination::new(config.base_dir.clone()).db_path(config.mode);
    init_schema_for_path(&db_path)?;
    let connection =
        Connection::open(&db_path).map_err(|error| StorageError::DatabaseInitFailed {
            path: db_path.display().to_string(),
            message: error.to_string(),
        })?;
    let client =
        Client::builder()
            .build()
            .map_err(|error| StorageError::WriteFailedWithContext {
                message: error.to_string(),
            })?;

    let mut liquidation_rows = 0usize;
    let mut kline_rows = 0usize;

    if config.import_liquidation {
        let url = liquidation_snapshot_url(config.product, &config.symbol, config.date);
        let bytes = download_zip(&client, &url)?;
        liquidation_rows = import_liquidation_snapshot_bytes(
            &connection,
            config.mode,
            config.product,
            &config.symbol,
            bytes,
        )?;
    }

    if config.import_klines {
        let url = kline_zip_url(
            config.product,
            &config.symbol,
            &config.kline_interval,
            config.date,
        );
        let bytes = download_zip(&client, &url)?;
        kline_rows = import_kline_bytes(
            &connection,
            config.mode,
            config.product,
            &config.symbol,
            &config.kline_interval,
            bytes,
        )?;
    }

    Ok(BinancePublicImportReport {
        db_path: db_path.display().to_string(),
        liquidation_rows,
        kline_rows,
    })
}

fn download_zip(client: &Client, url: &str) -> Result<Vec<u8>, StorageError> {
    let response = client
        .get(url)
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|error| StorageError::WriteFailedWithContext {
            message: error.to_string(),
        })?;
    response
        .bytes()
        .map(|bytes| bytes.to_vec())
        .map_err(|error| StorageError::WriteFailedWithContext {
            message: error.to_string(),
        })
}

fn import_liquidation_snapshot_bytes(
    connection: &Connection,
    mode: BinanceMode,
    product: BinanceFuturesProduct,
    symbol: &str,
    bytes: Vec<u8>,
) -> Result<usize, StorageError> {
    let csv = first_zip_csv(bytes)?;
    let mut rows = 0usize;
    for (index, line) in csv.lines().enumerate() {
        if line.trim().is_empty() || line.starts_with("time,") {
            continue;
        }
        let cols = split_csv_line(line);
        if cols.len() < 10 {
            continue;
        }
        let side = cols[1].trim().to_string();
        let time_ms = parse_i64(&cols[0])?;
        let limit_price = parse_f64(&cols[5])?;
        let average_price = parse_f64(&cols[6]).unwrap_or(limit_price);
        let qty = parse_f64(&cols[9]).or_else(|_| parse_f64(&cols[8]))?;
        let price = if average_price > 0.0 {
            average_price
        } else {
            limit_price
        };
        let notional = price * qty;
        connection
            .execute(
                "INSERT INTO raw_liquidation_events (
                    event_id, mode, symbol, event_time, receive_time, force_side, price, qty, notional, raw_payload
                 ) VALUES (
                    ?, ?, ?, to_timestamp(? / 1000.0), to_timestamp(? / 1000.0), ?, ?, ?, ?, ?
                 )",
                params![
                    next_liquidation_event_id(connection)? + index as i64,
                    mode.as_str(),
                    symbol,
                    time_ms,
                    time_ms,
                    side,
                    price,
                    qty,
                    notional,
                    format!(
                        "{{\"source\":\"binance-public\",\"product\":\"{}\",\"symbol\":\"{}\",\"line\":\"{}\"}}",
                        product.as_str(),
                        symbol,
                        line.replace('\"', "\\\"")
                    ),
                ],
            )
            .map_err(storage_err)?;
        rows += 1;
    }
    Ok(rows)
}

fn import_kline_bytes(
    connection: &Connection,
    mode: BinanceMode,
    product: BinanceFuturesProduct,
    symbol: &str,
    interval: &str,
    bytes: Vec<u8>,
) -> Result<usize, StorageError> {
    let csv = first_zip_csv(bytes)?;
    let mut rows = 0usize;
    for (index, line) in csv.lines().enumerate() {
        if line.trim().is_empty() || line.starts_with("open_time,") {
            continue;
        }
        let cols = split_csv_line(line);
        if cols.len() < 11 {
            continue;
        }
        let open_time_ms = parse_i64(&cols[0])?;
        let open = parse_f64(&cols[1])?;
        let high = parse_f64(&cols[2])?;
        let low = parse_f64(&cols[3])?;
        let close = parse_f64(&cols[4])?;
        let volume = parse_f64(&cols[5])?;
        let close_time_ms = parse_i64(&cols[6])?;
        let quote_volume = parse_f64(&cols[7])?;
        let trade_count = parse_i64(&cols[8])?;
        let taker_buy_base_volume = parse_f64(&cols[9]).ok();
        let taker_buy_quote_volume = parse_f64(&cols[10]).ok();
        connection
            .execute(
                "INSERT INTO raw_klines (
                    kline_id, mode, product, symbol, interval, open_time, close_time,
                    open, high, low, close, volume, quote_volume, trade_count,
                    taker_buy_base_volume, taker_buy_quote_volume, raw_payload
                 ) VALUES (
                    ?, ?, ?, ?, ?, to_timestamp(? / 1000.0), to_timestamp(? / 1000.0),
                    ?, ?, ?, ?, ?, ?, ?, ?, ?, ?
                 )",
                params![
                    next_kline_id(connection)? + index as i64,
                    mode.as_str(),
                    product.as_str(),
                    symbol,
                    interval,
                    open_time_ms,
                    close_time_ms,
                    open,
                    high,
                    low,
                    close,
                    volume,
                    quote_volume,
                    trade_count,
                    taker_buy_base_volume,
                    taker_buy_quote_volume,
                    format!(
                        "{{\"source\":\"binance-public\",\"product\":\"{}\",\"symbol\":\"{}\",\"interval\":\"{}\",\"line\":\"{}\"}}",
                        product.as_str(),
                        symbol,
                        interval,
                        line.replace('\"', "\\\"")
                    ),
                ],
            )
            .map_err(storage_err)?;
        rows += 1;
    }
    Ok(rows)
}

fn first_zip_csv(bytes: Vec<u8>) -> Result<String, StorageError> {
    let reader = Cursor::new(bytes);
    let mut archive =
        ZipArchive::new(reader).map_err(|error| StorageError::WriteFailedWithContext {
            message: error.to_string(),
        })?;
    let mut file = archive
        .by_index(0)
        .map_err(|error| StorageError::WriteFailedWithContext {
            message: error.to_string(),
        })?;
    let mut out = String::new();
    file.read_to_string(&mut out)
        .map_err(|error| StorageError::WriteFailedWithContext {
            message: error.to_string(),
        })?;
    Ok(out)
}

fn next_liquidation_event_id(connection: &Connection) -> Result<i64, StorageError> {
    let mut statement = connection
        .prepare("SELECT COALESCE(MAX(event_id), 0) + 1 FROM raw_liquidation_events")
        .map_err(storage_err)?;
    statement
        .query_row([], |row| row.get(0))
        .map_err(storage_err)
}

fn next_kline_id(connection: &Connection) -> Result<i64, StorageError> {
    let mut statement = connection
        .prepare("SELECT COALESCE(MAX(kline_id), 0) + 1 FROM raw_klines")
        .map_err(storage_err)?;
    statement
        .query_row([], |row| row.get(0))
        .map_err(storage_err)
}

fn liquidation_snapshot_url(
    product: BinanceFuturesProduct,
    symbol: &str,
    date: NaiveDate,
) -> String {
    format!(
        "https://data.binance.vision/data/futures/{}/daily/liquidationSnapshot/{}/{}-liquidationSnapshot-{}.zip",
        product.as_str(),
        symbol,
        symbol,
        date.format("%Y-%m-%d")
    )
}

fn kline_zip_url(
    product: BinanceFuturesProduct,
    symbol: &str,
    interval: &str,
    date: NaiveDate,
) -> String {
    format!(
        "https://data.binance.vision/data/futures/{}/daily/klines/{}/{}/{}-{}-{}.zip",
        product.as_str(),
        symbol,
        interval,
        symbol,
        interval,
        date.format("%Y-%m-%d")
    )
}

fn split_csv_line(line: &str) -> Vec<String> {
    line.split(',')
        .map(|value| value.trim().to_string())
        .collect()
}

fn parse_i64(value: &str) -> Result<i64, StorageError> {
    value
        .trim()
        .parse::<i64>()
        .map_err(|error| StorageError::WriteFailedWithContext {
            message: error.to_string(),
        })
}

fn parse_f64(value: &str) -> Result<f64, StorageError> {
    value
        .trim()
        .parse::<f64>()
        .map_err(|error| StorageError::WriteFailedWithContext {
            message: error.to_string(),
        })
}

fn storage_err(error: duckdb::Error) -> StorageError {
    StorageError::WriteFailedWithContext {
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binance_public_urls_match_expected_layout() {
        let date = NaiveDate::from_ymd_opt(2023, 9, 29).unwrap();
        assert_eq!(
            liquidation_snapshot_url(BinanceFuturesProduct::Cm, "ADAUSD_230929", date),
            "https://data.binance.vision/data/futures/cm/daily/liquidationSnapshot/ADAUSD_230929/ADAUSD_230929-liquidationSnapshot-2023-09-29.zip"
        );
        assert_eq!(
            kline_zip_url(BinanceFuturesProduct::Um, "BTCUSDT", "1m", date),
            "https://data.binance.vision/data/futures/um/daily/klines/BTCUSDT/1m/BTCUSDT-1m-2023-09-29.zip"
        );
    }
}
