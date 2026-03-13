use chrono::NaiveDate;
use sandbox_quant::app::bootstrap::BinanceMode;
use sandbox_quant::collector_app::binance_public::{
    import_binance_public_data, BinancePublicImportConfig, BinancePublicImportReport,
    BinancePublicProduct,
};

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
        _ => {
            eprintln!(
                "usage: sandbox-quant-collector binance-public import (--product <spot|um|cm> | --products <spot,um,cm>) (--symbol <symbol> | --symbols <a,b,c>) (--date <YYYY-MM-DD> | --from <YYYY-MM-DD> --to <YYYY-MM-DD>) [--kline-interval <interval>] [--mode <demo|real>] [--base-dir <path>] [--skip-liquidation] [--skip-klines]"
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
