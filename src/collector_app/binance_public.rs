use chrono::NaiveDate;
use duckdb::{params, Connection};
use reqwest::blocking::Client;
use std::io::{Cursor, Read};
use zip::read::ZipArchive;

use crate::app::bootstrap::BinanceMode;
use crate::dataset::schema::init_schema_for_path;
use crate::error::storage_error::StorageError;
use crate::record::coordination::RecorderCoordination;
use crate::storage::postgres_market_data::{
    connect as connect_postgres, init_schema as init_postgres_schema, insert_kline,
    insert_liquidation, mask_postgres_url, CollectorStorageBackend, PostgresKlineRecord,
    PostgresLiquidationRecord, SHARED_MARKET_DATA_MODE,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinancePublicProduct {
    Spot,
    Um,
    Cm,
}

impl BinancePublicProduct {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Spot => "spot",
            Self::Um => "um",
            Self::Cm => "cm",
        }
    }

    pub fn supports_liquidation(self) -> bool {
        matches!(self, Self::Um | Self::Cm)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BinancePublicImportConfig {
    pub product: BinancePublicProduct,
    pub symbol: String,
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub kline_interval: String,
    pub import_liquidation: bool,
    pub import_klines: bool,
    pub mode: BinanceMode,
    pub base_dir: String,
    pub storage_backend: CollectorStorageBackend,
    pub postgres_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinancePublicImportReport {
    pub db_path: String,
    pub dates_requested: usize,
    pub dates_with_imports: usize,
    pub skipped_liquidation_dates: usize,
    pub skipped_kline_dates: usize,
    pub liquidation_rows: usize,
    pub kline_rows: usize,
}

pub fn import_binance_public_data(
    config: &BinancePublicImportConfig,
) -> Result<BinancePublicImportReport, StorageError> {
    let target = match config.storage_backend {
        CollectorStorageBackend::DuckDb => RecorderCoordination::new(config.base_dir.clone())
            .db_path(config.mode)
            .display()
            .to_string(),
        CollectorStorageBackend::Postgres => {
            mask_postgres_url(config.postgres_url.as_deref().ok_or_else(|| {
                StorageError::WriteFailedWithContext {
                    message: "postgres storage backend requires postgres_url".to_string(),
                }
            })?)
        }
    };

    let client =
        Client::builder()
            .build()
            .map_err(|error| StorageError::WriteFailedWithContext {
                message: error.to_string(),
            })?;

    let mut liquidation_rows = 0usize;
    let mut kline_rows = 0usize;
    let mut dates_with_imports = 0usize;
    let mut skipped_liquidation_dates = 0usize;
    let mut skipped_kline_dates = 0usize;
    let dates = inclusive_dates(config.from, config.to);

    match config.storage_backend {
        CollectorStorageBackend::DuckDb => {
            let db_path = RecorderCoordination::new(config.base_dir.clone()).db_path(config.mode);
            init_schema_for_path(&db_path)?;
            let connection =
                Connection::open(&db_path).map_err(|error| StorageError::DatabaseInitFailed {
                    path: db_path.display().to_string(),
                    message: error.to_string(),
                })?;

            for date in &dates {
                let mut imported_any = false;

                if config.import_liquidation {
                    if let Some(url) =
                        liquidation_snapshot_url(config.product, &config.symbol, *date)
                    {
                        if let Some(bytes) = download_zip_if_exists(&client, &url)? {
                            liquidation_rows += import_liquidation_snapshot_bytes_duckdb(
                                &connection,
                                config.mode,
                                config.product,
                                &config.symbol,
                                bytes,
                            )?;
                            imported_any = true;
                        } else {
                            skipped_liquidation_dates += 1;
                        }
                    } else {
                        skipped_liquidation_dates += 1;
                    }
                }

                if config.import_klines {
                    let url = kline_zip_url(
                        config.product,
                        &config.symbol,
                        &config.kline_interval,
                        *date,
                    );
                    if let Some(bytes) = download_zip_if_exists(&client, &url)? {
                        kline_rows += import_kline_bytes_duckdb(
                            &connection,
                            config.mode,
                            config.product,
                            &config.symbol,
                            &config.kline_interval,
                            bytes,
                        )?;
                        imported_any = true;
                    } else {
                        skipped_kline_dates += 1;
                    }
                }

                if imported_any {
                    dates_with_imports += 1;
                }
            }
        }
        CollectorStorageBackend::Postgres => {
            let postgres_url = config.postgres_url.as_deref().ok_or_else(|| {
                StorageError::WriteFailedWithContext {
                    message: "postgres storage backend requires postgres_url".to_string(),
                }
            })?;
            let mut postgres = connect_postgres(postgres_url)?;
            let _ = init_postgres_schema(&mut postgres, postgres_url)?;

            for date in &dates {
                let mut imported_any = false;

                if config.import_liquidation {
                    if let Some(url) =
                        liquidation_snapshot_url(config.product, &config.symbol, *date)
                    {
                        if let Some(bytes) = download_zip_if_exists(&client, &url)? {
                            liquidation_rows += import_liquidation_snapshot_bytes_postgres(
                                &mut postgres,
                                config.mode,
                                config.product,
                                &config.symbol,
                                bytes,
                            )?;
                            imported_any = true;
                        } else {
                            skipped_liquidation_dates += 1;
                        }
                    } else {
                        skipped_liquidation_dates += 1;
                    }
                }

                if config.import_klines {
                    let url = kline_zip_url(
                        config.product,
                        &config.symbol,
                        &config.kline_interval,
                        *date,
                    );
                    if let Some(bytes) = download_zip_if_exists(&client, &url)? {
                        kline_rows += import_kline_bytes_postgres(
                            &mut postgres,
                            config.mode,
                            config.product,
                            &config.symbol,
                            &config.kline_interval,
                            bytes,
                        )?;
                        imported_any = true;
                    } else {
                        skipped_kline_dates += 1;
                    }
                }

                if imported_any {
                    dates_with_imports += 1;
                }
            }
        }
    }

    Ok(BinancePublicImportReport {
        db_path: target,
        dates_requested: dates.len(),
        dates_with_imports,
        skipped_liquidation_dates,
        skipped_kline_dates,
        liquidation_rows,
        kline_rows,
    })
}

fn download_zip_if_exists(client: &Client, url: &str) -> Result<Option<Vec<u8>>, StorageError> {
    let response =
        client
            .get(url)
            .send()
            .map_err(|error| StorageError::WriteFailedWithContext {
                message: error.to_string(),
            })?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let response =
        response
            .error_for_status()
            .map_err(|error| StorageError::WriteFailedWithContext {
                message: error.to_string(),
            })?;
    response
        .bytes()
        .map(|bytes| Some(bytes.to_vec()))
        .map_err(|error| StorageError::WriteFailedWithContext {
            message: error.to_string(),
        })
}

fn import_liquidation_snapshot_bytes_duckdb(
    connection: &Connection,
    mode: BinanceMode,
    product: BinancePublicProduct,
    symbol: &str,
    bytes: Vec<u8>,
) -> Result<usize, StorageError> {
    let csv = first_zip_csv(bytes)?;
    let mut rows = 0usize;
    let mut next_id = next_liquidation_event_id(connection)?;
    for line in csv.lines() {
        if line.trim().is_empty() || line.starts_with("time,") {
            continue;
        }
        let record = parse_liquidation_line(mode, product, symbol, line)?;
        connection
            .execute(
                "INSERT INTO raw_liquidation_events (
                    event_id, mode, symbol, event_time, receive_time, force_side, price, qty, notional, raw_payload
                 ) VALUES (
                    ?, ?, ?, to_timestamp(? / 1000.0), to_timestamp(? / 1000.0), ?, ?, ?, ?, ?
                 )",
                params![
                    next_id,
                    mode.as_str(),
                    record.symbol,
                    record.event_time_ms,
                    record.receive_time_ms,
                    record.force_side,
                    record.price,
                    record.qty,
                    record.notional,
                    record.raw_payload,
                ],
            )
            .map_err(storage_err)?;
        next_id += 1;
        rows += 1;
    }
    Ok(rows)
}

fn import_liquidation_snapshot_bytes_postgres(
    client: &mut postgres::Client,
    mode: BinanceMode,
    product: BinancePublicProduct,
    symbol: &str,
    bytes: Vec<u8>,
) -> Result<usize, StorageError> {
    let csv = first_zip_csv(bytes)?;
    let mut rows = 0usize;
    for line in csv.lines() {
        if line.trim().is_empty() || line.starts_with("time,") {
            continue;
        }
        let record = parse_liquidation_line(mode, product, symbol, line)?;
        insert_liquidation(client, &record)?;
        rows += 1;
    }
    Ok(rows)
}

fn parse_liquidation_line(
    _mode: BinanceMode,
    product: BinancePublicProduct,
    symbol: &str,
    line: &str,
) -> Result<PostgresLiquidationRecord, StorageError> {
    let cols = split_csv_line(line);
    if cols.len() < 10 {
        return Err(StorageError::WriteFailedWithContext {
            message: format!("liquidation line too short: {line}"),
        });
    }
    let side = cols[1].trim().to_string();
    let time_ms = normalize_epoch_millis(parse_i64(&cols[0])?);
    let limit_price = parse_f64(&cols[5])?;
    let average_price = parse_f64(&cols[6]).unwrap_or(limit_price);
    let qty = parse_f64(&cols[9]).or_else(|_| parse_f64(&cols[8]))?;
    let price = if average_price > 0.0 {
        average_price
    } else {
        limit_price
    };
    Ok(PostgresLiquidationRecord {
        product: product.as_str().to_string(),
        symbol: symbol.to_string(),
        event_time_ms: time_ms,
        receive_time_ms: time_ms,
        force_side: side,
        price,
        qty,
        notional: price * qty,
        raw_payload: format!(
            "{{\"source\":\"binance-public\",\"product\":\"{}\",\"symbol\":\"{}\",\"line\":\"{}\"}}",
            product.as_str(),
            symbol,
            line.replace('\"', "\\\"")
        ),
    })
}

fn import_kline_bytes_duckdb(
    connection: &Connection,
    mode: BinanceMode,
    product: BinancePublicProduct,
    symbol: &str,
    interval: &str,
    bytes: Vec<u8>,
) -> Result<usize, StorageError> {
    let csv = first_zip_csv(bytes)?;
    let mut rows = 0usize;
    let mut next_id = next_kline_id(connection)?;
    let shared_mode = SHARED_MARKET_DATA_MODE.to_string();
    for line in csv.lines() {
        if line.trim().is_empty() || line.starts_with("open_time,") {
            continue;
        }
        let record = parse_kline_line(mode, product, symbol, interval, line)?;
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
                    next_id,
                    shared_mode.clone(),
                    record.product,
                    record.symbol,
                    record.interval_name,
                    record.open_time_ms,
                    record.close_time_ms,
                    record.open,
                    record.high,
                    record.low,
                    record.close,
                    record.volume,
                    record.quote_volume,
                    record.trade_count,
                    record.taker_buy_base_volume,
                    record.taker_buy_quote_volume,
                    record.raw_payload,
                ],
            )
            .map_err(storage_err)?;
        next_id += 1;
        rows += 1;
    }
    Ok(rows)
}

fn import_kline_bytes_postgres(
    client: &mut postgres::Client,
    mode: BinanceMode,
    product: BinancePublicProduct,
    symbol: &str,
    interval: &str,
    bytes: Vec<u8>,
) -> Result<usize, StorageError> {
    let csv = first_zip_csv(bytes)?;
    let mut rows = 0usize;
    for line in csv.lines() {
        if line.trim().is_empty() || line.starts_with("open_time,") {
            continue;
        }
        let record = parse_kline_line(mode, product, symbol, interval, line)?;
        insert_kline(client, &record)?;
        rows += 1;
    }
    Ok(rows)
}

fn parse_kline_line(
    _mode: BinanceMode,
    product: BinancePublicProduct,
    symbol: &str,
    interval: &str,
    line: &str,
) -> Result<PostgresKlineRecord, StorageError> {
    let cols = split_csv_line(line);
    if cols.len() < 11 {
        return Err(StorageError::WriteFailedWithContext {
            message: format!("kline line too short: {line}"),
        });
    }
    Ok(PostgresKlineRecord {
        product: product.as_str().to_string(),
        symbol: symbol.to_string(),
        interval_name: interval.to_string(),
        open_time_ms: normalize_epoch_millis(parse_i64(&cols[0])?),
        open: parse_f64(&cols[1])?,
        high: parse_f64(&cols[2])?,
        low: parse_f64(&cols[3])?,
        close: parse_f64(&cols[4])?,
        volume: parse_f64(&cols[5])?,
        close_time_ms: normalize_epoch_millis(parse_i64(&cols[6])?),
        quote_volume: parse_f64(&cols[7])?,
        trade_count: parse_i64(&cols[8])?,
        taker_buy_base_volume: parse_f64(&cols[9]).ok(),
        taker_buy_quote_volume: parse_f64(&cols[10]).ok(),
        raw_payload: format!(
            "{{\"source\":\"binance-public\",\"product\":\"{}\",\"symbol\":\"{}\",\"interval\":\"{}\",\"line\":\"{}\"}}",
            product.as_str(),
            symbol,
            interval,
            line.replace('\"', "\\\"")
        ),
    })
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
    product: BinancePublicProduct,
    symbol: &str,
    date: NaiveDate,
) -> Option<String> {
    if !product.supports_liquidation() {
        return None;
    }
    Some(format!(
        "https://data.binance.vision/data/futures/{}/daily/liquidationSnapshot/{}/{}-liquidationSnapshot-{}.zip",
        product.as_str(),
        symbol,
        symbol,
        date.format("%Y-%m-%d")
    ))
}

fn kline_zip_url(
    product: BinancePublicProduct,
    symbol: &str,
    interval: &str,
    date: NaiveDate,
) -> String {
    match product {
        BinancePublicProduct::Spot => format!(
            "https://data.binance.vision/data/spot/daily/klines/{}/{}/{}-{}-{}.zip",
            symbol,
            interval,
            symbol,
            interval,
            date.format("%Y-%m-%d")
        ),
        BinancePublicProduct::Um | BinancePublicProduct::Cm => format!(
            "https://data.binance.vision/data/futures/{}/daily/klines/{}/{}/{}-{}-{}.zip",
            product.as_str(),
            symbol,
            interval,
            symbol,
            interval,
            date.format("%Y-%m-%d")
        ),
    }
}

fn inclusive_dates(from: NaiveDate, to: NaiveDate) -> Vec<NaiveDate> {
    let mut dates = Vec::new();
    let mut current = from;
    while current <= to {
        dates.push(current);
        current = current
            .succ_opt()
            .expect("date overflow while building import range");
    }
    dates
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

fn normalize_epoch_millis(value: i64) -> i64 {
    if value >= 1_000_000_000_000_000 {
        value / 1_000
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binance_public_urls_match_expected_layout() {
        let date = NaiveDate::from_ymd_opt(2023, 9, 29).unwrap();
        assert_eq!(
            liquidation_snapshot_url(BinancePublicProduct::Cm, "ADAUSD_230929", date),
            Some(
                "https://data.binance.vision/data/futures/cm/daily/liquidationSnapshot/ADAUSD_230929/ADAUSD_230929-liquidationSnapshot-2023-09-29.zip"
                    .to_string()
            )
        );
        assert_eq!(
            kline_zip_url(BinancePublicProduct::Um, "BTCUSDT", "1m", date),
            "https://data.binance.vision/data/futures/um/daily/klines/BTCUSDT/1m/BTCUSDT-1m-2023-09-29.zip"
        );
        assert_eq!(
            kline_zip_url(BinancePublicProduct::Spot, "BTCUSDT", "1d", date),
            "https://data.binance.vision/data/spot/daily/klines/BTCUSDT/1d/BTCUSDT-1d-2023-09-29.zip"
        );
    }

    #[test]
    fn inclusive_dates_builds_closed_range() {
        let from = NaiveDate::from_ymd_opt(2023, 7, 6).unwrap();
        let to = NaiveDate::from_ymd_opt(2023, 7, 8).unwrap();
        assert_eq!(
            inclusive_dates(from, to),
            vec![
                NaiveDate::from_ymd_opt(2023, 7, 6).unwrap(),
                NaiveDate::from_ymd_opt(2023, 7, 7).unwrap(),
                NaiveDate::from_ymd_opt(2023, 7, 8).unwrap(),
            ]
        );
    }

    #[test]
    fn normalize_epoch_millis_handles_microseconds() {
        assert_eq!(
            normalize_epoch_millis(1_735_689_600_000_000),
            1_735_689_600_000
        );
        assert_eq!(normalize_epoch_millis(1_735_689_600_000), 1_735_689_600_000);
    }
}
