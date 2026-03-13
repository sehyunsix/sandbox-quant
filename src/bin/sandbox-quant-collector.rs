use chrono::NaiveDate;
use sandbox_quant::app::bootstrap::BinanceMode;
use sandbox_quant::collector_app::binance_public::{
    import_binance_public_data, BinanceFuturesProduct, BinancePublicImportConfig,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("binance-public") if args.get(1).map(String::as_str) == Some("import") => {
            let config = parse_import_args(&args[2..])?;
            let report = import_binance_public_data(&config)?;
            println!(
                "{}",
                [
                    "collector import".to_string(),
                    format!("product={}", config.product.as_str()),
                    format!("symbol={}", config.symbol),
                    format!("date={}", config.date),
                    format!("mode={}", config.mode.as_str()),
                    format!("db_path={}", report.db_path),
                    format!("liquidation_rows={}", report.liquidation_rows),
                    format!("kline_rows={}", report.kline_rows),
                ]
                .join("\n")
            );
        }
        _ => {
            eprintln!(
                "usage: sandbox-quant-collector binance-public import --product <um|cm> --symbol <symbol> --date <YYYY-MM-DD> [--kline-interval <interval>] [--mode <demo|real>] [--base-dir <path>] [--skip-liquidation] [--skip-klines]"
            );
            std::process::exit(2);
        }
    }
    Ok(())
}

fn parse_import_args(
    args: &[String],
) -> Result<BinancePublicImportConfig, Box<dyn std::error::Error>> {
    let mut product = None;
    let mut symbol = None;
    let mut date = None;
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
                product = Some(match value.as_str() {
                    "um" => BinanceFuturesProduct::Um,
                    "cm" => BinanceFuturesProduct::Cm,
                    _ => return Err(format!("unsupported product: {value}").into()),
                });
                index += 2;
            }
            "--symbol" => {
                let value = args.get(index + 1).ok_or("missing value for --symbol")?;
                symbol = Some(value.trim().to_ascii_uppercase());
                index += 2;
            }
            "--date" => {
                let value = args.get(index + 1).ok_or("missing value for --date")?;
                date = Some(NaiveDate::parse_from_str(value, "%Y-%m-%d")?);
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
    Ok(BinancePublicImportConfig {
        product: product.ok_or("missing --product")?,
        symbol: symbol.ok_or("missing --symbol")?,
        date: date.ok_or("missing --date")?,
        kline_interval,
        import_liquidation,
        import_klines,
        mode,
        base_dir,
    })
}
