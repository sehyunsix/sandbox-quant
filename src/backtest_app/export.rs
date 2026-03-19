use crate::backtest_app::runner::BacktestReport;
use crate::error::storage_error::StorageError;
use crate::storage::postgres_market_data::{
    connect as connect_postgres, export_backtest_report, init_schema as init_postgres_schema,
    postgres_url_from_env,
};

pub fn maybe_export_report_to_postgres(
    report: &BacktestReport,
) -> Result<Option<i64>, StorageError> {
    if !postgres_export_enabled() {
        return Ok(None);
    }
    let postgres_url = postgres_url_from_env()?;
    export_report_to_postgres(report, &postgres_url).map(Some)
}

pub fn export_report_to_postgres(
    report: &BacktestReport,
    postgres_url: &str,
) -> Result<i64, StorageError> {
    let mut client = connect_postgres(postgres_url)?;
    let _ = init_postgres_schema(&mut client, postgres_url)?;
    export_backtest_report(&mut client, report)
}

fn postgres_export_enabled() -> bool {
    std::env::var("SANDBOX_QUANT_BACKTEST_EXPORT_POSTGRES")
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
        || std::env::var("SANDBOX_QUANT_POSTGRES_URL").is_ok()
        || std::env::var("DATABASE_URL").is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postgres_export_enables_with_explicit_flag() {
        std::env::remove_var("SANDBOX_QUANT_POSTGRES_URL");
        std::env::remove_var("DATABASE_URL");
        std::env::set_var("SANDBOX_QUANT_BACKTEST_EXPORT_POSTGRES", "1");

        assert!(postgres_export_enabled());

        std::env::remove_var("SANDBOX_QUANT_BACKTEST_EXPORT_POSTGRES");
    }
}
