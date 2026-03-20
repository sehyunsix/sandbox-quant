use chrono::{NaiveDate, Utc};
use duckdb::Connection;
use sandbox_quant::app::bootstrap::BinanceMode;
use sandbox_quant::collector_app::binance_public::{
    import_binance_public_data, BinancePublicImportConfig, BinancePublicImportReport,
    BinancePublicProduct,
};
use sandbox_quant::dataset::schema::{init_schema_for_path, MARKET_DATA_SCHEMA_VERSION};
use sandbox_quant::error::storage_error::StorageError;
use sandbox_quant::observability::logging::init_logging;
use sandbox_quant::record::coordination::RecorderCoordination;
use sandbox_quant::storage::postgres_market_data::{
    connect as connect_postgres, export_snapshot_to_duckdb, init_schema as init_postgres_schema,
    load_summary as load_postgres_summary, postgres_url_from_env, CollectorStorageBackend,
    PostgresToDuckDbSnapshotConfig,
};
use tracing::{error, info};

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
    storage_backend: CollectorStorageBackend,
    postgres_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
struct SnapshotConfig {
    mode: BinanceMode,
    base_dir: String,
    symbols: Vec<String>,
    from: NaiveDate,
    to: NaiveDate,
    product: Option<String>,
    interval_name: Option<String>,
    include_klines: bool,
    include_liquidations: bool,
    include_book_tickers: bool,
    include_agg_trades: bool,
    clear_duckdb_range: bool,
    postgres_url: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let init_mode = args
        .windows(2)
        .find(|window| window[0] == "--mode")
        .map(|window| window[1].as_str())
        .unwrap_or("demo");
    init_logging("collector", Some(init_mode))?;
    info!(service = "collector", mode = init_mode, args = ?args, "process started");

    let result: Result<(), Box<dyn std::error::Error>> = match args.first().map(String::as_str) {
        Some("binance-public") if args.get(1).map(String::as_str) == Some("import") => {
            let config = parse_import_args(&args[2..])?;
            let report = import_many(&config)?;
            info!(
                service = "collector",
                mode = config.mode.as_str(),
                storage = config.storage_backend.as_str(),
                symbols = %config.symbols.join(","),
                liquidation_rows = report.liquidation_rows,
                kline_rows = report.kline_rows,
                "collector import completed"
            );
            println!(
                "{}",
                {
                    let mut lines = vec![
                    "collector import".to_string(),
                    format!("storage={}", config.storage_backend.as_str()),
                    format!("products={}", join_products(&config.products)),
                    format!("symbols={}", config.symbols.join(",")),
                    format!("from={}", config.from),
                    format!("to={}", config.to),
                    format!("mode={}", config.mode.as_str()),
                    format!("target={}", report.db_path),
                    format!("dates_requested={}", report.dates_requested),
                    format!("dates_with_imports={}", report.dates_with_imports),
                    format!(
                        "skipped_liquidation_dates={}",
                        report.skipped_liquidation_dates
                    ),
                    format!("skipped_kline_dates={}", report.skipped_kline_dates),
                    format!("liquidation_rows={}", report.liquidation_rows),
                    format!("kline_rows={}", report.kline_rows),
                    ];
                    if let Some(warning) = archive_gap_warning(&config, &report) {
                        lines.push(warning);
                    }
                    lines.join("\n")
                }
            );
            Ok(())
        }
        Some("summary") => {
            let (mode, base_dir, storage_backend, postgres_url) = parse_summary_args(&args[1..])?;
            println!(
                "{}",
                render_summary(mode, &base_dir, storage_backend, postgres_url.as_deref())?
            );
            info!(service = "collector", mode = mode.as_str(), storage = storage_backend.as_str(), "collector summary completed");
            Ok(())
        }
        Some("snapshot") if args.get(1).map(String::as_str) == Some("postgres-to-duckdb") => {
            let config = parse_snapshot_args(&args[2..])?;
            let report = export_snapshot_to_duckdb(&PostgresToDuckDbSnapshotConfig {
                postgres_url: config.postgres_url.clone(),
                mode: config.mode,
                base_dir: config.base_dir.clone(),
                symbols: config.symbols.clone(),
                from: config.from,
                to: config.to,
                product: config.product.clone(),
                interval_name: config.interval_name.clone(),
                include_klines: config.include_klines,
                include_liquidations: config.include_liquidations,
                include_book_tickers: config.include_book_tickers,
                include_agg_trades: config.include_agg_trades,
                clear_duckdb_range: config.clear_duckdb_range,
            })?;
            println!(
                "{}",
                [
                    "collector snapshot".to_string(),
                    "source=postgres".to_string(),
                    format!("snapshot_export_id={}", report.snapshot_export_id),
                    format!("target={}", report.db_path),
                    format!("mode={}", config.mode.as_str()),
                    format!("symbols={}", config.symbols.join(",")),
                    format!("from={}", config.from),
                    format!("to={}", config.to),
                    format!("product={}", config.product.as_deref().unwrap_or("any")),
                    format!(
                        "interval={}",
                        config.interval_name.as_deref().unwrap_or("any")
                    ),
                    format!("include_klines={}", config.include_klines),
                    format!("include_liquidations={}", config.include_liquidations),
                    format!("include_book_tickers={}", config.include_book_tickers),
                    format!("include_agg_trades={}", config.include_agg_trades),
                    format!("clear_duckdb_range={}", config.clear_duckdb_range),
                    format!("kline_rows={}", report.kline_rows),
                    format!("liquidation_rows={}", report.liquidation_rows),
                    format!("book_ticker_rows={}", report.book_ticker_rows),
                    format!("agg_trade_rows={}", report.agg_trade_rows),
                ]
                .join("\n")
            );
            info!(
                service = "collector",
                mode = config.mode.as_str(),
                symbols = %config.symbols.join(","),
                kline_rows = report.kline_rows,
                liquidation_rows = report.liquidation_rows,
                book_ticker_rows = report.book_ticker_rows,
                agg_trade_rows = report.agg_trade_rows,
                "collector snapshot completed"
            );
            Ok(())
        }
        _ => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "usage: sandbox-quant-collector binance-public import (--product <spot|um|cm> | --products <spot,um,cm>) (--symbol <symbol> | --symbols <a,b,c>) (--date <YYYY-MM-DD> | --from <YYYY-MM-DD> --to <YYYY-MM-DD>) [--kline-interval <interval>] [--mode <demo|real>] [--base-dir <path>] [--skip-liquidation] [--skip-klines] [--storage <duckdb|postgres>] [--postgres-url <url>]\n       sandbox-quant-collector summary [--mode <demo|real>] [--base-dir <path>] [--storage <duckdb|postgres>] [--postgres-url <url>]\n       sandbox-quant-collector snapshot postgres-to-duckdb (--symbol <symbol> | --symbols <a,b,c>) (--date <YYYY-MM-DD> | --from <YYYY-MM-DD> --to <YYYY-MM-DD>) [--mode <demo|real>] [--base-dir <path>] [--product <spot|um|cm>] [--interval <interval>] [--postgres-url <url>] [--skip-klines] [--skip-liquidations] [--skip-book-tickers] [--skip-agg-trades] [--no-clear]",
        )
        .into()),
    };
    if let Err(error) = result {
        error!(service = "collector", mode = init_mode, error = %error, "process failed");
        return Err(error);
    }
    info!(service = "collector", mode = init_mode, "process completed");
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
    let mut storage_backend = CollectorStorageBackend::DuckDb;
    let mut postgres_url = None;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--product" | "--products" => {
                let value = args
                    .get(index + 1)
                    .ok_or(format!("missing value for {}", args[index]))?;
                products = Some(parse_products(value)?);
                index += 2;
            }
            "--symbol" | "--symbols" => {
                let value = args
                    .get(index + 1)
                    .ok_or(format!("missing value for {}", args[index]))?;
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
                mode = parse_mode(
                    args.get(index + 1)
                        .ok_or("missing value for --mode")?
                        .as_str(),
                )?;
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
            "--storage" => {
                storage_backend = parse_storage_backend(
                    args.get(index + 1)
                        .ok_or("missing value for --storage")?
                        .as_str(),
                )?;
                index += 2;
            }
            "--postgres-url" => {
                postgres_url = Some(
                    args.get(index + 1)
                        .ok_or("missing value for --postgres-url")?
                        .clone(),
                );
                index += 2;
            }
            other => return Err(format!("unsupported arg: {other}").into()),
        }
    }
    let (from, to) = resolve_date_range(date, from, to)?;
    let products = products.ok_or("missing --product or --products")?;
    let symbols = symbols.ok_or("missing --symbol or --symbols")?;
    let postgres_url = resolve_postgres_url(storage_backend, postgres_url)?;
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
        storage_backend,
        postgres_url,
    })
}

fn parse_summary_args(
    args: &[String],
) -> Result<
    (BinanceMode, String, CollectorStorageBackend, Option<String>),
    Box<dyn std::error::Error>,
> {
    let mut mode = BinanceMode::Demo;
    let mut base_dir = "var".to_string();
    let mut storage_backend = CollectorStorageBackend::DuckDb;
    let mut postgres_url = None;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--mode" => {
                mode = parse_mode(
                    args.get(index + 1)
                        .ok_or("missing value for --mode")?
                        .as_str(),
                )?;
                index += 2;
            }
            "--base-dir" => {
                base_dir = args
                    .get(index + 1)
                    .ok_or("missing value for --base-dir")?
                    .clone();
                index += 2;
            }
            "--storage" => {
                storage_backend = parse_storage_backend(
                    args.get(index + 1)
                        .ok_or("missing value for --storage")?
                        .as_str(),
                )?;
                index += 2;
            }
            "--postgres-url" => {
                postgres_url = Some(
                    args.get(index + 1)
                        .ok_or("missing value for --postgres-url")?
                        .clone(),
                );
                index += 2;
            }
            other => return Err(format!("unsupported arg: {other}").into()),
        }
    }
    Ok((
        mode,
        base_dir,
        storage_backend,
        resolve_postgres_url(storage_backend, postgres_url)?,
    ))
}

fn parse_snapshot_args(args: &[String]) -> Result<SnapshotConfig, Box<dyn std::error::Error>> {
    let mut mode = BinanceMode::Demo;
    let mut base_dir = "var".to_string();
    let mut symbols = None;
    let mut date = None;
    let mut from = None;
    let mut to = None;
    let mut product = None;
    let mut interval_name = None;
    let mut include_klines = true;
    let mut include_liquidations = true;
    let mut include_book_tickers = true;
    let mut include_agg_trades = true;
    let mut clear_duckdb_range = true;
    let mut postgres_url = None;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--symbol" | "--symbols" => {
                let value = args
                    .get(index + 1)
                    .ok_or(format!("missing value for {}", args[index]))?;
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
            "--mode" => {
                mode = parse_mode(
                    args.get(index + 1)
                        .ok_or("missing value for --mode")?
                        .as_str(),
                )?;
                index += 2;
            }
            "--base-dir" => {
                base_dir = args
                    .get(index + 1)
                    .ok_or("missing value for --base-dir")?
                    .clone();
                index += 2;
            }
            "--product" => {
                product = Some(
                    args.get(index + 1)
                        .ok_or("missing value for --product")?
                        .clone(),
                );
                index += 2;
            }
            "--interval" => {
                interval_name = Some(
                    args.get(index + 1)
                        .ok_or("missing value for --interval")?
                        .clone(),
                );
                index += 2;
            }
            "--skip-klines" => {
                include_klines = false;
                index += 1;
            }
            "--skip-liquidations" => {
                include_liquidations = false;
                index += 1;
            }
            "--skip-book-tickers" => {
                include_book_tickers = false;
                index += 1;
            }
            "--skip-agg-trades" => {
                include_agg_trades = false;
                index += 1;
            }
            "--no-clear" => {
                clear_duckdb_range = false;
                index += 1;
            }
            "--postgres-url" => {
                postgres_url = Some(
                    args.get(index + 1)
                        .ok_or("missing value for --postgres-url")?
                        .clone(),
                );
                index += 2;
            }
            other => return Err(format!("unsupported arg: {other}").into()),
        }
    }
    let (from, to) = resolve_date_range(date, from, to)?;
    Ok(SnapshotConfig {
        mode,
        base_dir,
        symbols: symbols.ok_or("missing --symbol or --symbols")?,
        from,
        to,
        product,
        interval_name,
        include_klines,
        include_liquidations,
        include_book_tickers,
        include_agg_trades,
        clear_duckdb_range,
        postgres_url: resolve_postgres_url(CollectorStorageBackend::Postgres, postgres_url)?
            .expect("postgres URL required by resolver"),
    })
}

fn parse_mode(value: &str) -> Result<BinanceMode, Box<dyn std::error::Error>> {
    match value {
        "demo" => Ok(BinanceMode::Demo),
        "real" => Ok(BinanceMode::Real),
        other => Err(format!("unsupported mode: {other}").into()),
    }
}

fn parse_storage_backend(
    value: &str,
) -> Result<CollectorStorageBackend, Box<dyn std::error::Error>> {
    match value {
        "duckdb" => Ok(CollectorStorageBackend::DuckDb),
        "postgres" => Ok(CollectorStorageBackend::Postgres),
        other => Err(format!("unsupported storage backend: {other}").into()),
    }
}

fn resolve_postgres_url(
    backend: CollectorStorageBackend,
    postgres_url: Option<String>,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    match backend {
        CollectorStorageBackend::DuckDb => Ok(postgres_url),
        CollectorStorageBackend::Postgres => match postgres_url {
            Some(url) => Ok(Some(url)),
            None => Ok(Some(postgres_url_from_env()?)),
        },
    }
}

fn resolve_date_range(
    date: Option<NaiveDate>,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
) -> Result<(NaiveDate, NaiveDate), Box<dyn std::error::Error>> {
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
    Ok((from, to))
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
        db_path: match config.storage_backend {
            CollectorStorageBackend::DuckDb => RecorderCoordination::new(config.base_dir.clone())
                .db_path(config.mode)
                .display()
                .to_string(),
            CollectorStorageBackend::Postgres => config
                .postgres_url
                .clone()
                .unwrap_or_else(|| "postgres://***".to_string()),
        },
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
                storage_backend: config.storage_backend,
                postgres_url: config.postgres_url.clone(),
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

fn archive_gap_warning(
    config: &BatchImportConfig,
    report: &BinancePublicImportReport,
) -> Option<String> {
    if config.storage_backend != CollectorStorageBackend::Postgres {
        return None;
    }
    if report.kline_rows > 0 || report.skipped_kline_dates == 0 {
        return None;
    }
    let today_utc = Utc::now().date_naive();
    if config.to < today_utc {
        return None;
    }
    Some(format!(
        "warning=current_day_archive_not_available use=postgres_kline_backfill from={} to={} today_utc={}",
        config.from, config.to, today_utc
    ))
}

fn join_products(products: &[BinancePublicProduct]) -> String {
    products
        .iter()
        .map(|product| product.as_str())
        .collect::<Vec<_>>()
        .join(",")
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

fn render_summary(
    mode: BinanceMode,
    base_dir: &str,
    storage_backend: CollectorStorageBackend,
    postgres_url: Option<&str>,
) -> Result<String, Box<dyn std::error::Error>> {
    match storage_backend {
        CollectorStorageBackend::DuckDb => render_duckdb_summary(mode, base_dir),
        CollectorStorageBackend::Postgres => {
            let postgres_url = postgres_url.ok_or("postgres summary requires postgres_url")?;
            let mut client = connect_postgres(postgres_url)?;
            let previous_version = init_postgres_schema(&mut client, postgres_url)?;
            let summary = load_postgres_summary(&mut client, previous_version.clone())?;

            let mut lines = vec![
                "collector summary".to_string(),
                "storage=postgres".to_string(),
                format!("mode={}", mode.as_str()),
                format!("target={}", postgres_url),
                format!("schema_version={}", summary.schema_version),
                format!(
                    "schema_previous_version={}",
                    previous_version
                        .clone()
                        .unwrap_or_else(|| "missing".to_string())
                ),
            ];
            if let Some(warning) = schema_warning(previous_version.as_deref()) {
                lines.push(warning);
            }
            lines.push("klines".to_string());
            if summary.klines.is_empty() {
                lines.push("klines=none".to_string());
            } else {
                for row in summary.klines {
                    lines.push(format!(
                        "product={} symbol={} interval={} rows={} from={} to={}",
                        row.product,
                        row.symbol,
                        row.interval_name,
                        row.row_count,
                        row.min_time.unwrap_or_else(|| "n/a".to_string()),
                        row.max_time.unwrap_or_else(|| "n/a".to_string()),
                    ));
                }
            }
            lines.push("liquidations".to_string());
            if summary.liquidations.is_empty() {
                lines.push("liquidations=none".to_string());
            } else {
                for row in summary.liquidations {
                    lines.push(format!(
                        "symbol={} rows={} from={} to={}",
                        row.symbol,
                        row.row_count,
                        row.min_time.unwrap_or_else(|| "n/a".to_string()),
                        row.max_time.unwrap_or_else(|| "n/a".to_string()),
                    ));
                }
            }
            Ok(lines.join("\n"))
        }
    }
}

fn render_duckdb_summary(
    mode: BinanceMode,
    base_dir: &str,
) -> Result<String, Box<dyn std::error::Error>> {
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
        "storage=duckdb".to_string(),
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
    fn parse_import_args_supports_postgres_storage() {
        let config = parse_import_args(&[
            "--products".to_string(),
            "um".to_string(),
            "--symbols".to_string(),
            "BTCUSDT,ETHUSDT".to_string(),
            "--from".to_string(),
            "2026-01-01".to_string(),
            "--to".to_string(),
            "2026-01-31".to_string(),
            "--storage".to_string(),
            "postgres".to_string(),
            "--postgres-url".to_string(),
            "postgres://localhost/test".to_string(),
        ])
        .expect("parse");
        assert_eq!(config.storage_backend, CollectorStorageBackend::Postgres);
        assert_eq!(
            config.postgres_url.as_deref(),
            Some("postgres://localhost/test")
        );
    }

    #[test]
    fn parse_snapshot_args_builds_expected_config() {
        let config = parse_snapshot_args(&[
            "--symbols".to_string(),
            "BTCUSDT,ETHUSDT".to_string(),
            "--from".to_string(),
            "2026-01-01".to_string(),
            "--to".to_string(),
            "2026-01-31".to_string(),
            "--postgres-url".to_string(),
            "postgres://localhost/test".to_string(),
            "--interval".to_string(),
            "15m".to_string(),
            "--skip-liquidations".to_string(),
        ])
        .expect("parse");
        assert_eq!(
            config.symbols,
            vec!["BTCUSDT".to_string(), "ETHUSDT".to_string()]
        );
        assert_eq!(config.interval_name.as_deref(), Some("15m"));
        assert!(config.include_klines);
        assert!(!config.include_liquidations);
        assert!(config.include_book_tickers);
        assert!(config.include_agg_trades);
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

        let summary = render_summary(
            BinanceMode::Demo,
            base_dir.to_str().unwrap(),
            CollectorStorageBackend::DuckDb,
            None,
        )
        .expect("summary");
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
}
