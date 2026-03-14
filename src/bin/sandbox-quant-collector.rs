use chrono::NaiveDate;
use duckdb::Connection;
use sandbox_quant::app::bootstrap::BinanceMode;
use sandbox_quant::collector_app::binance_public::{
    import_binance_public_data, BinancePublicImportConfig, BinancePublicImportReport,
    BinancePublicProduct,
};
use sandbox_quant::dataset::schema::{init_schema_for_path, MARKET_DATA_SCHEMA_VERSION};
use sandbox_quant::error::storage_error::StorageError;
use sandbox_quant::record::coordination::RecorderCoordination;

#[derive(Debug, Clone, PartialEq)]
struct BatchImportConfig {
    products: Vec<BinancePublicProduct>,
    symbols: Vec<String>,
    from: NaiveDate,
    to: NaiveDate,
    kline_interval: String,
    import_liquidation: bool,
    import_klines: bool,
    mode: BinanceMode,
    base_dir: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("binance-public") if args.get(1).map(String::as_str) == Some("import") => {
            let config = parse_import_args(&args[2..])?;
            let report = import_many(&config)?;
            println!(
                "{}",
                [
                    "collector import".to_string(),
                    format!("products={}", join_products(&config.products)),
                    format!("symbols={}", config.symbols.join(",")),
                    format!("from={}", config.from),
                    format!("to={}", config.to),
                    format!("mode={}", config.mode.as_str()),
                    format!("db_path={}", report.db_path),
                    format!("dates_requested={}", report.dates_requested),
                    format!("dates_with_imports={}", report.dates_with_imports),
                    format!(
                        "skipped_liquidation_dates={}",
                        report.skipped_liquidation_dates
                    ),
                    format!("skipped_kline_dates={}", report.skipped_kline_dates),
                    format!("liquidation_rows={}", report.liquidation_rows),
                    format!("kline_rows={}", report.kline_rows),
                ]
                .join("\n")
            );
        }
        Some("summary") => {
            let (mode, base_dir) = parse_summary_args(&args[1..])?;
            println!("{}", render_summary(mode, &base_dir)?);
        }
        _ => {
            eprintln!(
                "usage: sandbox-quant-collector binance-public import (--product <spot|um|cm> | --products <spot,um,cm>) (--symbol <symbol> | --symbols <a,b,c>) (--date <YYYY-MM-DD> | --from <YYYY-MM-DD> --to <YYYY-MM-DD>) [--kline-interval <interval>] [--mode <demo|real>] [--base-dir <path>] [--skip-liquidation] [--skip-klines]\n       sandbox-quant-collector summary [--mode <demo|real>] [--base-dir <path>]"
            );
            std::process::exit(2);
        }
    }
    Ok(())
}

fn parse_import_args(args: &[String]) -> Result<BatchImportConfig, Box<dyn std::error::Error>> {
    let mut products = None;
    let mut symbols = None;
    let mut date = None;
    let mut from = None;
    let mut to = None;
    let mut kline_interval = "1m".to_string();
    let mut mode = BinanceMode::Demo;
    let mut base_dir = "var".to_string();
    let mut import_liquidation = true;
    let mut import_klines = true;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--product" => {
                let value = args.get(index + 1).ok_or("missing value for --product")?;
                products = Some(parse_products(value)?);
                index += 2;
            }
            "--products" => {
                let value = args.get(index + 1).ok_or("missing value for --products")?;
                products = Some(parse_products(value)?);
                index += 2;
            }
            "--symbol" => {
                let value = args.get(index + 1).ok_or("missing value for --symbol")?;
                symbols = Some(parse_symbols(value));
                index += 2;
            }
            "--symbols" => {
                let value = args.get(index + 1).ok_or("missing value for --symbols")?;
                symbols = Some(parse_symbols(value));
                index += 2;
            }
            "--date" => {
                let value = args.get(index + 1).ok_or("missing value for --date")?;
                date = Some(NaiveDate::parse_from_str(value, "%Y-%m-%d")?);
                index += 2;
            }
            "--from" => {
                let value = args.get(index + 1).ok_or("missing value for --from")?;
                from = Some(NaiveDate::parse_from_str(value, "%Y-%m-%d")?);
                index += 2;
            }
            "--to" => {
                let value = args.get(index + 1).ok_or("missing value for --to")?;
                to = Some(NaiveDate::parse_from_str(value, "%Y-%m-%d")?);
                index += 2;
            }
            "--kline-interval" => {
                kline_interval = args
                    .get(index + 1)
                    .ok_or("missing value for --kline-interval")?
                    .clone();
                index += 2;
            }
            "--mode" => {
                let value = args.get(index + 1).ok_or("missing value for --mode")?;
                mode = match value.as_str() {
                    "demo" => BinanceMode::Demo,
                    "real" => BinanceMode::Real,
                    _ => return Err(format!("unsupported mode: {value}").into()),
                };
                index += 2;
            }
            "--base-dir" => {
                base_dir = args
                    .get(index + 1)
                    .ok_or("missing value for --base-dir")?
                    .clone();
                index += 2;
            }
            "--skip-liquidation" => {
                import_liquidation = false;
                index += 1;
            }
            "--skip-klines" => {
                import_klines = false;
                index += 1;
            }
            other => return Err(format!("unsupported arg: {other}").into()),
        }
    }
    let (from, to) = match (date, from, to) {
        (Some(date), None, None) => (date, date),
        (None, Some(from), Some(to)) => (from, to),
        (Some(_), Some(_), _) | (Some(_), _, Some(_)) => {
            return Err("use either --date or --from/--to, not both".into())
        }
        _ => return Err("missing --date or --from/--to".into()),
    };
    if from > to {
        return Err("--from must be <= --to".into());
    }
    let products = products.ok_or("missing --product or --products")?;
    let symbols = symbols.ok_or("missing --symbol or --symbols")?;
    Ok(BatchImportConfig {
        products,
        symbols,
        from,
        to,
        kline_interval,
        import_liquidation,
        import_klines,
        mode,
        base_dir,
    })
}

fn parse_products(raw: &str) -> Result<Vec<BinancePublicProduct>, Box<dyn std::error::Error>> {
    let mut products = Vec::new();
    for value in raw
        .split(',')
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        let product = match value {
            "spot" => BinancePublicProduct::Spot,
            "um" => BinancePublicProduct::Um,
            "cm" => BinancePublicProduct::Cm,
            other => return Err(format!("unsupported product: {other}").into()),
        };
        if !products.contains(&product) {
            products.push(product);
        }
    }
    if products.is_empty() {
        return Err("no products provided".into());
    }
    Ok(products)
}

fn parse_symbols(raw: &str) -> Vec<String> {
    let mut symbols = raw
        .split(',')
        .map(|value| value.trim().to_ascii_uppercase())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    symbols.sort();
    symbols.dedup();
    symbols
}

fn import_many(
    config: &BatchImportConfig,
) -> Result<BinancePublicImportReport, Box<dyn std::error::Error>> {
    let mut total = BinancePublicImportReport {
        db_path: sandbox_quant::record::coordination::RecorderCoordination::new(
            config.base_dir.clone(),
        )
        .db_path(config.mode)
        .display()
        .to_string(),
        dates_requested: 0,
        dates_with_imports: 0,
        skipped_liquidation_dates: 0,
        skipped_kline_dates: 0,
        liquidation_rows: 0,
        kline_rows: 0,
    };

    for product in &config.products {
        for symbol in &config.symbols {
            let report = import_binance_public_data(&BinancePublicImportConfig {
                product: *product,
                symbol: symbol.clone(),
                from: config.from,
                to: config.to,
                kline_interval: config.kline_interval.clone(),
                import_liquidation: config.import_liquidation,
                import_klines: config.import_klines,
                mode: config.mode,
                base_dir: config.base_dir.clone(),
            })?;
            total.dates_requested += report.dates_requested;
            total.dates_with_imports += report.dates_with_imports;
            total.skipped_liquidation_dates += report.skipped_liquidation_dates;
            total.skipped_kline_dates += report.skipped_kline_dates;
            total.liquidation_rows += report.liquidation_rows;
            total.kline_rows += report.kline_rows;
        }
    }

    Ok(total)
}

fn join_products(products: &[BinancePublicProduct]) -> String {
    products
        .iter()
        .map(|product| product.as_str())
        .collect::<Vec<_>>()
        .join(",")
}

fn parse_summary_args(
    args: &[String],
) -> Result<(BinanceMode, String), Box<dyn std::error::Error>> {
    let mut mode = BinanceMode::Demo;
    let mut base_dir = "var".to_string();
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--mode" => {
                let value = args.get(index + 1).ok_or("missing value for --mode")?;
                mode = match value.as_str() {
                    "demo" => BinanceMode::Demo,
                    "real" => BinanceMode::Real,
                    _ => return Err(format!("unsupported mode: {value}").into()),
                };
                index += 2;
            }
            "--base-dir" => {
                base_dir = args
                    .get(index + 1)
                    .ok_or("missing value for --base-dir")?
                    .clone();
                index += 2;
            }
            other => return Err(format!("unsupported arg: {other}").into()),
        }
    }
    Ok((mode, base_dir))
}

fn existing_schema_version(db_path: &std::path::Path) -> Option<String> {
    if !db_path.exists() {
        return None;
    }
    let connection = Connection::open(db_path).ok()?;
    let table_exists: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM information_schema.tables WHERE table_name = 'schema_metadata'",
            [],
            |row| row.get(0),
        )
        .ok()?;
    if table_exists == 0 {
        return None;
    }
    connection
        .query_row(
            "SELECT value FROM schema_metadata WHERE key = 'market_data_schema_version'",
            [],
            |row| row.get(0),
        )
        .ok()
}

fn schema_warning(previous_version: Option<&str>) -> Option<String> {
    match previous_version {
        None => Some(format!(
            "warning=schema_bootstrap_applied previous=missing current={}",
            MARKET_DATA_SCHEMA_VERSION
        )),
        Some(previous) if previous != MARKET_DATA_SCHEMA_VERSION => Some(format!(
            "warning=schema_version_updated previous={} current={}",
            previous, MARKET_DATA_SCHEMA_VERSION
        )),
        _ => None,
    }
}

fn render_summary(mode: BinanceMode, base_dir: &str) -> Result<String, Box<dyn std::error::Error>> {
    let db_path = RecorderCoordination::new(base_dir.to_string()).db_path(mode);
    let previous_schema_version = existing_schema_version(&db_path);
    init_schema_for_path(&db_path)?;
    let connection =
        Connection::open(&db_path).map_err(|error| StorageError::DatabaseInitFailed {
            path: db_path.display().to_string(),
            message: error.to_string(),
        })?;

    let mut lines = vec![
        "collector summary".to_string(),
        format!("mode={}", mode.as_str()),
        format!("db_path={}", db_path.display()),
        format!("schema_version={}", MARKET_DATA_SCHEMA_VERSION),
        format!(
            "schema_previous_version={}",
            previous_schema_version
                .clone()
                .unwrap_or_else(|| "missing".to_string())
        ),
    ];
    if let Some(warning) = schema_warning(previous_schema_version.as_deref()) {
        lines.push(warning);
    }

    let mut kline_statement = connection.prepare(
        "SELECT product, symbol, interval, COUNT(*) AS row_count,
                CAST(MIN(open_time) AS VARCHAR), CAST(MAX(close_time) AS VARCHAR)
         FROM raw_klines
         GROUP BY product, symbol, interval
         ORDER BY product, symbol, interval",
    )?;
    let mut kline_rows = kline_statement.query([])?;
    lines.push("klines".to_string());
    let mut has_klines = false;
    while let Some(row) = kline_rows.next()? {
        has_klines = true;
        let product: String = row.get(0)?;
        let symbol: String = row.get(1)?;
        let interval: String = row.get(2)?;
        let row_count: i64 = row.get(3)?;
        let min_time: Option<String> = row.get(4)?;
        let max_time: Option<String> = row.get(5)?;
        lines.push(format!(
            "product={} symbol={} interval={} rows={} from={} to={}",
            product,
            symbol,
            interval,
            row_count,
            min_time.unwrap_or_else(|| "n/a".to_string()),
            max_time.unwrap_or_else(|| "n/a".to_string())
        ));
    }
    if !has_klines {
        lines.push("klines=none".to_string());
    }

    let mut liquidation_statement = connection.prepare(
        "SELECT symbol, COUNT(*) AS row_count,
                CAST(MIN(event_time) AS VARCHAR), CAST(MAX(event_time) AS VARCHAR)
         FROM raw_liquidation_events
         GROUP BY symbol
         ORDER BY symbol",
    )?;
    let mut liquidation_rows = liquidation_statement.query([])?;
    lines.push("liquidations".to_string());
    let mut has_liquidations = false;
    while let Some(row) = liquidation_rows.next()? {
        has_liquidations = true;
        let symbol: String = row.get(0)?;
        let row_count: i64 = row.get(1)?;
        let min_time: Option<String> = row.get(2)?;
        let max_time: Option<String> = row.get(3)?;
        lines.push(format!(
            "symbol={} rows={} from={} to={}",
            symbol,
            row_count,
            min_time.unwrap_or_else(|| "n/a".to_string()),
            max_time.unwrap_or_else(|| "n/a".to_string())
        ));
    }
    if !has_liquidations {
        lines.push("liquidations=none".to_string());
    }

    Ok(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn temp_base_dir(name: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "sandbox_quant_{name}_{}_{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn render_summary_bootstraps_missing_collector_tables_for_legacy_db() {
        let base_dir = temp_base_dir("collector_summary_legacy");
        let db_path = RecorderCoordination::new(base_dir.clone()).db_path(BinanceMode::Demo);
        let connection = Connection::open(&db_path).expect("open db");
        connection
            .execute_batch(
                r#"
                CREATE TABLE raw_liquidation_events (
                  event_id BIGINT,
                  mode VARCHAR NOT NULL,
                  symbol VARCHAR NOT NULL,
                  event_time TIMESTAMP NOT NULL,
                  receive_time TIMESTAMP NOT NULL,
                  force_side VARCHAR NOT NULL,
                  price DOUBLE NOT NULL,
                  qty DOUBLE NOT NULL,
                  notional DOUBLE NOT NULL,
                  raw_payload VARCHAR NOT NULL
                );
                INSERT INTO raw_liquidation_events VALUES (
                  1, 'demo', 'BTCUSDT',
                  CAST('2026-03-13 00:00:00' AS TIMESTAMP),
                  CAST('2026-03-13 00:00:01' AS TIMESTAMP),
                  'SELL', 100000, 1, 100000, '{}');
                "#,
            )
            .expect("seed legacy schema");
        drop(connection);

        let summary =
            render_summary(BinanceMode::Demo, base_dir.to_str().unwrap()).expect("summary");
        assert!(summary.contains("schema_version=1"), "{summary}");
        assert!(
            summary.contains("schema_previous_version=missing"),
            "{summary}"
        );
        assert!(
            summary.contains("warning=schema_bootstrap_applied previous=missing current=1"),
            "{summary}"
        );
        assert!(summary.contains("klines=none"), "{summary}");
        assert!(summary.contains("symbol=BTCUSDT rows=1"), "{summary}");

        let bootstrap_connection = Connection::open(&db_path).expect("reopen db");
        let exists: i64 = bootstrap_connection
            .query_row(
                "SELECT COUNT(*) FROM information_schema.tables WHERE table_name = 'raw_klines'",
                [],
                |row| row.get(0),
            )
            .expect("query table existence");
        assert_eq!(exists, 1);

        fs::remove_dir_all(base_dir).ok();
    }

    #[test]
    fn schema_warning_is_none_when_version_matches() {
        assert_eq!(schema_warning(Some(MARKET_DATA_SCHEMA_VERSION)), None);
    }
}
